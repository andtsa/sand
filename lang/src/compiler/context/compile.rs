//! The compilation context, threaded through every compiler pass.

use std::cell::Cell;

use pest::iterators::Pair;
use thiserror::Error;

use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::structure::CodeModule;
use crate::compiler::structure::EnumDef;
use crate::compiler::structure::EnumVariant;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleInfo;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::OriginalFun;
use crate::compiler::structure::OriginalVar;
use crate::compiler::structure::OriginalVarRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::RegionParam;
use crate::compiler::structure::RegionParamSpec;
use crate::compiler::structure::Set;
use crate::compiler::structure::TypeParam;
use crate::compiler::structure::TypeParamSpec;
use crate::compiler::structure::UniqVar;
use crate::compiler::structure::VarName;
use crate::internal_bug;
use crate::ir_types::hhir::HirVar;
use crate::lang::types::CommonTypes;
use crate::lang::types::EnumRef;
use crate::lang::types::Kind;
use crate::lang::types::Region;
use crate::lang::types::RegionConstraint;
use crate::lang::types::RegionVar;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::lang::types::TypeParamId;
use crate::passes::parse::Rule;

/// This should not be used and is intentionally misspelled to be easily
/// detectable.
const DEFAULT_MODULE_NAME: &str = "mAin";

// ============================= Arena =========================================

/// Backing store for all arena-allocated compiler data.
///
/// this is opaque by design, since only [`CompileCtx`] internals allocate
/// through it. to swap the allocators we only need to change this struct and
/// its methods.
///
/// `bump` holds `Copy`, destructor-free type data ([`TyKind`]). The
/// [`typed_arena::Arena`]s hold owning data ([`OriginalFun`], [`CodeModule`],
/// [`EnumDef`], [`OriginalVar`]) whose `String`/`Vec` fields must have their
/// destructors run when the arena is dropped, since `bumpalo` would leak them.
struct Arenas {
    bump: bumpalo::Bump,
    functions: typed_arena::Arena<OriginalFun<'static>>,
    modules: typed_arena::Arena<CodeModule>,
    enums: typed_arena::Arena<EnumDef<'static>>,
    variables: typed_arena::Arena<OriginalVar>,
}

/// Safety: after the initial compilation phase, the arena is never mutated
/// again; only existing allocations are read. `bumpalo::Bump` and
/// `typed_arena::Arena` both use `Cell<>` internally, which makes them `!Sync`
/// to prevent concurrent *writes*, but since the LSP and other multi-threaded
/// users only read after compilation, sharing `Arenas` across threads is sound.
///
/// todo: create a new struct `RoArena` that takes ownership of the inner arenas
/// after compilation has finished, and implement send + sync on that
unsafe impl Send for Arenas {}
unsafe impl Sync for Arenas {}

impl Arenas {
    fn new() -> Self {
        Self {
            bump: bumpalo::Bump::new(),
            functions: typed_arena::Arena::new(),
            modules: typed_arena::Arena::new(),
            enums: typed_arena::Arena::new(),
            variables: typed_arena::Arena::new(),
        }
    }

    fn alloc_ty<'tcx>(&'tcx self, kind: TyKind<'tcx>) -> &'tcx TyKind<'tcx> {
        self.bump.alloc(kind)
    }

    fn alloc_ty_slice<'tcx>(&'tcx self, tys: &[Ty<'tcx>]) -> &'tcx [Ty<'tcx>] {
        self.bump.alloc_slice_copy(tys)
    }

    // The `typed_arena` allocators are invariant in their element lifetime, so
    // we store them as `'static` and transmute the borrow to `'tcx` on the way
    // out. This is sound: the returned reference cannot outlive `&'tcx self`,
    // and every `'tcx` value stored inside (e.g. `ModuleRef<'tcx>`) is itself
    // an arena reference with the same provenance.

    fn alloc_function<'tcx>(&'tcx self, f: OriginalFun<'tcx>) -> &'tcx OriginalFun<'tcx> {
        let f: OriginalFun<'static> = unsafe { std::mem::transmute(f) };
        let r: &'tcx OriginalFun<'static> = self.functions.alloc(f);
        unsafe { std::mem::transmute(r) }
    }

    fn alloc_module(&self, m: CodeModule) -> &CodeModule {
        self.modules.alloc(m)
    }

    fn alloc_enum<'tcx>(&'tcx self, e: EnumDef<'tcx>) -> &'tcx EnumDef<'tcx> {
        let e: EnumDef<'static> = unsafe { std::mem::transmute(e) };
        let r: &'tcx EnumDef<'static> = self.enums.alloc(e);
        unsafe { std::mem::transmute(r) }
    }

    fn alloc_variable(&self, v: OriginalVar) -> &OriginalVar {
        self.variables.alloc(v)
    }
}

// ============================= Context =======================================

