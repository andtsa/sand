//! Type-parameter declarations carried through the IRs.

use crate::compiler::structure::Range;
use crate::lang::types::Kind;
use crate::lang::types::RegionVar;
use crate::lang::types::TypeParamId;
use crate::lang::types::Variance;

/// A single declared type parameter (the `T` in `def f<T>(...)`).
///
/// `id` is assigned during AST building and is globally unique; `name` and
/// `range` are retained for diagnostics. Uses of the parameter inside a type
/// resolve to [`TyKind::Param`](crate::lang::types::TyKind::Param)`(id)`.
/// `variance` and `kind` carry the declared (or defaulted) annotations
/// (Calculus §2.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeParam {
    pub id: TypeParamId,
    pub name: String,
    pub range: Range,
    pub variance: Variance,
    pub kind: Kind,
}

/// A parsed type-parameter declaration before an id is assigned — variance and
/// kind already defaulted from the (possibly absent) annotations.
pub struct TypeParamSpec {
    pub name: String,
    pub range: Range,
    pub variance: Variance,
    pub kind: Kind,
}

/// A single declared region (lifetime) parameter (the `'r` in `def f<'r>(...)`
/// or `type Ref<'r, a>`; Calculus §8.4).
///
/// `region` is the [`RegionVar`] allocated during AST building, scoped to this
/// declaration; `name` and `range` are retained for diagnostics. Uses of the
/// parameter inside a type (`&'r T`, `T @ 'r`) resolve to this `region`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegionParam {
    pub name: String,
    pub region: RegionVar,
    pub range: Range,
}

/// A parsed region-parameter declaration before a [`RegionVar`] is assigned.
pub struct RegionParamSpec {
    pub name: String,
    pub range: Range,
}
