//! The compilation context, threaded through every compiler pass.

use std::cell::Cell;

use pest::iterators::Pair;
use thiserror::Error;

use crate::compiler::context::DefTarget;
use crate::compiler::context::TypeRefEntry;
use crate::compiler::context::arenas::Arenas;
use crate::compiler::diagnostics::DiagnosticSeverity;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::structure::AdtDef;
use crate::compiler::structure::CodeModule;
use crate::compiler::structure::EnumVariant;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::ImplDef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleImports;
use crate::compiler::structure::ModuleInfo;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::OriginalFun;
use crate::compiler::structure::OriginalVar;
use crate::compiler::structure::OriginalVarRef;
use crate::compiler::structure::Pos;
use crate::compiler::structure::Range;
use crate::compiler::structure::RegionParam;
use crate::compiler::structure::RegionParamSpec;
use crate::compiler::structure::Set;
use crate::compiler::structure::TypeConstraint;
use crate::compiler::structure::TypeHead;
use crate::compiler::structure::TypeParam;
use crate::compiler::structure::TypeParamSpec;
use crate::compiler::structure::TypeclassDef;
use crate::compiler::structure::TypeclassRef;
use crate::compiler::structure::UniqVar;
use crate::compiler::structure::VarName;
use crate::internal_bug;
use crate::ir_types::hhir::HirVar;
use crate::lang::types::AdtRef;
use crate::lang::types::CommonTypes;
use crate::lang::types::FnMode;
use crate::lang::types::Kind;
use crate::lang::types::KindId;
use crate::lang::types::Region;
use crate::lang::types::RegionConstraint;
use crate::lang::types::RegionVar;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::lang::types::TypeParamId;
use crate::passes::parse::Rule;
use crate::passes::type_ast::AstTypeError;

