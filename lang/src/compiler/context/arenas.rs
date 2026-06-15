//! Arena allocators for symbols during the compilation context

use crate::compiler::structure::AdtDef;
use crate::compiler::structure::CodeModule;
use crate::compiler::structure::OriginalFun;
use crate::compiler::structure::OriginalVar;
use crate::lang::types::Region;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;

/// Backing store for all arena-allocated compiler data.
///
/// this is opaque by design, since only [`CompileCtx`] internals allocate
/// through it. to swap the allocators we only need to change this struct and
/// its methods.
///
/// `bump` holds `Copy`, destructor-free type data ([`TyKind`]). The
/// [`typed_arena::Arena`]s hold owning data ([`OriginalFun`], [`CodeModule`],
/// [`AdtDef`], [`OriginalVar`]) whose `String`/`Vec` fields must have their
/// destructors run when the arena is dropped, since `bumpalo` would leak them.
pub struct Arenas {
    bump: bumpalo::Bump,
    functions: typed_arena::Arena<OriginalFun<'static>>,
    modules: typed_arena::Arena<CodeModule>,
    adts: typed_arena::Arena<AdtDef<'static>>,
    variables: typed_arena::Arena<OriginalVar>,
}

/// Safety: after the initial compilation phase, the arena is never mutated
/// again; only existing allocations are read. `bumpalo::Bump` and
/// `typed_arena::Arena` both use `Cell<>` internally, which makes them `!Sync`
/// to prevent concurrent *writes*, but since the LSP and other multi-threaded
/// users only read after compilation, sharing `Arenas` across threads is sound.
///
/// TODO: create a new struct `RoArena` that takes ownership of the inner arenas
/// after compilation has finished, and implement send + sync on that
unsafe impl Send for Arenas {}
unsafe impl Sync for Arenas {}

impl Arenas {
    pub fn new() -> Self {
        Self {
            bump: bumpalo::Bump::new(),
            functions: typed_arena::Arena::new(),
            modules: typed_arena::Arena::new(),
            adts: typed_arena::Arena::new(),
            variables: typed_arena::Arena::new(),
        }
    }

    pub fn alloc_ty<'tcx>(&'tcx self, kind: TyKind<'tcx>) -> &'tcx TyKind<'tcx> {
        self.bump.alloc(kind)
    }

    pub fn alloc_ty_slice<'tcx>(&'tcx self, tys: &[Ty<'tcx>]) -> &'tcx [Ty<'tcx>] {
        self.bump.alloc_slice_copy(tys)
    }

    pub fn alloc_region_slice(&self, regions: &[Region]) -> &[Region] {
        self.bump.alloc_slice_copy(regions)
    }

    // The `typed_arena` allocators are invariant in their element lifetime, so
    // we store them as `'static` and transmute the borrow to `'tcx` on the way
    // out. This is sound: the returned reference cannot outlive `&'tcx self`,
    // and every `'tcx` value stored inside (e.g. `ModuleRef<'tcx>`) is itself
    // an arena reference with the same provenance.

    pub fn alloc_function<'tcx>(&'tcx self, f: OriginalFun<'tcx>) -> &'tcx OriginalFun<'tcx> {
        let f: OriginalFun<'static> = unsafe { std::mem::transmute(f) };
        let r: &'tcx OriginalFun<'static> = self.functions.alloc(f);
        unsafe { std::mem::transmute(r) }
    }

    pub fn alloc_module(&self, m: CodeModule) -> &CodeModule {
        self.modules.alloc(m)
    }

    pub fn alloc_enum<'tcx>(&'tcx self, e: AdtDef<'tcx>) -> &'tcx AdtDef<'tcx> {
        let e: AdtDef<'static> = unsafe { std::mem::transmute(e) };
        let r: &'tcx AdtDef<'static> = self.adts.alloc(e);
        unsafe { std::mem::transmute(r) }
    }

    pub fn alloc_variable(&self, v: OriginalVar) -> &OriginalVar {
        self.variables.alloc(v)
    }
}
