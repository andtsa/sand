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
use crate::compiler::structure::RegionParam;
use crate::compiler::structure::TypeParam;
use crate::compiler::structure::UniqVar;
pub use crate::ir_types::qhir::Parameter;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct TypedProgram<'tcx> {
    pub functions: Map<FunRef<'tcx>, TypedFunction<'tcx>>,
}

#[derive(Debug, Clone)]
pub struct TypedFunction<'tcx> {
    pub name: FunRef<'tcx>,
    pub range: Range,
    pub type_params: Vec<TypeParam>,
    pub region_params: Vec<RegionParam>,
    pub where_constraints: Vec<RegionConstraint>,
    pub parameters: Vec<Parameter<'tcx>>,
    pub ret_type: Ty<'tcx>,
    pub body: Expr<'tcx>,
    pub src_module: ModuleRef<'tcx>,
}

#[derive(Debug, Clone)]
pub enum Statement<'tcx> {
    Declaration {
        name: UniqVar<'tcx>,
        range: Range,
        ty: Ty<'tcx>,
        val: Expr<'tcx>,
    },

    /// Flat tuple-pattern binding after type-checking.
    ///
    /// Each element is `(name, ty, is_mutable, range)`.
    /// MIR lowering desugars this to a temporary + per-element
    /// `RValue::Field` extractions (no change to LLVM codegen needed).
    LetTuple {
        elems: Vec<(UniqVar<'tcx>, Ty<'tcx>, bool, Range)>,
        range: Range,
        val: Expr<'tcx>,
    },

    /// Constructor-pattern binding with mandatory else fallback.
    ///
    /// `let E#V(payload) = expr else fallback`
    ///
    /// The `pattern` is always a `MatchPattern::Variant` (its sub-pattern is
    /// irrefutable). `else_branch` has the same type as `val`; its outermost
    /// expression is guaranteed by the type checker to be
    /// `Constructor { variant_idx == pattern.variant_idx }`.
    ///
    /// MIR lowering desugars to a discriminant branch: if the discriminant
    /// matches, extract from `val`; otherwise evaluate `else_branch` and
    /// extract from that (guaranteed to succeed).
    LetPattern {
        pattern: MatchPattern<'tcx>,
        val: Expr<'tcx>,
        else_branch: Expr<'tcx>,
        range: Range,
    },

    Assignment {
        name: UniqVar<'tcx>,
        range: Range,
        val: Expr<'tcx>,
    },

    /// Write-through `*reference = value` (Calculus §3.2). `reference : &mut
    /// T`, `value : T`.
    DerefAssign {
        reference: Expr<'tcx>,
        value: Expr<'tcx>,
        range: Range,
    },

    Expr(Expr<'tcx>),
}

/// `Expr` wraps an `Expression` and carries its type, kind, and source
/// position. `kind` is `Owned` for ordinary expressions and `Never` for
/// diverging ones (e.g. an infinite loop); only `expr` participates in
/// equality/hashing (see the impls below), so `ty`/`kind` are free to differ.
#[derive(Debug, Clone)]
pub struct Expr<'tcx> {
    pub expr: Expression<'tcx>,
    pub ty: Ty<'tcx>,
    pub kind: Kind,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expression<'tcx> {
    If {
        cond: Box<Expr<'tcx>>,
        t: Box<Expr<'tcx>>,
        f: Box<Expr<'tcx>>,
    },
    While {
        cond: Box<Expr<'tcx>>,
        body: Box<Expr<'tcx>>,
    },
    BinOp {
        left: Box<Expr<'tcx>>,
        op: Bop,
        right: Box<Expr<'tcx>>,
    },
    UnOp {
        op: Uop,
        right: Box<Expr<'tcx>>,
    },
    Call {
        fn_name: FunRef<'tcx>,
        args: Vec<Expr<'tcx>>,
    },
    IntrinsicCall {
        fn_name: Intrinsic,
        args: Vec<Expr<'tcx>>,
    },
    Var(UniqVar<'tcx>),
    Int(i64),
    Bool(bool),
    Unit,
    /// borrow `&e` (shared) or `&mut e` (exclusive, the `bool` is `true`)
    /// (Calculus §3.2).
    Borrow(Box<Expr<'tcx>>, bool),
    /// dereference `*e`: read through a reference (`&T`/`&mut T` -> T).
    /// Transparent at runtime (borrows are erased), so it lowers like `Borrow`.
    Deref(Box<Expr<'tcx>>),
    Block {
        statements: Vec<Statement<'tcx>>,
        expr: Option<Box<Expr<'tcx>>>,
    },
    Constructor {
        enum_ref: EnumRef<'tcx>,
        variant_idx: usize,
        payload: Option<Box<Expr<'tcx>>>,
    },
    Match {
        scrutinee: Box<Expr<'tcx>>,
        arms: Vec<TypedMatchArm<'tcx>>,
    },
    Tuple(Vec<Expr<'tcx>>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypedMatchArm<'tcx> {
    pub pattern: MatchPattern<'tcx>,
    pub body: Expr<'tcx>,
    pub range: Range,
}

/// a pattern in a typed match expression.
/// all constructor and tag patterns have been resolved to (EnumRef,
/// variant_idx). wildcards are left as-is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MatchPattern<'tcx> {
    Variant {
        /// the enum type
        /// Carried for the same reason `Tuple` carries its `ty` and `Binding`
        /// carries its `ty`: MIR lowering operates without `CompileCtx` access
        /// and needs the type to correctly allocate extraction temporaries when
        /// this pattern appears in a *nested* (sub-pattern) position.
        ty: Ty<'tcx>,
        enum_ref: EnumRef<'tcx>,
        variant_idx: usize,
        /// `Some((payload_ty, sub_pattern))` when the pattern destructures
        /// the variant's payload. `payload_ty` is the variant's *declared*
        /// payload type
        payload: Option<(Ty<'tcx>, Box<MatchPattern<'tcx>>)>,
    },
    /// tuple destructuring `(p1, p2, ...)`. `ty` is the tuple's own type,
    /// needed by MIR lowering for the same reason `Variant.payload` carries
    /// its type (typing extraction temporaries for nested destructuring,
    /// e.g. the inner tuple in `Wrap((x, y))`).
    Tuple {
        ty: Ty<'tcx>,
        elems: Vec<MatchPattern<'tcx>>,
    },
    /// integer literal in pattern position: `42` or `-7`.
    IntLit(i64),
    /// boolean literal in pattern position: `true` or `false`.
    BoolLit(bool),
    /// a variable binding that destructures part of the scrutinee
    Binding {
        var: UniqVar<'tcx>,
        ty: Ty<'tcx>,
        range: Range,
    },
    Wildcard,
}

// --- trait implementations ---

impl<'tcx> PartialEq for Statement<'tcx> {
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

impl<'tcx> Eq for Statement<'tcx> {}

impl<'tcx> Hash for Statement<'tcx> {
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
            DerefAssign {
                reference, value, ..
            } => {
                reference.hash(state);
                value.hash(state);
            }
            Expr(e) => e.hash(state),
        }
    }
}

impl<'tcx> PartialEq for Expr<'tcx> {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl<'tcx> Hash for Expr<'tcx> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

impl<'tcx> Eq for Expr<'tcx> {}
