//! variable management

use std::cmp::Ordering;
use std::fmt::Display;
use std::hash::Hash;
use std::hash::Hasher;

use pest::iterators::Pair;

use crate::compiler::structure::Range;
use crate::internal_bug;
use crate::passes::parse::Rule;

/// a globally unique reference to a variable
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UniqVar<'tcx> {
    pub(in crate::compiler) idx: usize,
    pub(in crate::compiler) orig: OriginalVarRef<'tcx>,
}

/// A `Copy` handle to an arena-allocated [`OriginalVar`]
///
/// Equality/hashing by pointer identity,
/// ordering by the monotonic registration `id`.
#[derive(Copy, Clone)]
pub struct OriginalVarRef<'tcx>(pub(in crate::compiler) &'tcx OriginalVar);

impl PartialEq for OriginalVarRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}
impl Eq for OriginalVarRef<'_> {}
impl Hash for OriginalVarRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as *const OriginalVar).hash(state);
    }
}
impl PartialOrd for OriginalVarRef<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OriginalVarRef<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.id.cmp(&other.0.id)
    }
}
impl std::fmt::Debug for OriginalVarRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OriginalVarRef({}, {})", self.0.id, self.0.name.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarName(pub(in crate::compiler) String);

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum VarDeclType {
    Declaration,
    Parameter,
    IntrinsicParameter,
    /// a variable bound by a destructuring pattern in a `match` arm,
    /// e.g. the `r` in `Circle(r) => ...` or `a`/`b` in `(a, b) => ...`
    PatternBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OriginalVar {
    pub name: VarName,
    pub declaration: Range,
    pub inst: VarDeclType,
    pub(in crate::compiler) id: usize,
}

impl VarName {
    pub(in crate::compiler) fn from_pair(pair: &Pair<'_, Rule>) -> Self {
        VarName(pair.as_str().to_string())
    }

    pub(in crate::compiler) fn name(&self) -> String {
        self.0.clone()
    }

    /// A name for a compiler-synthesised variable (no backing source `Pair`).
    pub(in crate::compiler) fn synthetic(name: &str) -> Self {
        VarName(name.to_string())
    }
}

impl OriginalVar {
    pub(in crate::compiler) fn create(pair: &Pair<'_, Rule>, id: usize, inst: VarDeclType) -> Self {
        OriginalVar {
            name: VarName::from_pair(pair),
            declaration: Range::from(pair),
            inst,
            id,
        }
    }
}

impl From<Rule> for VarDeclType {
    fn from(value: Rule) -> Self {
        match value {
            Rule::declaration => Self::Declaration,
            Rule::parameter => Self::Parameter,
            Rule::binding_pattern => Self::PatternBinding,
            _ => internal_bug!("illegal instatiation of VarDeclType"),
        }
    }
}

impl Display for VarName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Vr({})", self.0)
    }
}