pub struct CompileCtx<'tcx> {
    arenas: &'tcx Arenas,
    /// Whether this `CompileCtx` owns (and must free) the arena it points to.
    /// Set to `true` only by [`CompileCtx::initial`]; all other constructors
    /// borrow an arena owned elsewhere and leave this `false`.
    owns_arena: bool,

    /// Pre-interned handles for the four primitive types.
    pub types: CommonTypes<'tcx>,
    /// Interner for all non-tuple types (keyed by structural content).
    ty_interner: Map<TyKind<'tcx>, Ty<'tcx>>,
    /// Separate interner for tuple types, keyed by element lists.
    tuple_interner: Map<Vec<Ty<'tcx>>, Ty<'tcx>>,
    /// Interner for generic enum instantiations, keyed by base enum + args.
    app_interner: Map<(EnumRef<'tcx>, Vec<Ty<'tcx>>), Ty<'tcx>>,
    /// Interner for region-ascribed types `T @ 'r`, keyed by inner type +
    /// region.
    region_ty_interner: Map<(Ty<'tcx>, Region), Ty<'tcx>>,

    // regions
    /// Number of region variables allocated so far.
    region_count: usize,
    /// Name → variable for the region parameters in scope while the current
    /// declaration is being built (set by [`Self::begin_region_params`]).
    /// Named regions (`&'r T`, `T @ 'r`) resolve against this scope, so a
    /// region must be declared (`def f<'r>(...)`) to be referenced.
    cur_regions: Map<String, RegionVar>,
    /// The single region shared by all elided borrows (`&T`/`&e` with no
    /// lifetime), allocated lazily. Shared so that an elided `&T` type and an
    /// elided `&e` value compare equal at the type level; the borrow's actual
    /// scope is carried in its `Kind` (`Borrowed(region)`) for the escape
    /// check.
    anon_region_var: Option<RegionVar>,
    /// Stack of region variables for the lexically-nested scopes (the function,
    /// then each enclosing block) currently being type-checked (Step 8b). Used
    /// by the borrow escape check to compare lifetimes by nesting depth.
    region_scope_stack: Vec<RegionVar>,
    /// Nesting depth of each lexical scope region (0 = function scope, deeper =
    /// inner blocks). Regions absent here: region parameters, `'static`, the
    /// elided-borrow region, are treated as depth 0: outermost, never escaping
    region_depths: Map<RegionVar, usize>,
    /// The `where 'a >= 's` constraints of the function currently being
    /// type-checked. Used as *assumptions* when checking a callee's `where`
    /// clauses at a call site (a generic caller can discharge a callee
    /// constraint with its own). Set by `infer_function`, restored on exit.
    cur_where_constraints: Vec<RegionConstraint>,

    // type parameters
    /// Number of type parameters allocated so far.
    /// assigns each a globally unique [`TypeParamId`].
    type_param_count: usize,
    /// Display names of every allocated type parameter, keyed by id.
    type_param_names: Map<TypeParamId, String>,
    /// Name → id for the type parameters in scope while the current generic
    /// declaration is being built (set by [`Self::begin_type_params`]).
    cur_type_params: Map<String, TypeParamId>,

    // variables
    /// Number of original variables registered so far.
    /// assigns each a stable monotonic `id` used for ordering.
    var_count: usize,
    pub variable_usages: Map<OriginalVarRef<'tcx>, Set<Range>>,
    /// Number of uniquified variables registered so far.
    /// assigns each a stable `idx` distinguishing shadowing
    /// re-bindings of the same declaration.
    uniq_count: usize,

    // functions
    global_functions: Vec<FunRef<'tcx>>,
    function_signatures: Map<FunRef<'tcx>, FunSig<'tcx>>,
    pub entrypoint: Option<FunRef<'tcx>>,

    // enums
    enum_defs: Vec<EnumRef<'tcx>>,
    enum_names: Map<String, EnumRef<'tcx>>,
    /// Interner for ad-hoc tag-union types, keyed by sorted tag list.
    anon_tag_types: Map<Vec<String>, EnumRef<'tcx>>,
    /// The module currently being built (set in `build_function`).
    cur_build_module: Option<ModuleRef<'tcx>>,

    // modules
    project_modules: Vec<ModuleRef<'tcx>>,

    // defaults
    file_defaults: Map<FileRef, ModuleRef<'tcx>>,
    default_module: Option<ModuleRef<'tcx>>,

    // diagnostics
    pub diagnostics: Vec<SandDiagnostic>,
}

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("use of undeclared variable: {name} at {range}")]
    UndeclaredVariable { name: VarName, range: Range },

    #[error("cannot register variable with rule {rule:?}")]
    IllegalVariableRegistration { rule: Rule },

    #[error("cannot register function with rule {rule:?}")]
    IllegalFunctionRegistration { rule: Rule },

    #[error("duplicate enum type '{name}'")]
    DuplicateEnum {
        name: String,
        first: Range,
        second: Range,
    },
}

#[derive(Debug)]
pub struct CtxEmptyError {}

impl CompileCtx<'static> {
    /// Create a compilation context backed by a freshly heap-allocated arena.
    ///
    /// The returned `CompileCtx` **owns** the arena: when it is dropped the
    /// arena is freed, which also reclaims every `Ty<'static>` and other
    /// arena-allocated value that was created through this context.
    ///
    /// # Safety invariant
    ///
    /// Every value that borrows from the arena (e.g. `TypedProgram<'static>`)
    /// must be dropped **before** this `CompileCtx` is dropped. The type
    /// system cannot enforce this when the lifetime is `'static`, so callers
    /// must uphold it manually. In practice, the `CheckResult` enum stores
    /// both in the same place and drops them together, so the invariant is
    /// trivially satisfied.
    pub fn initial() -> Self {
        let arenas: &'static Arenas = Box::leak(Box::new(Arenas::new()));
        let mut ctx = Self::with_arenas(arenas);
        ctx.owns_arena = true;
        ctx
    }
}

impl Drop for CompileCtx<'_> {
    fn drop(&mut self) {
        if self.owns_arena {
            // Safety: the arena was created by `Box::leak` in `initial()`.
            // We are the sole owner. Every value that borrows from the arena
            // (Ty<'tcx>, etc.) is `Copy` with a trivial drop, so there is no
            // use-after-free even if some arena-backed value is still
            // technically in scope at the point where `CompileCtx` is dropped
            // the final drop of such values never dereferences the pointer.
            unsafe {
                let _ = Box::from_raw(self.arenas as *const Arenas as *mut Arenas);
            }
        }
    }
}

impl<'tcx> CompileCtx<'tcx> {
    // The `ty_interner`/`tuple_interner`/`variable_usages` maps are keyed by
    // `TyKind`/`Ty`/`OriginalVarRef`, which reach an enum payload `Cell`
    // through arena references. clippy flags these as interior-mutable keys,
    // but they hash by structural/pointer identity that never reads the
    // `Cell`, so the keys are stable. Suppressed at the impl level since this
    // applies to every method that touches those maps.
    #[allow(clippy::mutable_key_type)]
    fn with_arenas(arenas: &'tcx Arenas) -> Self {
        // Intern the four primitive kinds directly, bypassing intern_ty, so
        // that ctx.types is populated before any other interning occurs.
        let int = Ty(arenas.alloc_ty(TyKind::Int));
        let bool_ = Ty(arenas.alloc_ty(TyKind::Bool));
        let unit = Ty(arenas.alloc_ty(TyKind::Unit));
        let top = Ty(arenas.alloc_ty(TyKind::Top));

        let mut ty_interner: Map<TyKind<'tcx>, Ty<'tcx>> = Map::default();
        ty_interner.insert(TyKind::Int, int);
        ty_interner.insert(TyKind::Bool, bool_);
        ty_interner.insert(TyKind::Unit, unit);
        ty_interner.insert(TyKind::Top, top);

        Self {
            arenas,
            owns_arena: false,
            types: CommonTypes {
                int,
                bool: bool_,
                unit,
                top,
            },
            ty_interner,
            tuple_interner: Default::default(),
            app_interner: Default::default(),
            region_ty_interner: Default::default(),
            region_count: 0,
            cur_regions: Default::default(),
            anon_region_var: None,
            region_scope_stack: Vec::new(),
            region_depths: Default::default(),
            cur_where_constraints: Vec::new(),
            type_param_count: 0,
            type_param_names: Default::default(),
            cur_type_params: Default::default(),
            var_count: 0,
            variable_usages: Default::default(),
            uniq_count: 0,
            global_functions: Default::default(),
            function_signatures: Default::default(),
            entrypoint: None,
            enum_defs: Default::default(),
            enum_names: Default::default(),
            anon_tag_types: Default::default(),
            cur_build_module: None,
            project_modules: Default::default(),
            default_module: None,
            file_defaults: Default::default(),
            diagnostics: Vec::new(),
        }
    }

