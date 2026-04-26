//! variable management

use std::fmt::Display;

use pest::iterators::Pair;

use crate::compiler::structure::Range;
use crate::internal_bug;
use crate::passes::parse::Rule;

/// a globally unique reference to a variable
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UniqVar {
    pub(in crate::compiler) idx: usize,
    pub(in crate::compiler) orig: OriginalVarRef,
}

/// for any IR-specific variable type,
/// this object holds a unique reference
/// to the variable's source in the code
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OriginalVarRef(pub(in crate::compiler) usize);

impl OriginalVarRef {
    pub fn test_new(_idx: usize) -> Self {
        #[cfg(not(feature = "testing"))]
        unreachable!("unsafe reference initialisation outside tests");
        #[cfg(feature = "testing")]
        Self(_idx)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarName(pub(in crate::compiler) String);

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum VarDeclType {
    Declaration,
    Parameter,
    IntrinsicParameter,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OriginalVar {
    pub name: VarName,
    pub declaration: Range,
    pub inst: VarDeclType,
    index: OriginalVarRef,
}

impl VarName {
    pub(in crate::compiler) fn from_pair(pair: &Pair<'_, Rule>) -> Self {
        VarName(pair.as_str().to_string())
    }

    pub(in crate::compiler) fn name(&self) -> String {
        self.0.clone()
    }
}

impl OriginalVar {
    pub(in crate::compiler) fn create(
        pair: &Pair<'_, Rule>,
        index: OriginalVarRef,
        inst: VarDeclType,
    ) -> Self {
        OriginalVar {
            name: VarName::from_pair(pair),
            declaration: Range::from(pair),
            inst,
            index,
        }
    }
}

impl From<Rule> for VarDeclType {
    fn from(value: Rule) -> Self {
        match value {
            Rule::declaration => Self::Declaration,
            Rule::parameter => Self::Parameter,
            _ => internal_bug!("illegal instatiation of VarDeclType"),
        }
    }
}

impl Display for VarName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Vr({})", self.0)
    }
}
