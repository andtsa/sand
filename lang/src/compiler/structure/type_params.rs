//! Type-parameter declarations carried through the IRs.

use crate::compiler::structure::Range;
use crate::lang::types::TypeParamId;

/// A single declared type parameter (the `T` in `def f<T>(...)`).
///
/// `id` is assigned during AST building and is globally unique; `name` and
/// `range` are retained for diagnostics. Uses of the parameter inside a type
/// resolve to [`TyKind::Param`](crate::lang::types::TyKind::Param)`(id)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeParam {
    pub id: TypeParamId,
    pub name: String,
    pub range: Range,
}