    // ========================== Types ========================================

    /// Structurally intern a non-tuple [`TyKind`], returning the [`Ty`] handle
    /// that refers to it. Duplicate calls with identical structure return the
    /// same handle. Use [`Self::intern_tuple`] for `TyKind::Tuple`.
    fn intern_ty(&mut self, kind: TyKind<'tcx>) -> Ty<'tcx> {
        debug_assert!(
            !matches!(kind, TyKind::Tuple(_) | TyKind::App(..)),
            "use intern_tuple / intern_app for slice-bearing types"
        );
        if let Some(&ty) = self.ty_interner.get(&kind) {
            return ty;
        }
        let kind_ref = self.arenas.alloc_ty(kind);
        let ty = Ty(kind_ref);
        self.ty_interner.insert(kind, ty);
        ty
    }

    /// Intern a tuple type from its element handles. Arity must be >= 2.
    /// callers must route arity-0 to `ctx.types.unit` and arity-1 to the
    /// element type itself before calling this.
    pub fn intern_tuple(&mut self, elems: Vec<Ty<'tcx>>) -> Ty<'tcx> {
        debug_assert!(
            elems.len() >= 2,
            "tuple types must have arity >= 2, got {}",
            elems.len()
        );
        if let Some(&ty) = self.tuple_interner.get(&elems) {
            return ty;
        }
        let slice = self.arenas.alloc_ty_slice(&elems);
        let kind_ref = self.arenas.alloc_ty(TyKind::Tuple(slice));
        let ty = Ty(kind_ref);
        self.tuple_interner.insert(elems, ty);
        ty
    }

    /// Intern a generic enum instantiation `Base<args...>`. The argument count
    /// must match the base enum's declared type-parameter count (callers check
    /// arity and report a user-facing error). Distinct argument lists intern to
    /// distinct types.
    pub fn intern_app(&mut self, er: EnumRef<'tcx>, args: Vec<Ty<'tcx>>) -> Ty<'tcx> {
        let key = (er, args);
        if let Some(&ty) = self.app_interner.get(&key) {
            return ty;
        }
        let (er, args) = key;
        let slice = self.arenas.alloc_ty_slice(&args);
        let kind_ref = self.arenas.alloc_ty(TyKind::App(er, slice));
        let ty = Ty(kind_ref);
        self.app_interner.insert((er, args), ty);
        ty
    }

    /// # get the _kind of a type_
    /// returns the default kind for a value of that type
    /// (see Calculus§5 kinding judgment).
    /// A shared reference `&'r T` has kind `Borrowed` (`K-Borrow`); an
    /// exclusive reference `&'r mut T` has kind `BorrowedMut`
    /// (`K-BorrowMut`); everything else is `Owned`. The region lives on the
    /// *type*, not the kind.
    pub fn kind_of(&self, ty: Ty<'tcx>) -> Kind {
        match ty.kind() {
            TyKind::Ref(..) => Kind::Borrowed,
            TyKind::RefMut(..) => Kind::BorrowedMut,
            _ => Kind::Owned,
        }
    }

    // ============================ Regions ====================================

    /// Resolve a *declared* lifetime name to its [`Region`], or `None` if no
    /// region by that name is in scope. `'static` is always available; any
    /// other name must be a region parameter of the current declaration
    /// (`def f<'r>(...)`). Elided borrows do not go through here, instead they
    /// call [`Self::anon_region`] directly.
    pub fn resolve_region(&self, name: &str) -> Option<Region> {
        if name == "static" {
            return Some(Region::Static);
        }
        self.cur_regions.get(name).map(|&rv| Region::Var(rv))
    }

    /// Intern a region-ascribed type `inner @ region` (Calculus §2.3).
    pub fn region_ty(&mut self, inner: Ty<'tcx>, region: Region) -> Ty<'tcx> {
        let key = (inner, region);
        if let Some(&ty) = self.region_ty_interner.get(&key) {
            return ty;
        }
        let kind_ref = self.arenas.alloc_ty(TyKind::Region(inner, region));
        let ty = Ty(kind_ref);
        self.region_ty_interner.insert(key, ty);
        ty
    }

    /// Intern a shared reference type `&region inner` (Calculus §2.3).
    pub fn ref_ty(&mut self, region: Region, inner: Ty<'tcx>) -> Ty<'tcx> {
        self.intern_ty(TyKind::Ref(region, inner))
    }

    /// Intern an exclusive reference type `&region mut inner` (Calculus §2.3).
    pub fn ref_mut_ty(&mut self, region: Region, inner: Ty<'tcx>) -> Ty<'tcx> {
        self.intern_ty(TyKind::RefMut(region, inner))
    }

    /// Canonicalise the region of every reference (`&'r T`, `&'r mut T`) in
    /// `ty` to the shared anonymous region. Reference types carry no
    /// *type-level* region constraints — region safety is the lexical
    /// escape check, which reads the borrow's `Kind`, not its type — so at
    /// a call boundary a `&'r T` parameter accepts any `&_ T` argument
    /// (regions are inferred away). This is the type-checker's region
    /// inference; `T @ 'r` ascriptions keep their own region but their
    /// pointee is still canonicalised.
    pub fn region_erase(&mut self, ty: Ty<'tcx>) -> Ty<'tcx> {
        match ty.kind() {
            TyKind::Tuple(elems) => {
                let elems: Vec<Ty<'tcx>> = elems.iter().map(|e| self.region_erase(*e)).collect();
                self.intern_tuple(elems)
            }
            TyKind::App(er, args) => {
                let er = *er;
                let args: Vec<Ty<'tcx>> = args.iter().map(|a| self.region_erase(*a)).collect();
                self.intern_app(er, args)
            }
            TyKind::Region(inner, r) => {
                let inner = self.region_erase(*inner);
                self.region_ty(inner, *r)
            }
            TyKind::Ref(_, inner) => {
                let inner = self.region_erase(*inner);
                let r = self.anon_region();
                self.ref_ty(r, inner)
            }
            TyKind::RefMut(_, inner) => {
                let inner = self.region_erase(*inner);
                let r = self.anon_region();
                self.ref_mut_ty(r, inner)
            }
            _ => ty,
        }
    }

    /// Replace every (non-`'static`) reference / ascription region in `ty` with
    /// `region`, recursing through composites. Used at a call boundary to stamp
    /// the callee's return type with the call's computed result region (the
    /// [`region_meet`](Self::region_meet)), replacing the region-blind
    /// `region_erase`. `'static` regions are preserved.
    pub fn region_fill(&mut self, ty: Ty<'tcx>, region: Region) -> Ty<'tcx> {
        let pick = |r: &Region| {
            if matches!(r, Region::Static) {
                *r
            } else {
                region
            }
        };
        match ty.kind() {
            TyKind::Tuple(elems) => {
                let elems: Vec<Ty<'tcx>> =
                    elems.iter().map(|e| self.region_fill(*e, region)).collect();
                self.intern_tuple(elems)
            }
            TyKind::App(er, args) => {
                let er = *er;
                let args: Vec<Ty<'tcx>> =
                    args.iter().map(|a| self.region_fill(*a, region)).collect();
                self.intern_app(er, args)
            }
            TyKind::Region(inner, r) => {
                let rr = pick(r);
                let inner = self.region_fill(*inner, region);
                self.region_ty(inner, rr)
            }
            TyKind::Ref(r, inner) => {
                let rr = pick(r);
                let inner = self.region_fill(*inner, region);
                self.ref_ty(rr, inner)
            }
            TyKind::RefMut(r, inner) => {
                let rr = pick(r);
                let inner = self.region_fill(*inner, region);
                self.ref_mut_ty(rr, inner)
            }
            _ => ty,
        }
    }

    /// The result region of a call (Calculus §6.3, item 8): the greatest lower
    /// bound (shortest-lived) of the argument regions, so the result cannot
    /// outlive any borrowed argument. Argument regions live at the call site,
    /// so the GLB is well-defined; mutually-incomparable regions fall back
    /// to the call-site scope (a sound common lower bound). With no
    /// reference arguments the result has no region to fill, so the
    /// call-site scope is returned harmlessly. This is a *conservative*
    /// meet over all argument regions (it does not yet instantiate each
    /// lifetime parameter separately).
    pub fn region_meet(&self, regions: &[Region], assumptions: &[RegionConstraint]) -> Region {
        let call_site = self.current_scope_region();
        let mut it = regions.iter().copied();
        let Some(mut result) = it.next() else {
            return call_site;
        };
        for r in it {
            if self.outlives(result, r, assumptions) {
                result = r; // r is shorter-lived → closer to the GLB
            } else if self.outlives(r, result, assumptions) {
                // result is already the shorter-lived → keep it
            } else {
                result = call_site; // incomparable → safe common lower bound
            }
        }
        result
    }

    /// Infer the region substitution for a call: map each callee region
    /// parameter (and the shared elided/anon region) to the **meet** of the
    /// actual argument regions it aligns with. This is the region analogue of
    /// type-parameter [`unify`](crate::passes::type_ast::generics::unify) — the
    /// declared parameter types are walked in parallel with the actual argument
    /// types and each solved region collects the actual regions in its
    /// position; a region bound from several arguments meets them (so the
    /// result outlives none of them). A solved region with no binding
    /// defaults to the call-site scope (the safe, shortest region). Regions
    /// are *solved* here, not constrained; the callee's `where` clauses are
    /// checked separately.
    ///
    /// The elided region is included among the solved variables so that a
    /// non-region-parametric forwarder (`def id(x: &Int): &Int := x`) still
    /// ties its result to the actual argument region (preserving the escape
    /// check).
    pub fn infer_region_subst(
        &self,
        decls: &[Ty<'tcx>],
        actuals: &[Ty<'tcx>],
        region_params: &[RegionParam],
    ) -> Map<RegionVar, Region> {
        // the region variables we solve for: the declared params + the shared
        // elided-borrow region.
        let mut solve: Set<RegionVar> = region_params.iter().map(|p| p.region).collect();
        if let Some(rv) = self.anon_region_var {
            solve.insert(rv);
        }

        let mut collected: Map<RegionVar, Vec<Region>> = Map::new();
        for (d, a) in decls.iter().zip(actuals) {
            collect_region_bindings(*d, *a, &solve, &mut collected);
        }

        let call_site = self.current_scope_region();
        let mut subst: Map<RegionVar, Region> = Map::new();
        for rv in &solve {
            let region = match collected.get(rv) {
                Some(regions) => self.region_meet(regions, &[]),
                None => call_site, // unbound → conservative shortest region
            };
            subst.insert(*rv, region);
        }
        subst
    }

    /// Replace every region in `ty` that the call's region substitution
    /// ([`infer_region_subst`](Self::infer_region_subst)) solved, recursing
    /// through composites. Like [`region_fill`](Self::region_fill) but maps
    /// each region individually rather than collapsing all to one;
    /// `'static` and unmapped regions are preserved.
    pub fn region_subst_ty(&mut self, ty: Ty<'tcx>, subst: &Map<RegionVar, Region>) -> Ty<'tcx> {
        match ty.kind() {
            TyKind::Tuple(elems) => {
                let elems: Vec<Ty<'tcx>> = elems
                    .iter()
                    .map(|e| self.region_subst_ty(*e, subst))
                    .collect();
                self.intern_tuple(elems)
            }
            TyKind::App(er, args) => {
                let er = *er;
                let args: Vec<Ty<'tcx>> = args
                    .iter()
                    .map(|a| self.region_subst_ty(*a, subst))
                    .collect();
                self.intern_app(er, args)
            }
            TyKind::Region(inner, r) => {
                let rr = apply_region_subst(*r, subst);
                let inner = self.region_subst_ty(*inner, subst);
                self.region_ty(inner, rr)
            }
            TyKind::Ref(r, inner) => {
                let rr = apply_region_subst(*r, subst);
                let inner = self.region_subst_ty(*inner, subst);
                self.ref_ty(rr, inner)
            }
            TyKind::RefMut(r, inner) => {
                let rr = apply_region_subst(*r, subst);
                let inner = self.region_subst_ty(*inner, subst);
                self.ref_mut_ty(rr, inner)
            }
            _ => ty,
        }
    }

    /// Enter a function's `where` constraints as the current outlives
    /// assumptions (used when checking callee `where` clauses at call sites),
    /// returning the previous set to restore on exit.
    pub fn set_where_assumptions(
        &mut self,
        constraints: Vec<RegionConstraint>,
    ) -> Vec<RegionConstraint> {
        std::mem::replace(&mut self.cur_where_constraints, constraints)
    }

    /// The outlives assumptions of the function currently being checked.
    pub fn where_assumptions(&self) -> &[RegionConstraint] {
        &self.cur_where_constraints
    }

    /// The shared anonymous region used for elided borrows (`&e`, `&T` with no
    /// explicit lifetime). All elided borrows share one region for now, so that
    /// an elided `&T` type and an elided `&e` value compare equal. Per-borrow
    /// fresh regions and the escape check arrive with the solver in Step 8b.
    pub fn anon_region(&mut self) -> Region {
        if let Some(rv) = self.anon_region_var {
            return Region::Var(rv);
        }
        let rv = RegionVar(self.region_count);
        self.region_count += 1;
        self.anon_region_var = Some(rv);
        Region::Var(rv)
    }

    // ---- lexical region scopes + the outlives solver (Step 8b) --------------

    /// Enter a fresh lexical region scope (a function body or a block),
    /// returning its region. Each scope nests one level deeper than its parent.
    pub fn enter_region_scope(&mut self) -> Region {
        let depth = self.region_scope_stack.len();
        let rv = RegionVar(self.region_count);
        self.region_count += 1;
        self.region_depths.insert(rv, depth);
        self.region_scope_stack.push(rv);
        Region::Var(rv)
    }

    /// Leave the innermost lexical region scope (see
    /// [`Self::enter_region_scope`]).
    pub fn exit_region_scope(&mut self) {
        self.region_scope_stack.pop();
    }

    /// The innermost (current) lexical scope region, or `'static` if no scope
    /// is open. Borrows of temporaries and freshly bound locals live here.
    pub fn current_scope_region(&self) -> Region {
        self.region_scope_stack
            .last()
            .map(|&rv| Region::Var(rv))
            .unwrap_or(Region::Static)
    }

    /// The lexical nesting depth of a region. `'static` and any region without
    /// a recorded scope (region parameters, the elided-borrow region) are
    /// depth 0: they outlive every lexical scope, so borrows into them
    /// never escape.
    pub fn region_depth(&self, r: Region) -> usize {
        match r {
            Region::Static => 0,
            Region::Var(rv) => self.region_depths.get(&rv).copied().unwrap_or(0),
        }
    }

    /// Whether `r` is a real lexical scope region (allocated by
    /// [`Self::enter_region_scope`]) — i.e. a *local* region (the function
    /// frame or a block) — as opposed to a *region parameter*, the
    /// elided-borrow region, or `'static` (all of which outlive the frame).
    /// Used by the function-return escape check: only non-scope (outer)
    /// regions may be named by a returned value's type (Calculus §6.3,
    /// frame boundary).
    pub fn is_scope_region(&self, r: Region) -> bool {
        matches!(r, Region::Var(rv) if self.region_depths.contains_key(&rv))
    }

    /// Does `longer` outlive `shorter` (`longer ≥ shorter`; Calculus §1.1)?
    ///
    /// `'static` outlives everything and every region outlives itself; a
    /// shallower lexical scope outlives a deeper (inner) one; and `assumptions`
    /// (a function's `where 'a >= 'b` clauses) add further edges, closed
    /// transitively.
    pub fn outlives(
        &self,
        longer: Region,
        shorter: Region,
        assumptions: &[RegionConstraint],
    ) -> bool {
        if longer == shorter || longer == Region::Static {
            return true;
        }
        // Lexical nesting: a shallower region outlives a deeper one. Depth is the
        // nesting level, with region parameters, the elided region, and `'static`
        // all at depth 0 (outermost) — so a caller lifetime or the function frame
        // outlives every inner block. Conversely nothing is concluded to outlive
        // a depth-0 region here (it stays conservative: only equality, `'static`,
        // or an explicit assumption can).
        if self.region_depth(longer) < self.region_depth(shorter) {
            return true;
        }
        // transitive closure over the assumed `where` edges.
        let mut worklist = vec![longer];
        let mut seen: Set<Region> = Set::new();
        while let Some(r) = worklist.pop() {
            if !seen.insert(r) {
                continue;
            }
            for c in assumptions {
                if c.longer == r {
                    if c.shorter == shorter {
                        return true;
                    }
                    worklist.push(c.shorter);
                }
            }
        }
        false
    }

    /// Are all `required` outlives constraints satisfied, given `assumptions`?
    pub fn satisfies_outlives(
        &self,
        required: &[RegionConstraint],
        assumptions: &[RegionConstraint],
    ) -> bool {
        required
            .iter()
            .all(|c| self.outlives(c.longer, c.shorter, assumptions))
    }

    /// The [`Ty`] handle for the enum type `er`.
    ///
    /// Every `EnumRef` is interned as a `Ty` at registration time, so this
    /// lookup is a pure read and always succeeds.
    pub fn enum_ty(&self, er: EnumRef<'tcx>) -> Ty<'tcx> {
        *self
            .ty_interner
            .get(&TyKind::Enum(er))
            .unwrap_or_else(|| internal_bug!("enum {er:?} was not interned as a Ty"))
    }

    // ======================== Type parameters ================================

    /// Allocate the type parameters for a generic declaration and make them
    /// the current in-scope set, so that [`Self::lookup_type_param`] (used by
    /// type resolution) can resolve their names. Returns the [`TypeParam`]
    /// list to store on the declaration. Call [`Self::end_type_params`] when
    /// the declaration is fully built.
    pub fn begin_type_params(&mut self, specs: &[TypeParamSpec]) -> Vec<TypeParam> {
        let mut params = Vec::with_capacity(specs.len());
        let mut scope = Map::new();
        for spec in specs {
            let id = TypeParamId(self.type_param_count);
            self.type_param_count += 1;
            self.type_param_names.insert(id, spec.name.clone());
            scope.insert(spec.name.clone(), id);
            params.push(TypeParam {
                id,
                name: spec.name.clone(),
                range: spec.range,
                variance: spec.variance,
                kind: spec.kind,
            });
        }
        self.cur_type_params = scope;
        params
    }

    /// Re-enter an already-allocated type-parameter scope (e.g. an enum's, when
    /// resolving its variant payloads in a later phase). Unlike
    /// [`Self::begin_type_params`], this allocates no new ids.
    pub fn enter_type_param_scope(&mut self, params: &[TypeParam]) {
        self.cur_type_params = params.iter().map(|p| (p.name.clone(), p.id)).collect();
    }

    /// Allocate the region parameters for a generic declaration and make them
    /// the current in-scope set, so that [`Self::resolve_region`] can resolve
    /// their names. Returns the [`RegionParam`] list to store on the
    /// declaration. Cleared alongside type parameters by
    /// [`Self::end_type_params`].
    pub fn begin_region_params(&mut self, specs: &[RegionParamSpec]) -> Vec<RegionParam> {
        let mut params = Vec::with_capacity(specs.len());
        let mut scope = Map::new();
        for spec in specs {
            let rv = RegionVar(self.region_count);
            self.region_count += 1;
            scope.insert(spec.name.clone(), rv);
            params.push(RegionParam {
                name: spec.name.clone(),
                region: rv,
                range: spec.range,
            });
        }
        self.cur_regions = scope;
        params
    }

    /// Re-enter an already-allocated region-parameter scope (the region
    /// counterpart of [`Self::enter_type_param_scope`]).
    pub fn enter_region_param_scope(&mut self, params: &[RegionParam]) {
        self.cur_regions = params.iter().map(|p| (p.name.clone(), p.region)).collect();
    }

    /// Clear the current in-scope type *and* region parameters (see
    /// [`Self::begin_type_params`] / [`Self::begin_region_params`]).
    pub fn end_type_params(&mut self) {
        self.cur_type_params.clear();
        self.cur_regions.clear();
    }

    /// Resolve a name to a type parameter in the current declaration's scope.
    pub fn lookup_type_param(&self, name: &str) -> Option<TypeParamId> {
        self.cur_type_params.get(name).copied()
    }

    /// The interned [`Ty`] for a type parameter use site.
    pub fn param_ty(&mut self, id: TypeParamId) -> Ty<'tcx> {
        self.intern_ty(TyKind::Param(id))
    }

    /// The display name of a type parameter.
    pub fn type_param_name(&self, id: TypeParamId) -> String {
        self.type_param_names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("?{}", id.0))
    }

    // ========================== Variables ====================================

    pub fn new_original_variable(
        &mut self,
        pair: &Pair<'_, Rule>,
        rule: Rule,
    ) -> Result<OriginalVarRef<'tcx>, ContextError> {
        if !matches!(
            rule,
            Rule::declaration | Rule::parameter | Rule::binding_pattern
        ) {
            return Err(ContextError::IllegalVariableRegistration { rule });
        }

        let id = self.var_count;
        self.var_count += 1;
        let var = OriginalVar::create(pair, id, rule.into());
        let r = self.arenas.alloc_variable(var);
        Ok(OriginalVarRef(r))
    }

    pub fn uniquify_original_variable(&mut self, ovref: OriginalVarRef<'tcx>) -> UniqVar<'tcx> {
        let idx = self.uniq_count;
        self.uniq_count += 1;
        UniqVar { idx, orig: ovref }
    }

    pub fn original_var_name(&self, ovref: &OriginalVarRef<'tcx>) -> String {
        ovref.0.name.name()
    }

    pub fn uniq_variable_name(&self, uv: &UniqVar<'tcx>) -> String {
        uv.orig.0.name.name()
    }

    pub fn uniq_var_declaration(&self, uv: &UniqVar<'tcx>) -> Range {
        uv.orig.0.declaration
    }

    /// Returns the display name of the given HIR variable.
    /// Should not be used for logic, only for diagnostic output.
    pub fn hir_var_name(&self, hv: &HirVar<'tcx>) -> String {
        match hv {
            HirVar::Decl(ovref) => self.original_var_name(ovref),
            HirVar::Unqualified(name) => name.clone(),
            HirVar::Uniq(uv) => self.uniq_variable_name(uv),
        }
    }

    #[allow(dead_code)]
    fn register_variable_usage(&mut self, var: OriginalVarRef<'tcx>, range: Range) {
        self.variable_usages
            .entry(var)
            .and_modify(|e| {
                e.insert(range);
            })
            .or_insert(Set::from([range]));
    }

    // ============================= Functions =================================

    pub fn function_count(&self) -> usize {
        self.global_functions.len()
    }

    pub fn all_functions(&self) -> impl Iterator<Item = FunRef<'tcx>> + '_ {
        self.global_functions.iter().copied()
    }

    pub fn register_function(
        &mut self,
        pair: &Pair<'_, Rule>,
        module: &ModuleRef<'tcx>,
    ) -> Result<FunRef<'tcx>, ContextError> {
        let id = self.global_functions.len();
        let fun = OriginalFun::create(pair, id, *module);
        let fref = FunRef(self.arenas.alloc_function(fun));
        self.global_functions.push(fref);
        Ok(fref)
    }

    /// Register a compiler-synthesised function (e.g. a monomorphised
    /// specialisation) with the given name, inheriting the original's module
    /// and declaration site. Returns a fresh, distinct [`FunRef`].
    pub fn register_mono_function(
        &mut self,
        name: String,
        module: ModuleRef<'tcx>,
        declaration: Range,
    ) -> FunRef<'tcx> {
        let id = self.global_functions.len();
        let fun = OriginalFun::synthetic(name, declaration, module, id);
        let fref = FunRef(self.arenas.alloc_function(fun));
        self.global_functions.push(fref);
        fref
    }

    #[track_caller]
    pub fn original_fun_name(&self, fun: FunRef<'tcx>) -> String {
        fun.0.name.name()
    }

    pub fn original_fun(&self, fun: &FunRef<'tcx>) -> &'tcx OriginalFun<'tcx> {
        fun.0
    }

    pub fn get_fun_sig(&self, fun: &FunRef<'tcx>) -> Option<FunSig<'tcx>> {
        self.function_signatures.get(fun).cloned()
    }

    #[track_caller]
    pub fn fun_sig(&self, fun: &FunRef<'tcx>) -> FunSig<'tcx> {
        self.function_signatures[fun].clone()
    }

    pub fn set_fun_sig(&mut self, fun: FunRef<'tcx>, sig: FunSig<'tcx>) {
        let out = self.function_signatures.insert(fun, sig);
        debug_assert!(out.is_none());
    }

    pub fn is_main(&self, fun: FunRef<'tcx>) -> bool {
        self.entrypoint == Some(fun)
    }

    // ============================ Enums =====================================

    /// Phase 1 of enum registration: allocate the `EnumRef` and record the
    /// variant *names*, with every payload initially `None`. This must run
    /// for every enum in the program before phase 2
    /// ([`Self::set_variant_payload`]) resolves payload type annotations
    ///
    /// payloads may reference other enums (including forward / recursive
    /// references), so every `EnumRef` must already exist before any
    /// `type_` gets resolved.
    pub fn register_enum(
        &mut self,
        name: &str,
        variant_names: Vec<String>,
        type_params: Vec<TypeParam>,
        region_params: Vec<RegionParam>,
        range: Range,
        module: ModuleRef<'tcx>,
    ) -> Result<EnumRef<'tcx>, ContextError> {
        if let Some(existing) = self.enum_names.get(name) {
            return Err(ContextError::DuplicateEnum {
                name: name.to_string(),
                first: existing.0.range,
                second: range,
            });
        }
        let id = self.enum_defs.len();
        let def = EnumDef {
            name: name.to_string(),
            variants: variant_names
                .into_iter()
                .map(|name| EnumVariant {
                    name,
                    payload: Cell::new(None),
                })
                .collect(),
            range,
            src_module: module,
            id,
            type_params,
            region_params,
            is_anonymous: false,
        };
        let er = EnumRef(self.arenas.alloc_enum(def));
        self.enum_defs.push(er);
        self.enum_names.insert(name.to_string(), er);
        self.intern_ty(TyKind::Enum(er));
        Ok(er)
    }

    /// Phase 2 of enum registration: attach a resolved payload type to a
    /// variant that was registered (with `payload: None`) by
    /// [`Self::register_enum`]. Uses the variant's `Cell` so the shared,
    /// arena-allocated `EnumDef` does not need to be mutably re-borrowed.
    pub fn set_variant_payload(
        &mut self,
        er: EnumRef<'tcx>,
        variant_idx: usize,
        payload: Ty<'tcx>,
    ) {
        er.0.variants[variant_idx].payload.set(Some(payload));
    }

    pub fn get_enum(&self, er: EnumRef<'tcx>) -> &'tcx EnumDef<'tcx> {
        er.0
    }

    /// Returns a [`Display`]-able wrapper for [`Ty`] that resolves enum names
    /// via this context.
    pub fn display_ty<'a>(&'a self, ty: Ty<'tcx>) -> TyDisplay<'a, 'tcx> {
        TyDisplay { ty, ctx: self }
    }

    pub fn lookup_enum_by_name(&self, name: &str) -> Option<EnumRef<'tcx>> {
        self.enum_names.get(name).copied()
    }

    pub fn lookup_variant(&self, er: EnumRef<'tcx>, variant: &str) -> Option<usize> {
        er.0.variants.iter().position(|v| v.name == variant)
    }

    pub fn lookup_enum_in_module(
        &self,
        module: ModuleRef<'tcx>,
        name: &str,
    ) -> Option<EnumRef<'tcx>> {
        self.enum_defs
            .iter()
            .find(|er| er.0.src_module == module && er.0.name == name)
            .copied()
    }

    /// Record the module currently being processed during AST building so that
    /// anonymous tag-union types can be attributed to the right module.
    pub fn set_build_module(&mut self, m: ModuleRef<'tcx>) {
        self.cur_build_module = Some(m);
    }

    /// Intern an ad-hoc tag-union type from a sorted, deduplicated list of tag
    /// names. Returns the same `EnumRef` for any two call sites with the same
    /// tag set (structural identity).
    ///
    /// The resulting `EnumDef` uses a sorted `variants` list so that variant
    /// indices are stable regardless of the order tags were written.
    pub fn register_or_get_anon_enum(
        &mut self,
        mut tags: Vec<String>,
        range: Range,
    ) -> EnumRef<'tcx> {
        tags.sort();
        tags.dedup();
        if let Some(&er) = self.anon_tag_types.get(&tags) {
            return er;
        }
        let display_name = tags
            .iter()
            .map(|t| format!("#{t}"))
            .collect::<Vec<_>>()
            .join(" | ");
        let src_module = self
            .cur_build_module
            .or(self.default_module)
            .or_else(|| self.project_modules.first().copied())
            .unwrap_or_else(|| internal_bug!("no module context for anonymous enum"));
        let id = self.enum_defs.len();
        let def = EnumDef {
            name: display_name,
            variants: tags
                .iter()
                .map(|name| EnumVariant {
                    name: name.clone(),
                    payload: Cell::new(None),
                })
                .collect(),
            range,
            src_module,
            id,
            type_params: Vec::new(),
            region_params: Vec::new(),
            is_anonymous: true,
        };
        let er = EnumRef(self.arenas.alloc_enum(def));
        self.enum_defs.push(er);
        // NOTE: anonymous enums are not inserted into `enum_names`; they are
        // only looked up via `anon_tag_types`.
        self.anon_tag_types.insert(tags, er);
        self.intern_ty(TyKind::Enum(er));
        er
    }

    pub fn enum_display(&self, enum_ref: EnumRef<'tcx>, variant: usize) -> String {
        let variants = &enum_ref.0.variants;
        if variants.len() <= variant {
            internal_bug!("enum display indexed with oob variant: {enum_ref:?}[{variant}]");
        }
        variants[variant].name.clone()
    }

    // ============================ Modules ===================================

    pub fn create_dummy_module(
        &mut self,
        for_file: FileRef,
    ) -> Result<ModuleRef<'tcx>, CtxEmptyError> {
        if self.default_module.is_some() {
            Err(CtxEmptyError {})
        } else {
            let mr = self.register_module(DEFAULT_MODULE_NAME, for_file);
            self.default_module = Some(mr);
            Ok(mr)
        }
    }

    pub fn register_module(&mut self, name: &str, in_file: FileRef) -> ModuleRef<'tcx> {
        let id = self.project_modules.len();
        let cm = CodeModule {
            from_file: in_file,
            name: name.to_string(),
            id,
        };
        let mr = ModuleRef(self.arenas.alloc_module(cm));
        self.project_modules.push(mr);
        mr
    }

    pub fn create_default_module(&mut self, for_file: FileRef, name: &str) -> ModuleRef<'tcx> {
        let mr = self.register_module(name, for_file);
        self.file_defaults.insert(for_file, mr);
        mr
    }

