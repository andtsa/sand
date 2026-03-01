//! highest high intermediate representation:
//! the abstract syntax tree IR

use std::hash::Hash;
use std::hash::Hasher;

use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct Program(pub Vec<Function>);

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub ty: Ty,
    pub start: (usize, usize),
    pub end: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub name_start: (usize, usize),
    pub name_end: (usize, usize),
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Declaration {
        name: String,
        name_start: (usize, usize),
        name_end: (usize, usize),
        ty: Ty,
        val: Expr,
    },

    Assignment {
        name: String,
        name_start: (usize, usize),
        name_end: (usize, usize),
        val: Expr,
    },

    Expr(Expr),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col)
#[derive(Debug, Clone)]
pub struct Expr {
    pub expr: Expression,
    pub start: (usize, usize),
    pub end: (usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expression {
    If {
        cond: Box<Expr>,
        t: Box<Expr>,
        f: Box<Expr>,
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
        fn_name: String,
        args: Vec<Expr>,
    },
    Var(String),
    Int(i64),
    Bool(bool),
    Unit,
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expr>>,
    },
}

impl Eq for Statement {}

impl Hash for Statement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use Statement::*;
        std::mem::discriminant(self).hash(state);
        match self {
            Declaration { name, ty, val, .. } => {
                name.hash(state);
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
