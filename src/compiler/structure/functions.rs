//! function management

use std::fmt::Display;

use pest::iterators::Pair;

use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::types::Ty;
use crate::passes::parse::Rule;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FnName(String);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunRef(pub(in crate::compiler) usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OriginalFun {
    pub name: FnName,
    pub declaration: Range,
    pub module: ModuleRef,
    index: FunRef,
}

impl OriginalFun {
    pub(in crate::compiler) fn create(
        pair: &Pair<'_, Rule>,
        index: FunRef,
        module: ModuleRef,
    ) -> Self {
        OriginalFun {
            name: FnName::from_pair(pair),
            declaration: Range::from(pair),
            module,
            index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunSig {
    pub args: Vec<(UniqVar, Ty)>,
    pub ret_ty: Ty,
}

impl FunSig {
    pub fn with(args: &[crate::ir_types::qhir::Parameter], ret_ty: Ty) -> Self {
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
