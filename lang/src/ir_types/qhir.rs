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
use crate::compiler::structure::RegionParam;
use crate::compiler::structure::TypeParam;
use crate::compiler::structure::TypeclassRef;
use crate::compiler::structure::UniqVar;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug, Clone)]
pub struct Program<'tcx> {
    pub functions: Map<FunRef<'tcx>, Function<'tcx>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Parameter<'tcx> {
    pub name: UniqVar<'tcx>,
    pub ty: Ty<'tcx>,
    pub range: Range,
    pub is_mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Function<'tcx> {
    pub name: FunRef<'tcx>,
    pub range: Range,
    pub type_params: Vec<TypeParam>,
    pub region_params: Vec<RegionParam>,
    pub where_constraints: Vec<RegionConstraint>,
    pub type_constraints: Vec<crate::compiler::structure::TypeConstraint>,
    pub parameters: Vec<Parameter<'tcx>>,
    pub ret_type: Ty<'tcx>,
    pub body: Expr<'tcx>,
    pub src_module: ModuleRef<'tcx>,
}

#[derive(Debug, Clone, PartialOrd, Ord)]
pub enum Statement<'tcx> {
    Declaration {
        name: UniqVar<'tcx>,
        range: Range,
        ty: Option<Ty<'tcx>>,
        is_mutable: bool,
        val: Expr<'tcx>,
    },

    /// Flat tuple-pattern binding after uniquification.
    LetTuple {
        elems: Vec<(UniqVar<'tcx>, bool, Range)>,
        ty: Option<Ty<'tcx>>,
        val: Expr<'tcx>,
        range: Range,
    },

    /// Constructor-pattern binding: `let E#V(payload) = expr else fallback`.
    LetPattern {
        pattern: QPattern<'tcx>,
        ty: Option<Ty<'tcx>>,
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
    /// T`.
    DerefAssign {
        reference: Expr<'tcx>,
        value: Expr<'tcx>,
        range: Range,
    },

    Expr(Expr<'tcx>),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col).
#[derive(Debug, Clone, PartialOrd, Ord)]
pub struct Expr<'tcx> {
    pub expr: Expression<'tcx>,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Expression<'tcx> {
    If {
        cond: Box<Expr<'tcx>>,
        t: Box<Expr<'tcx>>,
        f: Option<Box<Expr<'tcx>>>,
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
        /// Explicit turbofish type arguments (Memory Step C). Empty except for
        /// type-argument intrinsics like `size_of::<T>()`.
        type_args: Vec<Ty<'tcx>>,
    },
    /// A call to a typeclass method (Step 10). The instance is unresolved here
    /// — type-checking picks it from the argument types (rewriting to
    /// `Call`, or to a deferred `typed_hir::MethodCall` when the receiver
    /// is a type parameter).
    MethodCall {
        class: TypeclassRef,
        method: String,
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
    Tag {
        variant: String,
        payload: Option<Box<Expr<'tcx>>>,
    },
    Match {
        scrutinee: Box<Expr<'tcx>>,
        arms: Vec<QMatchArm<'tcx>>,
    },
    Tuple(Vec<Expr<'tcx>>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QMatchArm<'tcx> {
    pub pattern: QPattern<'tcx>,
    pub body: Expr<'tcx>,
    pub range: Range,
}

/// a pattern after the qualify pass.
/// constructor patterns are fully resolved to (EnumRef, variant_idx),
/// tag patterns keep their string name for resolution by the type checker,
/// wildcards are left as-is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QPattern<'tcx> {
    /// fully resolved enum variant: `T::A` -> (EnumRef, 0)
    Variant {
        enum_ref: EnumRef<'tcx>,
        variant_idx: usize,
        payload: Option<Box<QPattern<'tcx>>>,
    },
    /// Bare tag `#tag`, resolved by the type checker
    Tag {
        variant: String,
        payload: Option<Box<QPattern<'tcx>>>,
    },
    /// Tuple destructuring `(p1, p2, ...)`
    Tuple(Vec<QPattern<'tcx>>),
    /// integer literal in pattern position: `42` or `-7`.
    IntLit(i64),
    /// boolean literal in pattern position: `true` or `false`.
    BoolLit(bool),
    /// a variable binding already uniquified
    Binding { var: UniqVar<'tcx>, range: Range },
    /// Wildcard `_`
    Wildcard,
}

impl<'tcx> Eq for Statement<'tcx> {}

impl<'tcx> Hash for Statement<'tcx> {
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
            DerefAssign {
                reference, value, ..
            } => {
                reference.hash(state);
                value.hash(state);
            }
            LetTuple { elems, ty, val, .. } => {
                elems.hash(state);
                ty.hash(state);
                val.hash(state);
            }
            LetPattern {
                pattern,
                ty,
                val,
                else_branch,
                ..
            } => {
                pattern.hash(state);
                ty.hash(state);
                val.hash(state);
                else_branch.hash(state);
            }
            Expr(e) => e.hash(state),
        }
    }
}

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

            (Expr(e1), Expr(e2)) => e1 == e2,
            (
                LetTuple {
                    elems: e1,
                    ty: t1,
                    val: v1,
                    ..
                },
                LetTuple {
                    elems: e2,
                    ty: t2,
                    val: v2,
                    ..
                },
            ) => e1 == e2 && t1 == t2 && v1 == v2,
            (
                LetPattern {
                    pattern: p1,
                    ty: t1,
                    val: v1,
                    else_branch: eb1,
                    ..
                },
                LetPattern {
                    pattern: p2,
                    ty: t2,
                    val: v2,
                    else_branch: eb2,
                    ..
                },
            ) => p1 == p2 && t1 == t2 && v1 == v2 && eb1 == eb2,
            _ => false,
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
