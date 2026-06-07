//! highest high intermediate representation:
//! the abstract syntax tree IR

use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::FunRef;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::OriginalVarRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug)]
pub struct ProgramModule {
    pub functions: Vec<Function>,
    pub module_name: ModuleRef,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum HirVar {
    Decl(OriginalVarRef),
    Unqualified(String),
    Uniq(UniqVar),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HirFnCall {
    Local(String),
    External { module: String, name: String },
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: HirVar,
    pub ty: Ty,
    pub range: Range,
    pub is_mutable: bool,
}

#[derive(Debug)]
pub struct Function {
    pub name: FunRef,
    pub range: Range,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Declaration {
        name: HirVar,
        range: Range,
        ty: Option<Ty>,
        is_mutable: bool,
        val: Expr,
    },

    Assignment {
        name: HirVar,
        range: Range,
        val: Expr,
    },

    Expr(Expr),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col)
#[derive(Debug, Clone)]
pub struct Expr {
    pub expr: Expression,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
        fn_name: HirFnCall,
        args: Vec<Expr>,
    },
    Var(HirVar),
    Int(i64),
    Bool(bool),
    Unit,
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expr>>,
    },
    Constructor {
        type_name: String,
        variant: String,
    },
    ExternalConstructor {
        mod_name: String,
        type_name: String,
        variant: String,
    },
    Tag {
        variant: String,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<HirMatchArm>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub body: Expr,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirPattern {
    Constructor { type_name: String, variant: String },
    Tag { variant: String },
    Wildcard,
}

impl HirVar {
    pub fn is_uniq(&self) -> bool {
        matches!(self, HirVar::Uniq(_))
    }
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
