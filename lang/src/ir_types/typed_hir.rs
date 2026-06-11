//! a strongly typed abstract syntax tree IR,
//! - expressions are annotated with their types
//! - variables and functions are resolved (VarRef and FnRef instead of String)
//! - uniquify has already been run, so no name clashes

use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
pub use crate::ir_types::qhir::Parameter;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub functions: Map<FunRef, TypedFunction>,
}

#[derive(Debug, Clone)]
pub struct TypedFunction {
    pub name: FunRef,
    pub range: Range,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
    pub src_module: ModuleRef,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Declaration {
        name: UniqVar,
        range: Range,
        ty: Ty,
        val: Expr,
    },

    /// Flat tuple-pattern binding after type-checking.
    ///
    /// Each element is `(name, ty, is_mutable, range)`.
    /// MIR lowering desugars this to a temporary + per-element
    /// `RValue::Field` extractions (no change to LLVM codegen needed).
    LetTuple {
        elems: Vec<(UniqVar, Ty, bool, Range)>,
        range: Range,
        val: Expr,
    },

    /// Constructor-pattern binding with mandatory else fallback.
    ///
    /// `let E#V(payload) = expr else fallback`
    ///
    /// The `pattern` is always a `MatchPattern::Variant` (its sub-pattern is
    /// irrefutable — only bindings/wildcards/tuples inside). `else_branch` has
    /// the same type as `val`; its outermost expression is guaranteed by the
    /// type checker to be `Constructor { variant_idx == pattern.variant_idx }`.
    ///
    /// MIR lowering desugars to a discriminant branch: if the discriminant
    /// matches, extract from `val`; otherwise evaluate `else_branch` and
    /// extract from that (guaranteed to succeed).
    LetPattern {
        pattern: MatchPattern,
        val: Expr,
        else_branch: Expr,
        range: Range,
    },

    Assignment {
        name: UniqVar,
        range: Range,
        val: Expr,
    },

    Expr(Expr),
}

/// `Expr` wraps an `Expression` and
/// carries its expected type &
/// start/end positions (line,col)
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
        payload: Option<Box<Expr>>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<TypedMatchArm>,
    },
    Tuple(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypedMatchArm {
    pub pattern: MatchPattern,
    pub body: Expr,
    pub range: Range,
}

/// a pattern in a typed match expression.
/// all constructor and tag patterns have been resolved to (EnumRef,
/// variant_idx). wildcards are left as-is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MatchPattern {
    Variant {
        /// the enum type — the type of the value matched against this pattern.
        /// Carried for the same reason `Tuple` carries its `ty` and `Binding`
        /// carries its `ty`: MIR lowering operates without `CompileCtx` access
        /// and needs the type to correctly allocate extraction temporaries when
        /// this pattern appears in a *nested* (sub-pattern) position.
        ty: Ty,
        enum_ref: EnumRef,
        variant_idx: usize,
        /// `Some((payload_ty, sub_pattern))` when the pattern destructures
        /// the variant's payload. `payload_ty` is the variant's *declared*
        /// payload type — carried here for the same reason as `ty`.
        payload: Option<(Ty, Box<MatchPattern>)>,
    },
    /// tuple destructuring `(p1, p2, ...)`. `ty` is the tuple's own type —
    /// needed by MIR lowering for the same reason `Variant.payload` carries
    /// its type (typing extraction temporaries for nested destructuring,
    /// e.g. the inner tuple in `Wrap((x, y))`).
    Tuple {
        ty: Ty,
        elems: Vec<MatchPattern>,
    },
    /// integer literal in pattern position: `42` or `-7`.
    IntLit(i64),
    /// boolean literal in pattern position: `true` or `false`.
    BoolLit(bool),
    /// a variable binding that destructures part of the scrutinee
    Binding {
        var: UniqVar,
        ty: Ty,
        range: Range,
    },
    Wildcard,
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

            (
                Assignment {
                    name: n1, val: v1, ..
                },
                Assignment {
                    name: n2, val: v2, ..
                },
            ) => n1 == n2 && v1 == v2,

            (
                LetTuple {
                    elems: e1, val: v1, ..
                },
                LetTuple {
                    elems: e2, val: v2, ..
                },
            ) => e1 == e2 && v1 == v2,

            (
                LetPattern {
                    pattern: p1,
                    val: v1,
                    else_branch: eb1,
                    ..
                },
                LetPattern {
                    pattern: p2,
                    val: v2,
                    else_branch: eb2,
                    ..
                },
            ) => p1 == p2 && v1 == v2 && eb1 == eb2,

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
            LetTuple { elems, val, .. } => {
                elems.hash(state);
                val.hash(state);
            }
            LetPattern {
                pattern,
                val,
                else_branch,
                ..
            } => {
                pattern.hash(state);
                val.hash(state);
                else_branch.hash(state);
            }
            Assignment { name, val, .. } => {
                name.hash(state);
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