    pub fn default_module(&mut self, for_file: FileRef) -> ModuleRef<'tcx> {
        if let Some(dm) = self.file_defaults.get(&for_file) {
            *dm
        } else {
            let name = format!("{DEFAULT_MODULE_NAME}_{}", for_file.0);
            let mr = self.register_module(&name, for_file);
            self.file_defaults.insert(for_file, mr);
            mr
        }
    }

    /// Return a reference to a dummy file.
    /// This should ONLY EVER be invoked at the start of a file-less
    /// compilation.
    pub fn stub_file(&self) -> FileRef {
        assert!(self.project_modules.is_empty());
        FileRef(usize::MAX)
    }

    /// Register (idempotently) the synthetic core library module and return its
    /// `FileRef`. Uses a reserved sentinel `FileRef(usize::MAX - 1)` that never
    /// conflicts with real files or test stubs.
    pub fn ensure_core_module(&mut self) -> FileRef {
        let core_file = FileRef(usize::MAX - 1);
        if !self.file_defaults.contains_key(&core_file) {
            self.create_default_module(core_file, "__core");
        }
        core_file
    }

    pub fn is_core_module(&self, file: FileRef) -> bool {
        file.0 == usize::MAX - 1
    }

    pub fn get_mod_by_name(&self, name: &str) -> Option<ModuleRef<'tcx>> {
        self.project_modules
            .iter()
            .find(|m| m.0.name == name)
            .copied()
    }

    pub fn module_info(&self, mr: &ModuleRef<'tcx>) -> ModuleInfo<'tcx> {
        ModuleInfo {
            name: mr.0.name.clone(),
            index: *mr,
        }
    }

    pub fn file_of_module(&self, mr: ModuleRef<'tcx>) -> FileRef {
        mr.0.from_file
    }
}

