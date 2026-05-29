//! qualified functions high intermediate representation:
//!
//! all modules are combined,
//! all function calls have been confirmed
//! to be calling existing functions or intrinsics,
//! functions and variables all have unique identifiers

use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct Program {
    pub functions: Map<FunRef, Function>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Parameter {
    pub name: UniqVar,
    pub ty: Ty,
    pub range: Range,
    pub is_mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Function {
    pub name: FunRef,
    pub range: Range,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
    pub src_module: ModuleRef,
}

#[derive(Debug, Clone, PartialOrd, Ord)]
pub enum Statement {
    Declaration {
        name: UniqVar,
        range: Range,
        ty: Option<Ty>,
        is_mutable: bool,
        val: Expr,
    },

    Assignment {
        name: UniqVar,
        range: Range,
        val: Expr,
    },

    Expr(Expr),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col)
#[derive(Debug, Clone, PartialOrd, Ord)]
pub struct Expr {
    pub expr: Expression,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Expression {
    If {
        cond: Box<Expr>,
        t: Box<Expr>,
        f: Option<Box<Expr>>,
    },
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
    },
    BinOp {
        left: Box<Expr>,
        op: Bop,
        right: Box<Expr>,
    },
    UnOp {
        op: Uop,
        right: Box<Expr>,
    },
    Call {
        fn_name: FunRef,
        args: Vec<Expr>,
    },
    IntrinsicCall {
        fn_name: Intrinsic,
        args: Vec<Expr>,
    },
    Var(UniqVar),
    Int(i64),
    Bool(bool),
    Unit,
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expr>>,
    },
    Constructor {
        enum_ref: EnumRef,
        variant_idx: usize,
    },
    Tag {
        variant: String,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<QMatchArm>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QMatchArm {
    pub pattern: QPattern,
    pub body: Expr,
    pub range: Range,
}

/// a pattern after the qualify pass.
/// constructor patterns are fully resolved to (EnumRef, variant_idx),
/// tag patterns keep their string name for resolution by the type checker,
/// wildcards are left as-is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QPattern {
    /// fully resolved enum variant: `T::A` -> (EnumRef, 0)
    Variant {
        enum_ref: EnumRef,
        variant_idx: usize,
    },
    /// Bare tag `#tag`, resolved by the type checker
    Tag { variant: String },
    /// Wildcard `_`
    Wildcard,
}

impl Eq for Statement {}

impl Hash for Statement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use Statement::*;
        std::mem::discriminant(self).hash(state);
        match self {
            Declaration {
                name: var, ty, val, ..
            } => {
                var.hash(state);
                ty.hash(state);
                val.hash(state);
            }
            Assignment { name, val, .. } => {
                name.hash(state);
                val.hash(state);
            }
            Expr(e) => e.hash(state),
        }
    }
}

impl PartialEq for Statement {
    fn eq(&self, other: &Self) -> bool {
        use Statement::*;
        match (self, other) {
            (
                Declaration {
                    name: n1,
                    ty: t1,
                    val: v1,
                    ..
                },
                Declaration {
                    name: n2,
                    ty: t2,
                    val: v2,
                    ..
                },
            ) => n1 == n2 && t1 == t2 && v1 == v2,

            (
                Assignment {
                    name: n1, val: v1, ..
                },
                Assignment {
                    name: n2, val: v2, ..
                },
            ) => n1 == n2 && v1 == v2,

            (Expr(e1), Expr(e2)) => e1 == e2,
            _ => false,
        }
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl Hash for Expr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

impl Eq for Expr {}
