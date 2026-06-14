//! Typeclass and instance tables (Calculus §7, §8.8–8.9).
//!
//! A `typeclass` is a named, module-owned item carrying a single type
//! parameter, a set of method signatures (over that parameter), and an optional
//! list of superclasses (`requires`). An `impl C for T` registers a concrete
//! instance, keyed *globally* by `(class, head(T))` — instances form one
//! coherent set and are never `use`d (module redesign principle 6).

use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::TypeParam;
use crate::lang::types::EnumRef;
use crate::lang::types::Ty;
use crate::lang::types::TypeParamId;

/// Index into the context's typeclass table.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct TypeclassRef(pub usize);

/// A `where T : C` constraint: the type parameter `param` must have an instance
/// of typeclass `class`. Checked at call sites (once `param` is concrete) and
/// assumed inside the constrained function's body.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct TypeConstraint {
    pub param: TypeParamId,
    pub class: TypeclassRef,
}

/// The head (type constructor) of an instance's `for` type
/// `Option<Int>` and `Option<Bool>` share the head `Enum(Option)`, so there is
/// one instance per base constructor (per-argument behaviour is mono's job).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum TypeHead<'tcx> {
    Int,
    Bool,
    Unit,
    Enum(EnumRef<'tcx>),
}

/// A typeclass method's signature, expressed over the class's type parameter
/// (so `eq : (T, T) -> Bool` stores `param_tys = [Param(T), Param(T)]`,
/// `ret_ty = Bool`). Resolution unifies these against actual argument types to
/// solve the class parameter (the receiver type).
#[derive(Clone, Debug)]
pub struct MethodDef<'tcx> {
    pub name: String,
    /// the method's own generics (usually empty); the class parameter is
    /// separate.
    pub type_params: Vec<TypeParam>,
    pub param_tys: Vec<Ty<'tcx>>,
    pub ret_ty: Ty<'tcx>,
    pub has_default: bool,
    /// The default body, built once as a generic function `<T> where T : C`
    /// (set after sig resolution). An impl that omits this method points its
    /// instance entry here.
    pub default_fn: Option<FunRef<'tcx>>,
    pub range: Range,
}

/// A registered typeclass.
#[derive(Clone, Debug)]
pub struct TypeclassDef<'tcx> {
    pub name: String,
    /// the class's single type variable (the `T` in `typeclass Eq<T>`).
    pub param: TypeParamId,
    pub superclasses: Vec<TypeclassRef>,
    pub methods: Map<String, MethodDef<'tcx>>,
    /// declaration order, for stable iteration / completeness diagnostics.
    pub method_order: Vec<String>,
    pub src_module: ModuleRef<'tcx>,
    pub range: Range,
}

/// A registered `impl C for T` instance.
#[derive(Clone, Debug)]
pub struct ImplDef<'tcx> {
    pub class: TypeclassRef,
    pub for_ty: Ty<'tcx>,
    pub head: TypeHead<'tcx>,
    /// method name -> the concrete function implementing it for this instance.
    pub methods: Map<String, FunRef<'tcx>>,
    pub src_module: ModuleRef<'tcx>,
    pub range: Range,
}