// ========================= Display helpers ===================================

/// A [`Display`]-able wrapper for [`Ty`] that resolves enum names via a
/// [`CompileCtx`]. Obtain via [`CompileCtx::display_ty`].
pub struct TyDisplay<'a, 'tcx: 'a> {
    ty: Ty<'tcx>,
    ctx: &'a CompileCtx<'tcx>,
}

impl std::fmt::Display for TyDisplay<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty.kind() {
            TyKind::Enum(er) => write!(f, "{}", self.ctx.get_enum(*er).name),
            TyKind::Param(id) => write!(f, "{}", self.ctx.type_param_name(*id)),
            TyKind::Tuple(elems) => {
                write!(f, "(")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", self.ctx.display_ty(*elem))?;
                }
                write!(f, ")")
            }
            _ => write!(f, "{}", self.ty),
        }
    }
}

/// Apply a call's region substitution to a single region: a solved region
/// variable is replaced by its inferred region; `'static` and unsolved regions
/// are left as-is.
fn apply_region_subst(r: Region, subst: &Map<RegionVar, Region>) -> Region {
    match r {
        Region::Var(rv) => subst.get(&rv).copied().unwrap_or(r),
        Region::Static => Region::Static,
    }
}

/// Walk a declared parameter type against an actual argument type in parallel,
/// recording — for every region position whose declared region is one of the
/// `solve` variables — the actual region found there. The accumulated
/// candidates are later met per variable (see
/// [`CompileCtx::infer_region_subst`]). Region positions whose declared region
/// is not solved (e.g. `'static`) are ignored; structural mismatches simply
/// stop the recursion (the type checker has already verified the shapes agree
/// modulo regions).
fn collect_region_bindings(
    decl: Ty<'_>,
    actual: Ty<'_>,
    solve: &Set<RegionVar>,
    out: &mut Map<RegionVar, Vec<Region>>,
) {
    let mut bind = |dr: Region, ar: Region| {
        if let Region::Var(rv) = dr
            && solve.contains(&rv)
        {
            out.entry(rv).or_default().push(ar);
        }
    };
    match (decl.kind(), actual.kind()) {
        (TyKind::Ref(dr, di), TyKind::Ref(ar, ai))
        | (TyKind::RefMut(dr, di), TyKind::RefMut(ar, ai)) => {
            bind(*dr, *ar);
            collect_region_bindings(*di, *ai, solve, out);
        }
        (TyKind::Region(di, dr), TyKind::Region(ai, ar)) => {
            bind(*dr, *ar);
            collect_region_bindings(*di, *ai, solve, out);
        }
        (TyKind::Tuple(ds), TyKind::Tuple(as_)) if ds.len() == as_.len() => {
            for (d, a) in ds.iter().zip(*as_) {
                collect_region_bindings(*d, *a, solve, out);
            }
        }
        (TyKind::App(_, ds), TyKind::App(_, as_)) if ds.len() == as_.len() => {
            for (d, a) in ds.iter().zip(*as_) {
                collect_region_bindings(*d, *a, solve, out);
            }
        }
        _ => {}
    }
}
