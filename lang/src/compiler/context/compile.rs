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
use crate::compiler::structure::Set;
use crate::compiler::structure::UniqVar;
use crate::compiler::structure::VarName;
use crate::internal_bug;
use crate::ir_types::hhir::HirVar;
use crate::lang::types::CommonTypes;
use crate::lang::types::EnumRef;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::passes::parse::Rule;

/// This should not be used and is intentionally misspelled to be easily
/// detectable.
const DEFAULT_MODULE_NAME: &str = "mAin";

// ============================= Arena =========================================

/// Backing store for all arena-allocated compiler data.
///
/// Opaque by design — only [`CompileCtx`] internals allocate through it.
/// To swap the allocators, change only this struct and its methods.
///
/// `bump` holds `Copy`, destructor-free type data ([`TyKind`]). The
/// [`typed_arena::Arena`]s hold owning data ([`OriginalFun`], [`CodeModule`],
/// [`EnumDef`], [`OriginalVar`]) whose `String`/`Vec` fields must have their
/// destructors run when the arena is dropped — `bumpalo` would leak them.
struct Arenas {
    bump: bumpalo::Bump,
    functions: typed_arena::Arena<OriginalFun<'static>>,
    modules: typed_arena::Arena<CodeModule>,
    enums: typed_arena::Arena<EnumDef<'static>>,
    variables: typed_arena::Arena<OriginalVar>,
}

/// Safety: after the initial compilation phase, the arena is never mutated
/// again — only existing allocations are read. `bumpalo::Bump` and
/// `typed_arena::Arena` both use `Cell<>` internally, which makes them `!Sync`
/// to prevent concurrent *writes*, but since the LSP and other multi-threaded
/// users only read after compilation, sharing `Arenas` across threads is sound.
///
/// todo: create a new struct RoArena that takes ownership of the inner arenas
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

    // variables
    /// Number of original variables registered so far — assigns each a stable
    /// monotonic `id` used for ordering.
    var_count: usize,
    pub variable_usages: Map<OriginalVarRef<'tcx>, Set<Range>>,
    /// Number of uniquified variables registered so far — assigns each a stable
    /// `idx` distinguishing shadowing re-bindings of the same declaration.
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
            // — the final drop of such values never dereferences the pointer.
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
            !matches!(kind, TyKind::Tuple(_)),
            "use intern_tuple for tuple types"
        );
        if let Some(&ty) = self.ty_interner.get(&kind) {
            return ty;
        }
        let kind_ref = self.arenas.alloc_ty(kind);
        let ty = Ty(kind_ref);
        self.ty_interner.insert(kind, ty);
        ty
    }

    /// Intern a tuple type from its element handles. Arity must be >= 2 —
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
    /// ([`Self::set_variant_payload`]) resolves payload type annotations —
    /// payloads may reference other enums (including forward / recursive
    /// references), so every `EnumRef` must already exist before any
    /// `type_` gets resolved.
    pub fn register_enum(
        &mut self,
        name: &str,
        variant_names: Vec<String>,
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
