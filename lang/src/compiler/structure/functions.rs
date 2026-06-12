//! Function management.

use std::cmp::Ordering;
use std::fmt::Display;
use std::hash::Hash;
use std::hash::Hasher;

use pest::iterators::Pair;

use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::types::Ty;
use crate::passes::parse::Rule;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FnName(String);

/// A `Copy` handle to an arena-allocated [`OriginalFun`].
///
/// Equality and hashing are by pointer identity (each `register_function`
/// allocates a distinct `OriginalFun`); ordering is by the monotonic
/// registration `id`, preserving source/registration order for deterministic
/// iteration over `BTreeMap<FunRef, _>`.
#[derive(Copy, Clone)]
pub struct FunRef<'tcx>(pub(in crate::compiler) &'tcx OriginalFun<'tcx>);

impl PartialEq for FunRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}
impl Eq for FunRef<'_> {}
impl Hash for FunRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as *const OriginalFun<'_>).hash(state);
    }
}
impl PartialOrd for FunRef<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FunRef<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.id.cmp(&other.0.id)
    }
}
impl std::fmt::Debug for FunRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FunRef({}, {})", self.0.id, self.0.name.0)
    }
}

#[derive(Debug, Clone)]
pub struct OriginalFun<'tcx> {
    pub name: FnName,
    pub declaration: Range,
    pub module: ModuleRef<'tcx>,
    pub(in crate::compiler) id: usize,
}

impl<'tcx> OriginalFun<'tcx> {
    pub(in crate::compiler) fn create(
        pair: &Pair<'_, Rule>,
        id: usize,
        module: ModuleRef<'tcx>,
    ) -> Self {
        OriginalFun {
            name: FnName::from_pair(pair),
            declaration: Range::from(pair),
            module,
            id,
        }
    }

    pub(in crate::compiler) fn synthetic(
        name: String,
        declaration: Range,
        module: ModuleRef<'tcx>,
        id: usize,
    ) -> Self {
        OriginalFun {
            name: FnName::synthetic(name),
            declaration,
            module,
            id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunSig<'tcx> {
    pub args: Vec<(UniqVar<'tcx>, Ty<'tcx>)>,
    pub ret_ty: Ty<'tcx>,
}

impl<'tcx> FunSig<'tcx> {
    pub fn with(args: &[crate::ir_types::qhir::Parameter<'tcx>], ret_ty: Ty<'tcx>) -> Self {
        Self {
            args: args.iter().map(|a| (a.name, a.ty)).collect(),
            ret_ty,
        }
    }
}

impl FnName {
    pub(in crate::compiler) fn from_pair(pair: &Pair<'_, Rule>) -> Self {
        FnName(pair.as_str().to_string())
    }

    /// Construct a name directly — used for compiler-synthesised functions such
    /// as monomorphised specialisations (`id$Int`).
    pub(in crate::compiler) fn synthetic(name: String) -> Self {
        FnName(name)
    }

    pub(in crate::compiler) fn name(&self) -> String {
        self.0.clone()
    }
}

impl From<Intrinsic> for FnName {
    fn from(value: Intrinsic) -> Self {
        FnName(value.to_string())
    }
}

impl Display for FnName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fn({})", self.0)
    }
}