/// This should not be used and is intentionally misspelled to be easily
/// detectable.
const DEFAULT_MODULE_NAME: &str = "mAin";

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
    /// Interner for generic enum instantiations, keyed by base enum + type args
    /// + region args (so `Holder<'a>` and `Holder<'b>` are distinct types).
    app_interner: Map<(AdtRef<'tcx>, Vec<Ty<'tcx>>, Vec<Region>), Ty<'tcx>>,
    /// Interner for region-ascribed types `T @ 'r`, keyed by inner type +
    /// region.
    region_ty_interner: Map<(Ty<'tcx>, Region), Ty<'tcx>>,
    /// Interner for arrow kinds `K₁ -> K₂`. `kind_arrows[id]` is the
    /// `(domain, codomain)` pair; `kind_arrow_ids` maps the pair back to its
    /// canonical [`KindId`], so equal arrows share an id.
    kind_arrows: Vec<(Kind, Kind)>,
    kind_arrow_ids: Map<(Kind, Kind), KindId>,
    /// Interner for higher-kinded parameter applications `F<A>`,
    /// keyed by the constructor parameter + its type arguments.
    param_app_interner: Map<(TypeParamId, Vec<Ty<'tcx>>), Ty<'tcx>>,

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
    /// then each enclosing block) currently being type-checked. Used
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
    /// The `where T : C` typeclass constraints of the function currently being
    /// checked, assumed while resolving method calls on its type parameters.
    cur_type_constraints: Vec<TypeConstraint>,

    // type parameters
    /// Number of type parameters allocated so far.
    /// assigns each a globally unique [`TypeParamId`].
    type_param_count: usize,
    /// Display names of every allocated type parameter, keyed by id.
    type_param_names: Map<TypeParamId, String>,
    /// Declared kind of every allocated type parameter, keyed by id
    /// `Owned` for ordinary params, an arrow for higher-kinded ones. Ids are
    /// globally unique, so this persists across declarations.
    type_param_kinds: Map<TypeParamId, Kind>,
    /// Name → id for the type parameters in scope while the current generic
    /// declaration is being built (set by [`Self::begin_type_params`]).
    cur_type_params: Map<String, TypeParamId>,

    // variables
    /// Number of original variables registered so far.
    /// assigns each a stable monotonic `id` used for ordering.
    var_count: usize,
    /// Number of uniquified variables registered so far.
    /// assigns each a stable `idx` distinguishing shadowing
    /// re-bindings of the same declaration.
    uniq_count: usize,

    // functions
    global_functions: Vec<FunRef<'tcx>>,
    function_signatures: Map<FunRef<'tcx>, FunSig<'tcx>>,
    pub entrypoint: Option<FunRef<'tcx>>,
    /// External (FFI) functions: a bodyless `extern def` bound
    /// to a C symbol. Registered with a real `FunRef`+`FunSig` (so calls
    /// resolve through the normal path) but no body in any IR; codegen
    /// declares the symbol, the interpreter dispatches to a built-in. Maps
    /// to `(declaring module, C symbol name)`.
    extern_funcs: Map<FunRef<'tcx>, (ModuleRef<'tcx>, String)>,

    // enums
    enum_defs: Vec<AdtRef<'tcx>>,
    enum_names: Map<String, AdtRef<'tcx>>,
    /// Interner for ad-hoc tag-union types, keyed by sorted tag list.
    anon_tag_types: Map<Vec<String>, AdtRef<'tcx>>,
    /// The module currently being built (set in `build_function`).
    cur_build_module: Option<ModuleRef<'tcx>>,

    // typeclasses
    /// All registered typeclasses, indexed by `TypeclassRef`.
    typeclasses: Vec<TypeclassDef<'tcx>>,
    /// Class name -> ref. Global for now (class names are unique program-wide);
    /// `src_module` on the def carries ownership for the orphan rule.
    typeclass_names: Map<String, TypeclassRef>,
    /// Method name -> its owning class (one class per method name).
    method_index: Map<String, TypeclassRef>,
    /// The one global, coherent instance set, keyed by `(class, head type)`.
    // Instances bucketed by `(class, head-constructor)`. A bucket holds more than
    // one instance only for a higher-kinded class whose members fix different
    // slots (`Functor for Result<_, Int>` vs `… <_, Bool>`); `register_instance`
    // rejects *overlapping* heads, so resolution stays unambiguous (Calculus:
    // Typeclasses). Ground classes (`Copy`/`Clone`/…) always have at most one per key.
    instances: Map<(TypeclassRef, TypeHead<'tcx>), Vec<ImplDef<'tcx>>>,
    /// Lang-item handles for the `Copy` / `Clone` classes (resolved by name
    /// when `core.sand` registers them), used to drive implicit-copy.
    copy_class: Option<TypeclassRef>,
    clone_class: Option<TypeclassRef>,
    /// Lang-item handles for the `Send` / `Sync` thread-safety marker classes
    /// (resolved by name from `core.sand`), so the compiler can reason about
    /// them structurally (like `Copy`) when checking `where T : Send` bounds.
    send_class: Option<TypeclassRef>,
    sync_class: Option<TypeclassRef>,
    /// The `Heaped` lang-item class (a registration hook). The class itself is
    /// declared in `core.sand`; this slot stays `None` until then, but the
    /// by-name registration is reserved here so the compiler can emit
    /// `alloc`/`borrow`/`release` calls once it exists.
    heaped_class: Option<TypeclassRef>,
    /// Concrete `Unique<...>` enum instances. Monomorphisation
    /// erases the generic `Unique<T>` into an ordinary specialised enum, but
    /// codegen still needs to know "this enum is a heap handle" to emit a
    /// `free` when one is dropped. `request_enum` records each specialisation
    /// of the `Unique` lang-item here.
    unique_instances: Set<AdtRef<'tcx>>,

    // modules
    project_modules: Vec<ModuleRef<'tcx>>,
    /// Per-module `use` imports: for an importing module, the explicit
    /// `use src::name` brings `name` into its unqualified scope, and `use
    /// src::*` glob-imports `src`. Source modules are stored by name and
    /// resolved lazily (every module is registered before resolution runs).
    module_imports: Map<ModuleRef<'tcx>, ModuleImports>,

    // defaults
    file_defaults: Map<FileRef, ModuleRef<'tcx>>,
    default_module: Option<ModuleRef<'tcx>>,

    /// Source-position to definition index for *type* and *typeclass* name
    /// references (signatures, annotations, payloads, `impl`/`where`/`requires`
    /// heads). Populated during AST building so the LSP can resolve
    /// go-to-definition on a type or class name
    type_refs: Vec<TypeRefEntry<'tcx>>,

    // diagnostics
    /// Non-fatal diagnostics (warnings) emitted by passes via [`Self::warn`].
    /// Drained by `pipeline::run` into the diagnostics sink after each run.
    pub diagnostics: Vec<SandDiagnostic>,
    /// The file currently being checked, set per-function by `infer_function`.
    /// Used to anchor inline warnings (a [`Range`] alone has no file).
    pub cur_file: Option<FileRef>,
    /// Recovered (non-aborting) type errors collected while checking the
    /// current function: a statement that fails to type-check is recorded
    /// here and the pass continues (see `type_ast`'s statement-level `Top`
    /// recovery), so a single bad statement no longer hides errors in its
    /// siblings. Drained per-function by `TypedProgram::from_ast_program`.
    pub type_errors: Vec<AstTypeError<'tcx>>,
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
    // The `ty_interner`/`tuple_interner` maps are keyed by `TyKind`/`Ty`, which
    // reach an enum payload `Cell`
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
            kind_arrows: Default::default(),
            kind_arrow_ids: Default::default(),
            param_app_interner: Default::default(),
            region_ty_interner: Default::default(),
            region_count: 0,
            cur_regions: Default::default(),
            anon_region_var: None,
            region_scope_stack: Vec::new(),
            region_depths: Default::default(),
            cur_where_constraints: Vec::new(),
            cur_type_constraints: Vec::new(),
            type_param_count: 0,
            type_param_names: Default::default(),
            type_param_kinds: Default::default(),
            cur_type_params: Default::default(),
            var_count: 0,
            uniq_count: 0,
            global_functions: Default::default(),
            function_signatures: Default::default(),
            entrypoint: None,
            extern_funcs: Default::default(),
            enum_defs: Default::default(),
            enum_names: Default::default(),
            anon_tag_types: Default::default(),
            typeclasses: Default::default(),
            typeclass_names: Default::default(),
            method_index: Default::default(),
            instances: Default::default(),
            copy_class: None,
            clone_class: None,
            send_class: None,
            sync_class: None,
            heaped_class: None,
            unique_instances: Default::default(),
            cur_build_module: None,
            project_modules: Default::default(),
            module_imports: Default::default(),
            default_module: None,
            file_defaults: Default::default(),
            type_refs: Vec::new(),
            diagnostics: Vec::new(),
            cur_file: None,
            type_errors: Vec::new(),
        }
    }

    /// Record a recovered (non-aborting) type error for the current function.
    pub fn push_type_error(&mut self, err: AstTypeError<'tcx>) {
        self.type_errors.push(err);
    }

    /// Drain the recovered type errors collected so far (per-function).
    pub fn take_type_errors(&mut self) -> Vec<AstTypeError<'tcx>> {
        std::mem::take(&mut self.type_errors)
    }

    /// Emit a non-fatal warning at `range` in the file currently being checked
    /// ([`Self::cur_file`]). Collected by `pipeline::run` into the diagnostics
    /// sink; a no-op if no current file is set.
    pub fn warn(&mut self, range: Range, message: impl Into<String>) {
        let Some(file) = self.cur_file else { return };
        self.diagnostics.push(SandDiagnostic {
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            range,
            file: Some(file),
            ..Default::default()
        });
    }

    // ========================== Types ========================================

    /// Shared structural-interning primitive: return the cached handle for
    /// `key`, or build it once (via `build`) and cache it. `build` receives the
    /// key by reference so slice-bearing keys (tuples / applications) can be
    /// read while `key` itself is moved into the cache.
    fn intern_with<K: Ord>(
        cache: &mut Map<K, Ty<'tcx>>,
        key: K,
        build: impl FnOnce(&K) -> Ty<'tcx>,
    ) -> Ty<'tcx> {
        if let Some(&ty) = cache.get(&key) {
            return ty;
        }
        let ty = build(&key);
        cache.insert(key, ty);
        ty
    }

    /// Structurally intern a non-tuple [`TyKind`], returning the [`Ty`] handle
    /// that refers to it. Duplicate calls with identical structure return the
    /// same handle. Use [`Self::intern_tuple`] for `TyKind::Tuple`.
    fn intern_ty(&mut self, kind: TyKind<'tcx>) -> Ty<'tcx> {
        debug_assert!(
            !matches!(kind, TyKind::Tuple(_) | TyKind::App(..)),
            "use intern_tuple / intern_app for slice-bearing types"
        );
        let arenas = self.arenas;
        Self::intern_with(&mut self.ty_interner, kind, |&kind| {
            Ty(arenas.alloc_ty(kind))
        })
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
        let arenas = self.arenas;
        Self::intern_with(&mut self.tuple_interner, elems, |elems| {
            let slice = arenas.alloc_ty_slice(elems);
            Ty(arenas.alloc_ty(TyKind::Tuple(slice)))
        })
    }

    /// Intern a generic enum instantiation `Base<args...>`. The argument count
    /// must match the base enum's declared type-parameter count (callers check
    /// arity and report a user-facing error). Distinct argument lists intern to
    /// distinct types.
    pub fn intern_app(
        &mut self,
        er: AdtRef<'tcx>,
        args: Vec<Ty<'tcx>>,
        regions: Vec<Region>,
    ) -> Ty<'tcx> {
        let arenas = self.arenas;
        Self::intern_with(
            &mut self.app_interner,
            (er, args, regions),
            |(er, args, regions)| {
                let slice = arenas.alloc_ty_slice(args);
                let region_slice = arenas.alloc_region_slice(regions);
                Ty(arenas.alloc_ty(TyKind::App(*er, slice, region_slice)))
            },
        )
    }

    /// # get the _kind of a type_
    /// returns the default kind for a value of that type
    /// (see the Calculus kinding rules).
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

    /// Intern a region-ascribed type `inner @ region` (Calculus: Types).
    pub fn region_ty(&mut self, inner: Ty<'tcx>, region: Region) -> Ty<'tcx> {
        let arenas = self.arenas;
        Self::intern_with(
            &mut self.region_ty_interner,
            (inner, region),
            |&(inner, region)| Ty(arenas.alloc_ty(TyKind::Region(inner, region))),
        )
    }

    /// Intern a shared reference type `&region inner` (Calculus: Types).
    pub fn ref_ty(&mut self, region: Region, inner: Ty<'tcx>) -> Ty<'tcx> {
        self.intern_ty(TyKind::Ref(region, inner))
    }

    /// Intern an exclusive reference type `&region mut inner` (Calculus:
    /// Types).
    pub fn ref_mut_ty(&mut self, region: Region, inner: Ty<'tcx>) -> Ty<'tcx> {
        self.intern_ty(TyKind::RefMut(region, inner))
    }

    /// Pre-intern `&'static T` for every currently-interned `T`.
    ///
    /// MIR lowering's interior-borrow temporaries (`explicate_control`'s
    /// `borrow_ref_ty`) need these reference types, but that pass runs in
    /// parallel over functions holding a shared `&CompileCtx` and **must not**
    /// mutate the arena (`Arenas: Sync` is justified only by being read-only
    /// after compilation). Interning them here (once, single-threaded, at the
    /// end of monomorphisation) lets lowering look them up read-only via
    /// [`lookup_static_ref`](Self::lookup_static_ref).
    ///
    /// This is a superset of what lowering needs: every `borrow_ref_ty` input
    /// is already an interned program type, so the corresponding `&'static
    /// T` is guaranteed present and the lookup is total.
    pub fn intern_static_refs(&mut self) {
        // The type interner is split across several caches (scalars/refs,
        // tuples, generic apps, …); collect every interned `Ty` from all of
        // them, then intern `&'static T` for each.
        let all: Vec<Ty<'tcx>> = self
            .ty_interner
            .values()
            .copied()
            .chain(self.tuple_interner.values().copied())
            .chain(self.app_interner.values().copied())
            .chain(self.region_ty_interner.values().copied())
            .chain(self.param_app_interner.values().copied())
            .collect();
        all.into_iter().for_each(|t| {
            self.ref_ty(Region::Static, t);
        });
    }

    /// The interned `&'static inner`, if present (pre-interned by
    /// [`intern_static_refs`](Self::intern_static_refs)). `&self`, so it is
    /// callable from the parallel MIR-lowering pass without mutating the arena.
    pub fn lookup_static_ref(&self, inner: Ty<'tcx>) -> Option<Ty<'tcx>> {
        self.ty_interner
            .get(&TyKind::Ref(Region::Static, inner))
            .copied()
    }

    /// Intern a raw pointer type `Ptr<inner>`. Unlike a
    /// reference, it carries no region and survives monomorphisation.
    pub fn ptr_ty(&mut self, inner: Ty<'tcx>) -> Ty<'tcx> {
        self.intern_ty(TyKind::Ptr(inner))
    }

    /// Intern a function type `arg -> ret` with the given calling mode and a
    /// capture-free (`Unit`) environment. Used for top-level functions and for
    /// signature/abstract arrow positions.
    pub fn fn_ty(&mut self, arg: Ty<'tcx>, ret: Ty<'tcx>, mode: FnMode) -> Ty<'tcx> {
        let env = self.types.unit;
        self.intern_ty(TyKind::Fn(arg, ret, mode, env))
    }

    /// Intern a closure type `arg -> ret` carrying an explicit environment type
    /// (the structural tuple of its captures). See [`TyKind::Fn`].
    pub fn closure_ty(
        &mut self,
        arg: Ty<'tcx>,
        ret: Ty<'tcx>,
        mode: FnMode,
        env: Ty<'tcx>,
    ) -> Ty<'tcx> {
        self.intern_ty(TyKind::Fn(arg, ret, mode, env))
    }

    /// Intern a higher-kinded parameter application `F<args>`.
    pub fn param_app_ty(&mut self, param: TypeParamId, args: Vec<Ty<'tcx>>) -> Ty<'tcx> {
        let arenas = self.arenas;
        Self::intern_with(
            &mut self.param_app_interner,
            (param, args),
            |(param, args)| {
                let slice = arenas.alloc_ty_slice(args);
                Ty(arenas.alloc_ty(TyKind::ParamApp(*param, slice)))
            },
        )
    }

    /// Intern a partial-application **hole** `_ᵢ` (Calculus: Partial
    /// application, [`TyKind::Hole`]). Well-formed only inside a
    /// constructor-abstraction head (the binding of a higher-kinded
    /// parameter, or an `impl` head).
    pub fn hole_ty(&mut self, idx: u32) -> Ty<'tcx> {
        self.intern_ty(TyKind::Hole(idx))
    }

    /// The **constructor kind** of a type viewed as a type constructor
    /// (Calculus: Partial application, generalised `K-App`): `Owned` when it is
    /// a saturated, hole-free value type; otherwise the arrow `k_{h₁} → … →
    /// k_{hₘ} → Owned` over the kinds of its holes, in slot order. Holes
    /// are the explicit `Hole` arguments of an `App` *plus* the implicit
    /// trailing holes of an under-saturated one (`args.len() < arity`). A
    /// non-`App` type has no holes, so this reduces to its value kind
    /// ([`kind_of`](Self::kind_of)).
    pub fn constructor_kind(&mut self, ty: Ty<'tcx>) -> Kind {
        // A bare `Enum(er)` is the all-holes abstraction (every parameter is a
        // hole), equivalent to `App(er, [])`; anything else non-`App` is a value
        // type with no holes.
        let (er, args): (AdtRef<'tcx>, &[Ty<'tcx>]) = match ty.kind() {
            TyKind::App(er, args, _) => (*er, args),
            TyKind::Enum(er) => (*er, &[]),
            _ => return self.kind_of(ty),
        };
        // Copy the declared parameter kinds out first, so the immutable
        // `get_enum` borrow is released before `intern_kind` (needs `&mut self`).
        let param_kinds: Vec<Kind> = self
            .get_enum(er)
            .type_params
            .iter()
            .map(|p| p.kind)
            .collect();
        let mut hole_kinds: Vec<Kind> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            if matches!(a.kind(), TyKind::Hole(_)) && i < param_kinds.len() {
                hole_kinds.push(param_kinds[i]);
            }
        }
        // trailing under-saturation: the un-supplied parameters are implicit holes.
        for &k in param_kinds.iter().skip(args.len()) {
            hole_kinds.push(k);
        }
        // curry right-associatively over `Owned`.
        hole_kinds
            .into_iter()
            .rev()
            .fold(Kind::Owned, |acc, k| self.intern_kind(k, acc))
    }

    /// Intern the arrow kind `from -> to`, returning the canonical
    /// `Kind::Arrow`. Equal arrows share one [`KindId`].
    pub fn intern_kind(&mut self, from: Kind, to: Kind) -> Kind {
        if let Some(&id) = self.kind_arrow_ids.get(&(from, to)) {
            return Kind::Arrow(id);
        }
        let id = KindId(self.kind_arrows.len());
        self.kind_arrows.push((from, to));
        self.kind_arrow_ids.insert((from, to), id);
        Kind::Arrow(id)
    }

    /// The `(domain, codomain)` of an interned arrow kind.
    pub fn kind_arrow(&self, id: KindId) -> (Kind, Kind) {
        self.kind_arrows[id.0]
    }

    /// Render a kind for diagnostics (`Owned`, `Never`, `Owned -> Owned`, ...).
    pub fn display_kind(&self, k: Kind) -> String {
        match k {
            Kind::Owned => "Owned".to_string(),
            Kind::Borrowed => "Borrowed".to_string(),
            Kind::BorrowedMut => "BorrowedMut".to_string(),
            Kind::Never => "Never".to_string(),
            Kind::Arrow(id) => {
                let (from, to) = self.kind_arrow(id);
                format!("{} -> {}", self.display_kind(from), self.display_kind(to))
            }
        }
    }

    /// Canonicalise the region of every reference (`&'r T`, `&'r mut T`) in
    /// `ty` to the shared anonymous region. Reference types carry no
    /// *type-level* region constraints: region safety is the lexical
    /// escape check, which reads the borrow's `Kind`, not its type, so at
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
            TyKind::App(er, args, regions) => {
                let er = *er;
                let args: Vec<Ty<'tcx>> = args.iter().map(|a| self.region_erase(*a)).collect();
                // canonicalise each region arg to the shared elided region.
                let anon = self.anon_region();
                let regions: Vec<Region> = regions
                    .iter()
                    .map(|r| {
                        if matches!(r, Region::Static) {
                            *r
                        } else {
                            anon
                        }
                    })
                    .collect();
                self.intern_app(er, args, regions)
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
            // raw pointers carry no region of their own, but their element may
            // (e.g. `Ptr<&'r T>`): recurse so it is canonicalised too.
            TyKind::Ptr(inner) => {
                let inner = self.region_erase(*inner);
                self.ptr_ty(inner)
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
            TyKind::App(er, args, regions) => {
                let er = *er;
                let args: Vec<Ty<'tcx>> =
                    args.iter().map(|a| self.region_fill(*a, region)).collect();
                let regions: Vec<Region> = regions.iter().map(pick).collect();
                self.intern_app(er, args, regions)
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
            TyKind::Ptr(inner) => {
                let inner = self.region_fill(*inner, region);
                self.ptr_ty(inner)
            }
            _ => ty,
        }
    }

    /// The result region of a call (Calculus: Region Substitution at Call
    /// Sites): the greatest lower
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
    /// type-parameter [`unify`](crate::passes::type_ast::generics::unify): the
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

    /// Infer the region arguments of a constructor `E#V(payload)` (R5c), in the
    /// enum's declared region-parameter order. Each region parameter is pinned
    /// by the actual payload's regions (unify the *declared* payload type,
    /// which names the region params, against the *actual* payload type); a
    /// parameter not pinned by the payload (e.g. a nullary variant, or a
    /// phantom) is seeded from the expected type's region args when
    /// available, else defaults to the call-site scope (the safe shortest
    /// region).
    pub fn infer_ctor_region_args(
        &self,
        region_params: &[RegionParam],
        declared_payload: Option<Ty<'tcx>>,
        actual_payload: Option<Ty<'tcx>>,
        expected_region_args: Option<&[Region]>,
    ) -> Vec<Region> {
        let solve: Set<RegionVar> = region_params.iter().map(|p| p.region).collect();
        let mut bound: Map<RegionVar, Vec<Region>> = Map::new();
        if let (Some(decl), Some(actual)) = (declared_payload, actual_payload) {
            collect_region_bindings(decl, actual, &solve, &mut bound);
        }
        let call_site = self.current_scope_region();
        region_params
            .iter()
            .enumerate()
            .map(|(i, p)| match bound.get(&p.region) {
                Some(regions) => self.region_meet(regions, &[]),
                None => expected_region_args
                    .and_then(|regs| regs.get(i).copied())
                    .unwrap_or(call_site),
            })
            .collect()
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
            TyKind::App(er, args, regions) => {
                let er = *er;
                let args: Vec<Ty<'tcx>> = args
                    .iter()
                    .map(|a| self.region_subst_ty(*a, subst))
                    .collect();
                let regions: Vec<Region> = regions
                    .iter()
                    .map(|r| apply_region_subst(*r, subst))
                    .collect();
                self.intern_app(er, args, regions)
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
            TyKind::Ptr(inner) => {
                let inner = self.region_subst_ty(*inner, subst);
                self.ptr_ty(inner)
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
    /// an elided `&T` type and an elided `&e` value compare equal.
    pub fn anon_region(&mut self) -> Region {
        if let Some(rv) = self.anon_region_var {
            return Region::Var(rv);
        }
        let rv = RegionVar(self.region_count);
        self.region_count += 1;
        self.anon_region_var = Some(rv);
        Region::Var(rv)
    }

    // ---- lexical region scopes + the outlives solver ------------------------

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
    /// [`Self::enter_region_scope`]): i.e. a *local* region (the function
    /// frame or a block), as opposed to a *region parameter*, the
    /// elided-borrow region, or `'static` (all of which outlive the frame).
    /// Used by the function-return escape check: only non-scope (outer)
    /// regions may be named by a returned value's type (Calculus: The Escape
    /// Check, frame boundary).
    pub fn is_scope_region(&self, r: Region) -> bool {
        matches!(r, Region::Var(rv) if self.region_depths.contains_key(&rv))
    }

    /// Does `longer` outlive `shorter` (`longer ≥ shorter`; Calculus: Regions)?
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
        // all at depth 0 (outermost), so a caller lifetime or the function frame
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
    pub fn enum_ty(&self, er: AdtRef<'tcx>) -> Ty<'tcx> {
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
            self.type_param_kinds.insert(id, spec.kind);
            scope.insert(spec.name.clone(), id);
            params.push(TypeParam {
                id,
                name: spec.name.clone(),
                range: spec.range,
                variance: spec.variance,
                explicit_variance: spec.explicit_variance,
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

    /// Allocate fresh type parameters and **add** them to the current scope
    /// used for a typeclass method's own generics, which are in
    /// scope *alongside* the class parameter. Pair with
    /// [`Self::retract_type_params`] to remove them again.
    pub fn extend_type_params(&mut self, specs: &[TypeParamSpec]) -> Vec<TypeParam> {
        let mut params = Vec::with_capacity(specs.len());
        for spec in specs {
            let id = TypeParamId(self.type_param_count);
            self.type_param_count += 1;
            self.type_param_names.insert(id, spec.name.clone());
            self.type_param_kinds.insert(id, spec.kind);
            self.cur_type_params.insert(spec.name.clone(), id);
            params.push(TypeParam {
                id,
                name: spec.name.clone(),
                range: spec.range,
                variance: spec.variance,
                explicit_variance: spec.explicit_variance,
                kind: spec.kind,
            });
        }
        params
    }

    /// Remove type parameters added by [`Self::extend_type_params`] from the
    /// current scope (by name), leaving the enclosing scope intact.
    pub fn retract_type_params(&mut self, params: &[TypeParam]) {
        for p in params {
            self.cur_type_params.remove(&p.name);
        }
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
    /// The declared kind of a type parameter: `Owned` for ordinary
    /// params, an arrow for higher-kinded ones. Defaults to `Owned` for ids
    /// with no recorded kind (none should occur in practice).
    pub fn type_param_kind(&self, id: TypeParamId) -> Kind {
        self.type_param_kinds
            .get(&id)
            .copied()
            .unwrap_or(Kind::Owned)
    }

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

    /// Mint a fresh, uniquified variable not backed by any source `Pair`,
    /// used by compiler-synthesised bindings (e.g. the heap lowering's
    /// `unique_take` node temporaries and the all-binding match desugaring).
    pub fn fresh_synthetic_var(
        &mut self,
        name: &str,
        declaration: Range,
        inst: crate::compiler::structure::VarDeclType,
    ) -> UniqVar<'tcx> {
        let id = self.var_count;
        self.var_count += 1;
        let var = OriginalVar {
            name: crate::compiler::structure::VarName::synthetic(name),
            declaration,
            inst,
            id,
        };
        let r = self.arenas.alloc_variable(var);
        self.uniquify_original_variable(OriginalVarRef(r))
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

    /// Fallible [`Self::fun_sig`]: `None` when no signature is registered for
    /// `fun` (e.g. a typeclass default/impl method a method call was rewritten
    /// to; the type checker builds that `Call` directly and never looks its
    /// signature up). Used by best-effort consumers like LSP hover that must
    /// not panic on such a `FunRef`.
    pub fn try_fun_sig(&self, fun: &FunRef<'tcx>) -> Option<FunSig<'tcx>> {
        self.function_signatures.get(fun).cloned()
    }

    pub fn set_fun_sig(&mut self, fun: FunRef<'tcx>, sig: FunSig<'tcx>) {
        let out = self.function_signatures.insert(fun, sig);
        debug_assert!(out.is_none());
    }

    pub fn is_main(&self, fun: FunRef<'tcx>) -> bool {
        self.entrypoint == Some(fun)
    }

    /// Record `fun` as an external (FFI) function bound to the
    /// given C symbol in `module`.
    pub fn register_extern(&mut self, fun: FunRef<'tcx>, module: ModuleRef<'tcx>, symbol: String) {
        self.extern_funcs.insert(fun, (module, symbol));
    }

    /// Whether `fun` is an external (FFI) function with no Sand body.
    pub fn is_extern(&self, fun: FunRef<'tcx>) -> bool {
        self.extern_funcs.contains_key(&fun)
    }

    /// The C symbol an external function is bound to, if it is one.
    pub fn extern_symbol(&self, fun: FunRef<'tcx>) -> Option<&str> {
        self.extern_funcs.get(&fun).map(|(_, s)| s.as_str())
    }

    /// All registered external functions, as `(FunRef, C symbol)` pairs.
    pub fn extern_functions(&self) -> impl Iterator<Item = (FunRef<'tcx>, &str)> + '_ {
        self.extern_funcs.iter().map(|(f, (_, s))| (*f, s.as_str()))
    }

    /// The external functions declared in `module` (for unqualified-call
    /// visibility in [`crate::passes::qualify`]).
    pub fn externs_in_module(&self, module: ModuleRef<'tcx>) -> Vec<FunRef<'tcx>> {
        self.extern_funcs
            .iter()
            .filter(|(_, (m, _))| *m == module)
            .map(|(f, _)| *f)
            .collect()
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
    #[allow(clippy::too_many_arguments)]
    pub fn register_enum(
        &mut self,
        name: &str,
        variant_names: Vec<String>,
        type_params: Vec<TypeParam>,
        region_params: Vec<RegionParam>,
        range: Range,
        module: ModuleRef<'tcx>,
        derives: Vec<crate::compiler::structure::Derivable>,
    ) -> Result<AdtRef<'tcx>, ContextError> {
        if let Some(existing) = self.enum_names.get(name) {
            return Err(ContextError::DuplicateEnum {
                name: name.to_string(),
                first: existing.0.range,
                second: range,
            });
        }
        let id = self.enum_defs.len();
        let def = AdtDef {
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
            derives,
        };
        let er = AdtRef(self.arenas.alloc_enum(def));
        self.enum_defs.push(er);
        self.enum_names.insert(name.to_string(), er);
        self.intern_ty(TyKind::Enum(er));
        Ok(er)
    }

    /// Phase 2 of enum registration: attach a resolved payload type to a
    /// variant that was registered (with `payload: None`) by
    /// [`Self::register_enum`]. Uses the variant's `Cell` so the shared,
    /// arena-allocated `AdtDef` does not need to be mutably re-borrowed.
    pub fn set_variant_payload(&mut self, er: AdtRef<'tcx>, variant_idx: usize, payload: Ty<'tcx>) {
        er.0.variants[variant_idx].payload.set(Some(payload));
    }

    /// Every registered enum, in registration order (recursion detection and
    /// `Heaped` legality use this).
    pub fn all_enums(&self) -> impl Iterator<Item = AdtRef<'tcx>> + '_ {
        self.enum_defs.iter().copied()
    }

    pub fn get_enum(&self, er: AdtRef<'tcx>) -> &'tcx AdtDef<'tcx> {
        er.0
    }

    /// Returns a [`Display`]-able wrapper for [`Ty`] that resolves enum names
    /// via this context.
    pub fn display_ty<'a>(&'a self, ty: Ty<'tcx>) -> TyDisplay<'a, 'tcx> {
        TyDisplay { ty, ctx: self }
    }

    pub fn lookup_enum_by_name(&self, name: &str) -> Option<AdtRef<'tcx>> {
        self.enum_names.get(name).copied()
    }

    pub fn lookup_variant(&self, er: AdtRef<'tcx>, variant: &str) -> Option<usize> {
        er.0.variants.iter().position(|v| v.name == variant)
    }

    pub fn lookup_enum_in_module(
        &self,
        module: ModuleRef<'tcx>,
        name: &str,
    ) -> Option<AdtRef<'tcx>> {
        self.enum_defs
            .iter()
            .find(|er| er.0.src_module == module && er.0.name == name)
            .copied()
    }

    /// Resolve a bare (unqualified) type name lexically: the importing
    /// `module`, then its explicit `use` imports, then its glob `use` imports,
    /// then the prelude (`core`). A type in another *user* module is reachable
    /// only if qualified (`mod::Type`) or `use`d. (`core` defines no types yet,
    /// so the prelude layer is empty.)
    pub fn lookup_enum_scoped(&self, name: &str, module: ModuleRef<'tcx>) -> Option<AdtRef<'tcx>> {
        // own module
        if let Some(er) = self.lookup_enum_in_module(module, name) {
            return Some(er);
        }
        // explicit `use src::name`
        if let Some(src) = self.explicit_import(module, name)
            && let Some(er) = self.lookup_enum_in_module(src, name)
        {
            return Some(er);
        }
        // glob `use src::*`: two distinct hits are ambiguous and resolve to
        // nothing, forcing a qualification rather than a silent pick.
        let mut glob_hit: Option<AdtRef<'tcx>> = None;
        for g in self.glob_import_modules(module) {
            if let Some(er) = self.lookup_enum_in_module(g, name) {
                match glob_hit {
                    None => glob_hit = Some(er),
                    Some(prev) if prev == er => {}
                    Some(_) => {
                        glob_hit = None;
                        break;
                    }
                }
            }
        }
        if let Some(er) = glob_hit {
            return Some(er);
        }
        // prelude (core)
        self.core_module()
            .and_then(|core| self.lookup_enum_in_module(core, name))
    }

    /// [`Self::lookup_enum_scoped`] against the module currently being built
    /// (set by [`Self::set_build_module`]). Used by `build_type` to resolve
    /// bare type names during AST construction.
    pub fn lookup_enum_current(&self, name: &str) -> Option<AdtRef<'tcx>> {
        self.lookup_enum_scoped(name, self.cur_build_module?)
    }

    // ---- module imports (`use`) ---------------------------------------------

    /// Record a `use src::name;` (item = `Some(name)`) or `use src::*;`
    /// (item = `None`) declared in `importing`.
    pub fn add_import(&mut self, importing: ModuleRef<'tcx>, source: String, item: Option<String>) {
        let entry = self.module_imports.entry(importing).or_default();
        match item {
            Some(name) => {
                entry.explicit.insert(name, source);
            }
            None => entry.globs.push(source),
        }
    }

    /// The source module a `name` is explicitly imported from
    /// (`use src::name`).
    pub fn explicit_import(
        &self,
        importing: ModuleRef<'tcx>,
        name: &str,
    ) -> Option<ModuleRef<'tcx>> {
        let src = self.module_imports.get(&importing)?.explicit.get(name)?;
        self.get_mod_by_name(src)
    }

    /// The modules glob-imported by `importing` (`use src::*`).
    pub fn glob_import_modules(&self, importing: ModuleRef<'tcx>) -> Vec<ModuleRef<'tcx>> {
        self.module_imports
            .get(&importing)
            .map(|i| {
                i.globs
                    .iter()
                    .filter_map(|n| self.get_mod_by_name(n))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The `core` module (the prelude), if registered.
    pub fn core_module(&self) -> Option<ModuleRef<'tcx>> {
        self.project_modules
            .iter()
            .copied()
            .find(|m| self.is_core_module(self.file_of_module(*m)))
    }

    /// Record the module currently being processed during AST building so that
    /// anonymous tag-union types can be attributed to the right module.
    pub fn set_build_module(&mut self, m: ModuleRef<'tcx>) {
        self.cur_build_module = Some(m);
    }

    /// Record that the source span `range` (in the file of the module currently
    /// being built) is a reference to `target`. Used
    /// for go-to-definition on type positions. No-op when no build module
    /// is set (no file context to attribute the span to).
    pub fn record_type_ref(&mut self, range: Range, target: DefTarget<'tcx>) {
        if let Some(m) = self.cur_build_module {
            let file = self.file_of_module(m);
            self.type_refs.push(TypeRefEntry {
                file,
                range,
                target,
            });
        }
    }

    /// The definition a type/typeclass name reference at `pos` in `file` points
    /// at, if any. Returns the *innermost* (smallest-span) match so a nested
    /// reference like the `Int` in `Option<Int>` wins over the enclosing one.
    pub fn type_ref_at(&self, file: FileRef, pos: Pos) -> Option<DefTarget<'tcx>> {
        fn contains(r: Range, p: Pos) -> bool {
            (p.line, p.col) >= (r.start.line, r.start.col)
                && (p.line, p.col) <= (r.end.line, r.end.col)
        }
        fn span(r: Range) -> (usize, usize) {
            (r.end.line - r.start.line, r.end.col)
        }
        self.type_refs
            .iter()
            .filter(|e| e.file == file && contains(e.range, pos))
            .min_by_key(|e| span(e.range))
            .map(|e| e.target)
    }

    /// Intern an ad-hoc tag-union type from a sorted, deduplicated list of tag
    /// names. Returns the same `EnumRef` for any two call sites with the same
    /// tag set (structural identity).
    ///
    /// The resulting `AdtDef` uses a sorted `variants` list so that variant
    /// indices are stable regardless of the order tags were written.
    pub fn register_or_get_anon_enum(
        &mut self,
        mut tags: Vec<String>,
        range: Range,
    ) -> AdtRef<'tcx> {
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
        let def = AdtDef {
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
            // anonymous tag-unions are never recursive and derive nothing.
            derives: Vec::new(),
        };
        let er = AdtRef(self.arenas.alloc_enum(def));
        self.enum_defs.push(er);
        // NOTE: anonymous enums are not inserted into `enum_names`; they are
        // only looked up via `anon_tag_types`.
        self.anon_tag_types.insert(tags, er);
        self.intern_ty(TyKind::Enum(er));
        er
    }

    pub fn enum_display(&self, enum_ref: AdtRef<'tcx>, variant: usize) -> String {
        let variants = &enum_ref.0.variants;
        if variants.len() <= variant {
            internal_bug!("enum display indexed with oob variant: {enum_ref:?}[{variant}]");
        }
        variants[variant].name.clone()
    }

    // ========================== Typeclasses =================================

    /// The instance-coherence head of a type: its outermost constructor. `None`
    /// for types that cannot (yet) carry an instance (references, tuples, type
    /// parameters, region-ascribed types).
    pub fn type_head(&self, ty: Ty<'tcx>) -> Option<TypeHead<'tcx>> {
        match ty.kind() {
            TyKind::Int => Some(TypeHead::Int),
            TyKind::Bool => Some(TypeHead::Bool),
            TyKind::Unit => Some(TypeHead::Unit),
            TyKind::Enum(er) => Some(TypeHead::Enum(*er)),
            TyKind::App(er, _, _) => Some(TypeHead::Enum(*er)),
            _ => None,
        }
    }

    /// Register a typeclass, returning its ref. Also indexes its name and every
    /// method name (the one-class-per-method-name rule is validated
    /// separately).
    pub fn register_typeclass(&mut self, def: TypeclassDef<'tcx>) -> TypeclassRef {
        let tref = TypeclassRef(self.typeclasses.len());
        // Recognise the `Copy` / `Clone` lang-items by name (declared in
        // `core.sand`) so the ownership pass can drive implicit copy.
        match def.name.as_str() {
            "Copy" => self.copy_class = Some(tref),
            "Clone" => self.clone_class = Some(tref),
            // Thread-safety markers; the compiler reasons about them
            // structurally (see `structurally_marked`).
            "Send" => self.send_class = Some(tref),
            "Sync" => self.sync_class = Some(tref),
            // Reserved; harmless until `core.sand` declares `Heaped`.
            "Heaped" => self.heaped_class = Some(tref),
            _ => {}
        }
        self.typeclass_names.insert(def.name.clone(), tref);
        // Index method names now (from declaration order) so the skeleton's
        // methods can be filled in a later resolve phase.
        for name in &def.method_order {
            self.method_index.insert(name.clone(), tref);
        }
        self.typeclasses.push(def);
        tref
    }

    /// Record the generic default-body function for a typeclass method
    /// so impls that omit the method can dispatch to it.
    pub fn set_method_default_fn(&mut self, class: TypeclassRef, method: &str, fr: FunRef<'tcx>) {
        if let Some(m) = self.typeclasses[class.0].methods.get_mut(method) {
            m.default_fn = Some(fr);
        }
    }

    /// Fill in a class skeleton's resolved method signatures and superclass
    /// refs (second phase, once all classes + enums exist).
    pub fn set_typeclass_methods(
        &mut self,
        tref: TypeclassRef,
        methods: Map<String, crate::compiler::structure::MethodDef<'tcx>>,
        superclasses: Vec<TypeclassRef>,
    ) {
        let def = &mut self.typeclasses[tref.0];
        def.methods = methods;
        def.superclasses = superclasses;
    }

    pub fn lookup_typeclass(&self, name: &str) -> Option<TypeclassRef> {
        self.typeclass_names.get(name).copied()
    }

    /// The `Clone` lang-item class, if `core.sand` declared it.
    pub fn clone_class(&self) -> Option<TypeclassRef> {
        self.clone_class
    }

    /// The `Copy` lang-item class, if `core.sand` declared it.
    pub fn copy_class(&self) -> Option<TypeclassRef> {
        self.copy_class
    }

    /// The `Heaped` lang-item class, if `core.sand` declared it. `None` until
    /// then; the registration hook is reserved ahead of time.
    pub fn heaped_class(&self) -> Option<TypeclassRef> {
        self.heaped_class
    }

    /// The `Unique<T>` handle enum lang-item: the core-lib
    /// `type Unique<T> = U(Ptr<T>)` backing the unique heap strategy. The heap
    /// lowering rewrites every `deriving Heaped` value into a `Unique<Node>`,
    /// and codegen recognizes this enum to make dropping a handle a
    /// `unique_release`. Resolved by name (like the other lang-items); `None`
    /// if `core.sand` has not been loaded.
    pub fn unique_enum(&self) -> Option<AdtRef<'tcx>> {
        self.lookup_enum_by_name("Unique")
    }

    /// Record that `er` is a monomorphised `Unique<…>` instance;
    /// called by monomorphisation when it specialises the `Unique` lang-item.
    pub fn mark_unique_instance(&mut self, er: AdtRef<'tcx>) {
        self.unique_instances.insert(er);
    }

    /// Whether `er` is a (monomorphised) `Unique<…>` heap handle: i.e.
    /// dropping a value of this enum must `free` its backing allocation.
    pub fn is_unique_handle(&self, er: AdtRef<'tcx>) -> bool {
        self.unique_instances.contains(&er)
    }

    /// Look up a (source or synthesised) global function by its simple name.
    /// Used by the heap lowering to resolve the core-lib strategy functions
    /// (`unique_alloc` / `unique_take` / `unique_release`) it injects. Runs
    /// pre-monomorphisation, so the generic definitions are still present and
    /// names are unambiguous.
    pub fn lookup_function_by_name(&self, name: &str) -> Option<FunRef<'tcx>> {
        self.global_functions
            .iter()
            .copied()
            .find(|f| self.original_fun_name(*f) == name)
    }

    /// **The single source of truth** for "does `ty` satisfy typeclass
    /// `class`?", used by *both* the trait checker (`where T : C` resolution)
    /// and the affine/move checker (`Copy`/`Clone`). It unifies three sources
    /// that previously disagreed (causing e.g. `&T` to be `Copy` to the move
    /// checker but not to a `where T : Copy` bound):
    ///
    ///   1. a **type parameter** licensed by an in-scope assumption (`where T :
    ///      C'` where `C'` is `C` or a subclass);
    ///   2. the **builtin structural** instances of the `Copy`/`Clone`
    ///      lang-items (shared references, raw pointers, and, structurally,
    ///      tuples), which have no surface `impl`;
    ///   3. a **registered `impl`** (its superclasses are registered too, per
    ///      `requires`, so an exact lookup suffices).
    pub fn satisfies(
        &self,
        class: TypeclassRef,
        ty: Ty<'tcx>,
        assumptions: &[crate::compiler::structure::TypeConstraint],
    ) -> bool {
        // 1. a type parameter is satisfied only by an in-scope bound.
        if let TyKind::Param(pid) = ty.kind() {
            return assumptions
                .iter()
                .any(|tc| tc.param == *pid && self.class_satisfies(tc.class, class));
        }
        // 2. builtin structural instances of the `Copy`/`Clone` lang-items.
        if self.is_structural_copy_class(class)
            && self.structurally_copyable(class, ty, assumptions)
        {
            return true;
        }
        // 2b. builtin structural instances of the `Send`/`Sync` marker lang-items.
        if self.is_marker_class(class) && self.structurally_marked(class, ty, assumptions) {
            return true;
        }
        // 3. a registered instance.
        self.type_head(ty)
            .is_some_and(|head| self.lookup_instance(class, head).is_some())
    }

    /// Whether `class` is the `Copy` or `Clone` lang-item, which carry builtin
    /// structural instances (references, pointers, tuples) beyond any surface
    /// `impl`.
    fn is_structural_copy_class(&self, class: TypeclassRef) -> bool {
        self.copy_class == Some(class) || self.clone_class == Some(class)
    }

    /// Whether `class` is the `Send` or `Sync` thread-safety marker.
    fn is_marker_class(&self, class: TypeclassRef) -> bool {
        self.send_class == Some(class) || self.sync_class == Some(class)
    }

    /// The builtin structural `Send`/`Sync` instances. Primitives are
    /// `Send + Sync`; a shared `&T` is `Send`/`Sync` iff `T : Sync` (a shared
    /// borrow may cross or be shared only when the referent is
    /// thread-shareable); a `&mut T` follows `T` for the *same* marker; a
    /// region-annotated type and a tuple are structural; a raw `Ptr<T>` is
    /// neither (it escapes the ownership discipline). Anything else defers
    /// to a registered `impl`.
    fn structurally_marked(
        &self,
        class: TypeclassRef,
        ty: Ty<'tcx>,
        assumptions: &[crate::compiler::structure::TypeConstraint],
    ) -> bool {
        match ty.kind() {
            TyKind::Int | TyKind::Bool | TyKind::Unit => true,
            // `&T : Send` and `&T : Sync` both require `T : Sync`.
            TyKind::Ref(_, t) => self
                .sync_class
                .is_some_and(|sync| self.satisfies(sync, *t, assumptions)),
            // `&mut T : Send` ⟺ `T : Send`; `&mut T : Sync` ⟺ `T : Sync`.
            TyKind::RefMut(_, t) => self.satisfies(class, *t, assumptions),
            TyKind::Region(t, _) => self.satisfies(class, *t, assumptions),
            TyKind::Tuple(elems) => elems.iter().all(|e| self.satisfies(class, *e, assumptions)),
            _ => false,
        }
    }

    /// The builtin structural `Copy`/`Clone` instances: a shared `&T` and a raw
    /// `Ptr<T>` are copyable (a `&mut T` is **not**: it is move-only), a
    /// region-annotated type follows its inner type, and a tuple is copyable
    /// iff every element is. Primitives are included so the predicate
    /// stands even independently of `core.sand`'s `impl Copy for Int`.
    fn structurally_copyable(
        &self,
        class: TypeclassRef,
        ty: Ty<'tcx>,
        assumptions: &[crate::compiler::structure::TypeConstraint],
    ) -> bool {
        match ty.kind() {
            TyKind::Int | TyKind::Bool | TyKind::Unit => true,
            TyKind::Ref(..) | TyKind::Ptr(_) => true,
            TyKind::Region(t, _) => self.satisfies(class, *t, assumptions),
            TyKind::Tuple(elems) => elems.iter().all(|e| self.satisfies(class, *e, assumptions)),
            _ => false,
        }
    }

    /// Whether `ty` can be cloned. Thin wrapper over
    /// [`satisfies`](Self::satisfies).
    pub fn is_clone(&self, ty: Ty<'tcx>) -> bool {
        self.clone_class.is_some_and(|c| self.satisfies(c, ty, &[]))
    }

    /// Whether `ty` is implicitly copied on use. Thin wrapper over
    /// [`satisfies`](Self::satisfies); `where T : Copy` on a type parameter is
    /// handled by [`is_copy_under`](Self::is_copy_under).
    pub fn is_copy(&self, ty: Ty<'tcx>) -> bool {
        self.copy_class.is_some_and(|c| self.satisfies(c, ty, &[]))
    }

    /// Like [`is_copy`](Self::is_copy), but a type parameter additionally
    /// counts as `Copy` when the in-scope `constraints` bind it to `Copy`
    /// (a `where T : Copy`). Used by the ownership pass inside generic
    /// functions.
    pub fn is_copy_under(
        &self,
        ty: Ty<'tcx>,
        constraints: &[crate::compiler::structure::TypeConstraint],
    ) -> bool {
        self.copy_class
            .is_some_and(|c| self.satisfies(c, ty, constraints))
    }

    pub fn get_typeclass(&self, tref: TypeclassRef) -> &TypeclassDef<'tcx> {
        &self.typeclasses[tref.0]
    }

    /// The class that owns method `name`, if any (call-resolution entry point).
    pub fn method_class(&self, name: &str) -> Option<TypeclassRef> {
        self.method_index.get(name).copied()
    }

    pub fn all_typeclasses(&self) -> impl Iterator<Item = TypeclassRef> + '_ {
        (0..self.typeclasses.len()).map(TypeclassRef)
    }

    /// Look up the instance for `(class, head)`, if registered.
    pub fn lookup_instance(
        &self,
        class: TypeclassRef,
        head: TypeHead<'tcx>,
    ) -> Option<&ImplDef<'tcx>> {
        // The head-only lookup used by ground classes (`Copy`/`Clone`/superclass
        // presence) and by the single-instance case. A higher-kinded bucket with
        // several candidates is disambiguated by the receiver via
        // [`resolve_instance`](Self::resolve_instance); this returns the first.
        self.instances.get(&(class, head)).and_then(|v| v.first())
    }

    /// Resolve the instance for a `class` method call on a value of type
    /// `value` (Calculus: Typeclasses, resolution). Among the candidates
    /// bucketed under `value`'s head constructor, returns the first whose
    /// head abstraction **unifies** with `value`: its `Hole`s match the
    /// class-operated slots, its fixed slots pin the rest. The returned
    /// `for_ty` is what the class parameter `F` binds to (so `F<…>`
    /// β-reduces at the call site), and the method map selects the concrete
    /// impl functions. `None` if nothing matches.
    pub fn resolve_instance(
        &self,
        class: TypeclassRef,
        value: Ty<'tcx>,
    ) -> Option<(Ty<'tcx>, Map<String, FunRef<'tcx>>)> {
        use crate::passes::type_ast::generics::Subst;
        use crate::passes::type_ast::generics::unify;
        let head = self.type_head(value)?;
        let bucket = self.instances.get(&(class, head))?;
        bucket.iter().find_map(|idef| {
            let mut probe = Subst::new();
            unify(self, idef.for_ty, value, &mut probe)
                .ok()
                .map(|()| (idef.for_ty, idef.methods.clone()))
        })
    }

    /// Enter the current function's `where T : C` constraints as assumptions
    /// (used while resolving method calls on its type parameters); returns the
    /// previous set to restore on exit.
    pub fn set_type_assumptions(
        &mut self,
        constraints: Vec<crate::compiler::structure::TypeConstraint>,
    ) -> Vec<crate::compiler::structure::TypeConstraint> {
        std::mem::replace(&mut self.cur_type_constraints, constraints)
    }

    pub fn type_assumptions(&self) -> &[crate::compiler::structure::TypeConstraint] {
        &self.cur_type_constraints
    }

    /// Whether having an instance of `have` guarantees one of `want`: i.e.
    /// `want` is `have` or a (transitive) superclass of `have`. (An `impl B
    /// for T` requires its superclasses' instances, so a `T : B` bound
    /// licenses calling a superclass `A`'s methods on `T`.)
    pub fn class_satisfies(&self, have: TypeclassRef, want: TypeclassRef) -> bool {
        if have == want {
            return true;
        }
        let mut stack = vec![have];
        let mut seen = Set::new();
        while let Some(c) = stack.pop() {
            if !seen.insert(c) {
                continue;
            }
            for &s in &self.typeclasses[c.0].superclasses {
                if s == want {
                    return true;
                }
                stack.push(s);
            }
        }
        false
    }

    /// The `(class, head, range)` of every registered instance
    /// for the superclass-presence check.
    pub fn instance_keys(&self) -> Vec<(TypeclassRef, TypeHead<'tcx>, Range)> {
        self.instances
            .values()
            .flatten()
            .map(|d| (d.class, d.head, d.range))
            .collect()
    }

    /// Register an instance. Returns `Err(existing range)` if its head
    /// **overlaps** an already-registered instance in the same `(class,
    /// head)` bucket: two heads overlap iff some ground type matches both
    /// (Calculus: Typeclasses, coherence). For ground classes this reduces to
    /// "at most one instance per key"; for a higher-kinded class it also
    /// permits disjoint instances (e.g. fixing a slot to `Int` vs `Bool`)
    /// while rejecting a blanket-vs-specific clash.
    pub fn register_instance(&mut self, def: ImplDef<'tcx>) -> Result<(), Range> {
        let key = (def.class, def.head);
        let bucket = self.instances.entry(key).or_default();
        if let Some(existing) = bucket.iter().find(|e| heads_overlap(e.for_ty, def.for_ty)) {
            return Err(existing.range);
        }
        bucket.push(def);
        Ok(())
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
            self.create_default_module(core_file, "core");
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
            // `Name<T, ...>`: the enum name applied to its type arguments. Region
            // arguments are elided (their internal ids carry no source meaning in
            // a diagnostic; lifetime errors are reported separately).
            TyKind::App(er, args, _) => {
                write!(f, "{}", self.ctx.get_enum(*er).name)?;
                if !args.is_empty() {
                    write!(f, "<")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", self.ctx.display_ty(*arg))?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            // references print as `&T` / `&mut T`; a region ascription shows only
            // its inner type, with internal regions omitted for readability.
            TyKind::Ref(_, inner) => write!(f, "&{}", self.ctx.display_ty(*inner)),
            TyKind::RefMut(_, inner) => write!(f, "&mut {}", self.ctx.display_ty(*inner)),
            TyKind::Region(inner, _) => write!(f, "{}", self.ctx.display_ty(*inner)),
            TyKind::Ptr(inner) => write!(f, "Ptr<{}>", self.ctx.display_ty(*inner)),
            TyKind::Fn(arg, ret, _, _) => {
                write!(
                    f,
                    "{} -> {}",
                    self.ctx.display_ty(*arg),
                    self.ctx.display_ty(*ret)
                )
            }
            // A higher-kinded application `F<..>` or a partial-application hole
            // (Calculus: Partial application): shown structurally with resolved names.
            TyKind::ParamApp(id, args) => {
                write!(f, "{}<", self.ctx.type_param_name(*id))?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", self.ctx.display_ty(*arg))?;
                }
                write!(f, ">")
            }
            TyKind::Hole(_) => write!(f, "_"),
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
/// recording, for every region position whose declared region is one of the
/// `solve` variables, the actual region found there. The accumulated
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
        (TyKind::App(_, ds, dr), TyKind::App(_, as_, ar)) if ds.len() == as_.len() => {
            // pair the region args positionally: a declared `Holder<'a>` parameter
            // binds 'a from the actual argument's region (call-site inference for
            // ADT-typed parameters). Bind before recursing; `bind` holds `out`.
            for (dreg, areg) in dr.iter().zip(ar.iter()) {
                bind(*dreg, *areg);
            }
            for (d, a) in ds.iter().zip(*as_) {
                collect_region_bindings(*d, *a, solve, out);
            }
        }
        _ => {}
    }
}

/// Do two `impl` head abstractions (Calculus: Partial application) **overlap**:
/// is there a ground type matching both? `Hole`s (the class-operated slots) and
/// `Param`s (an instance's own parameters) act as wildcards; fixed slots must
/// match structurally, and a bare `Enum` (all-holes) blankets any application
/// of the same constructor. Used by [`CompileCtx::register_instance`] for
/// coherence.
fn heads_overlap<'a>(a: Ty<'a>, b: Ty<'a>) -> bool {
    match (a.kind(), b.kind()) {
        // wildcards: a hole, an instance parameter, or a (never-expected here)
        // higher-kinded application matches anything.
        (TyKind::Hole(_), _) | (_, TyKind::Hole(_)) => true,
        (TyKind::Param(_), _) | (_, TyKind::Param(_)) => true,
        (TyKind::ParamApp(..), _) | (_, TyKind::ParamApp(..)) => true,
        // a bare `Enum` is all-holes: it blankets any application of the same
        // constructor (and another bare `Enum` of it).
        (TyKind::Enum(e1), TyKind::Enum(e2)) => e1 == e2,
        (TyKind::Enum(e1), TyKind::App(e2, ..)) | (TyKind::App(e1, ..), TyKind::Enum(e2)) => {
            e1 == e2
        }
        (TyKind::App(e1, a1, _), TyKind::App(e2, a2, _)) => {
            e1 == e2
                && a1.len() == a2.len()
                && a1.iter().zip(a2.iter()).all(|(x, y)| heads_overlap(*x, *y))
        }
        (TyKind::Tuple(x), TyKind::Tuple(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| heads_overlap(*p, *q))
        }
        (TyKind::Ref(_, x), TyKind::Ref(_, y)) | (TyKind::RefMut(_, x), TyKind::RefMut(_, y)) => {
            heads_overlap(*x, *y)
        }
        (TyKind::Ptr(x), TyKind::Ptr(y)) => heads_overlap(*x, *y),
        (TyKind::Fn(a1, r1, _, _), TyKind::Fn(a2, r2, _, _)) => {
            heads_overlap(*a1, *a2) && heads_overlap(*r1, *r2)
        }
        // primitives (`Int`/`Bool`/`Unit`/`Top`) and any exact match.
        _ => a.type_eq(b),
    }
}

fn _assert() {
    fn s<T: Sync>() {}
    s::<CompileCtx>();
}
