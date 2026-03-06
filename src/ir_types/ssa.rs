//! a strongly typed abstract syntax tree IR,
//! - expressions are annotated with their types
//! - variables and functions are resolved (VarRef and FnRef instead of String)
//! - uniquify has already been run, so no name clashes
//! - is SSA form (each variable is assigned to exactly once)

use std::hash::Hash;
use std::hash::Hasher;

use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::structure::FnName;
use crate::lang::structure::Map;
use crate::lang::structure::Range;
use crate::lang::structure::VarName;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct SsaProgram {
    pub functions: Map<FnName, SsaFunction>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: VarName,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub struct SsaFunction {
    pub name: FnName,
    pub range: Range,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Declaration {
        name: VariableRef,
        range: Range,
        ty: Ty,
        val: Expr,
    },

    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VariableRef {
    Single(VarName),
    PhiNode(Vec<VarName>),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col)
#[derive(Debug, Clone)]
pub struct Expr {
    pub expr: Expression,
    pub ty: Ty,
    pub range: Range,
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
        fn_name: FnName,
        args: Vec<Expr>,
    },
    IntrinsicCall {
        fn_name: Intrinsic,
        args: Vec<Expr>,
    },
    /// resolved variable reference
    RVar(VarName),
    Int(i64),
    Bool(bool),
    Unit,
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expr>>,
    },
}

// --- trait implementations ---

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

            (Expr(e1), Expr(e2)) => e1 == e2,
            _ => false,
        }
    }
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
            Expr(e) => e.hash(state),
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
