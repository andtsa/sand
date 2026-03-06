//! types for structuring projects

use std::collections::BTreeMap;
use std::fmt::Display;
use std::path::PathBuf;

use pest::RuleType;
use pest::Span;
use pest::iterators::Pair;
use tower_lsp::lsp_types::Url;

use crate::ir_types::hhir::Parameter;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::types::Ty;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleRef {
    name: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Range {
    pub start: Pos,
    pub end: Pos,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileRef {
    path: PathBuf,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FnName(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FnSig {
    pub args: Vec<(VarName, Ty)>,
    pub ret_ty: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarName(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Project {
    pub config: ProjectConfig,
    pub files: Vec<FileRef>,
    pub modules: Vec<ModuleRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectConfig {
    //todo
}

pub type Map<K, V> = BTreeMap<K, V>;

impl From<&crate::ir_types::hhir::Function> for FnName {
    fn from(value: &crate::ir_types::hhir::Function) -> Self {
        FnName(value.name.clone())
    }
}

impl From<&crate::ir_types::hhir::Parameter> for VarName {
    fn from(value: &crate::ir_types::hhir::Parameter) -> Self {
        VarName(value.name.clone())
    }
}

impl From<Intrinsic> for FnName {
    fn from(value: Intrinsic) -> Self {
        FnName(value.to_string())
    }
}

impl TryFrom<&crate::ir_types::hhir::Statement> for VarName {
    type Error = ();
    fn try_from(value: &crate::ir_types::hhir::Statement) -> Result<Self, Self::Error> {
        match value {
            crate::ir_types::hhir::Statement::Declaration { name, .. } => Ok(VarName(name.clone())),
            _ => Err(()),
        }
    }
}

impl Display for FnName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fn({})", self.0)
    }
}

impl Display for VarName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Vr({})", self.0)
    }
}

impl FnSig {
    pub fn with(args: &[Parameter], ret_ty: Ty) -> Self {
        Self {
            args: args.iter().map(|a| (VarName::from(a), a.ty)).collect(),
            ret_ty,
        }
    }
}

impl From<(usize, usize)> for Pos {
    fn from(value: (usize, usize)) -> Self {
        Pos {
            line: value.0,
            col: value.1,
        }
    }
}

impl Pos {
    pub fn line_col(&self) -> (usize, usize) {
        (self.line, self.col)
    }

    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl Range {
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start: Pos::new(start_line, start_col),
            end: Pos::new(end_line, end_col),
        }
    }

    pub fn destruct(&self) -> ((usize, usize), (usize, usize)) {
        (self.start.line_col(), self.end.line_col())
    }
}

impl From<(Pos, Pos)> for Range {
    fn from(value: (Pos, Pos)) -> Self {
        Range {
            start: value.0,
            end: value.1,
        }
    }
}

impl From<Range> for (Pos, Pos) {
    fn from(value: Range) -> Self {
        (value.start, value.end)
    }
}

impl From<&Span<'_>> for Range {
    fn from(value: &Span) -> Self {
        let start = value.start_pos();
        let end = value.end_pos();
        Range {
            start: Pos::from(start.line_col()),
            end: Pos::from(end.line_col()),
        }
    }
}

impl From<Span<'_>> for Range {
    fn from(value: Span) -> Self {
        (&value).into()
    }
}

impl<T: RuleType> From<&Pair<'_, T>> for Range {
    fn from(value: &Pair<T>) -> Self {
        value.as_span().into()
    }
}

impl<T: RuleType> From<Pair<'_, T>> for Range {
    fn from(value: Pair<T>) -> Self {
        (&value).into()
    }
}

impl Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Range({}:{} – {}:{})",
            self.start.line, self.start.col, self.end.line, self.end.col
        )
    }
}

impl Display for Pos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

impl From<Span<'_>> for ModuleRef {
    fn from(value: Span<'_>) -> Self {
        ModuleRef {
            name: value.as_str().to_string(),
        }
    }
}

impl From<&FileRef> for ModuleRef {
    fn from(value: &FileRef) -> Self {
        ModuleRef {
            name: value.name.clone(),
        }
    }
}

impl From<Url> for ModuleRef {
    fn from(value: Url) -> Self {
        let name = value
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("unknown")
            .to_string();
        ModuleRef {
            name, //: value.path().to_string()
        }
    }
}

impl ModuleRef {
    pub fn main() -> Self {
        ModuleRef {
            name: "main".to_string(),
        }
    }
}
