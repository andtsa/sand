//! highest high intermediate representation:
//! the abstract syntax tree IR

use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::FunRef;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::OriginalVarRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::RegionParam;
use crate::compiler::structure::TypeConstraint;
use crate::compiler::structure::TypeParam;
use crate::compiler::structure::UniqVar;
use crate::lang::ops::*;
use crate::lang::types::*;

#[derive(Debug)]
pub struct ProgramModule<'tcx> {
    pub functions: Vec<Function<'tcx>>,
    pub module_name: ModuleRef<'tcx>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum HirVar<'tcx> {
    Decl(OriginalVarRef<'tcx>),
    Unqualified(String),
    Uniq(UniqVar<'tcx>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HirFnCall {
    Local(String),
    External { module: String, name: String },
}

#[derive(Debug, Clone)]
pub struct Parameter<'tcx> {
    pub name: HirVar<'tcx>,
    pub ty: Ty<'tcx>,
    pub range: Range,
    pub is_mutable: bool,
}

#[derive(Debug)]
pub struct Function<'tcx> {
    pub name: FunRef<'tcx>,
    pub range: Range,
    pub type_params: Vec<TypeParam>,
    pub region_params: Vec<RegionParam>,
    pub where_constraints: Vec<RegionConstraint>,
    pub type_constraints: Vec<TypeConstraint>,
    pub parameters: Vec<Parameter<'tcx>>,
    pub ret_type: Ty<'tcx>,
    pub body: Expr<'tcx>,
}

#[derive(Debug, Clone)]
pub enum Statement<'tcx> {
    Declaration {
        name: HirVar<'tcx>,
        range: Range,
        ty: Option<Ty<'tcx>>,
        is_mutable: bool,
        val: Expr<'tcx>,
    },

    /// Flat tuple-pattern binding: `let (a, mut b) = expr`.
    ///
    /// Each element is `(name, is_mutable, range)`.  `ty` is the optional
    /// type annotation on the whole binding (`let (a, b): (Int, Bool) = ...`).
    LetTuple {
        elems: Vec<(HirVar<'tcx>, bool, Range)>,
        ty: Option<Ty<'tcx>>,
        val: Expr<'tcx>,
        range: Range,
    },

    /// Constructor-pattern binding: `let E#V(payload) = expr else fallback`.
    ///
    /// The `pattern` must have a `Constructor` or `Tag` at its root
    /// (refutable). The `else_branch` is required whenever the pattern is
    /// refutable; it supplies a fallback **value** (not a pattern) of the
    /// same type as `val` whose outermost constructor must match the
    /// pattern's variant.
    LetPattern {
        pattern: HirPattern<'tcx>,
        ty: Option<Ty<'tcx>>,
        val: Expr<'tcx>,
        else_branch: Expr<'tcx>,
        range: Range,
    },

    Assignment {
        name: HirVar<'tcx>,
        range: Range,
        val: Expr<'tcx>,
    },

    /// Write-through `*reference = value` (Calculus §3.2): store `value`
    /// through a `&mut` reference. `reference : &mut T`, `value : T`.
    DerefAssign {
        reference: Expr<'tcx>,
        value: Expr<'tcx>,
        range: Range,
    },

    Expr(Expr<'tcx>),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col).
#[derive(Debug, Clone)]
pub struct Expr<'tcx> {
    pub expr: Expression<'tcx>,
    pub range: Range,
}

impl<'tcx> Expr<'tcx> {
    /// A **traversal** over the immediate sub-expressions of an `Expr`.
    ///
    /// Applies the fallible transformation `f` to every direct child `Expr`
    /// and reconstructs the parent node around the results, threading the
    /// original `range` through unchanged. Terminal nodes that contain no
    /// sub-expressions (`Var`, `Int`, `Bool`, `Unit`) are returned
    /// untouched; `Tag`, `Constructor`/`ExternalConstructor` recurse into their
    /// optional payload and `Tuple` into its elements. `Block` is
    /// deliberately *excluded*: its children are `Statement`s rather than
    /// bare `Expr`s and visiting it requires scope bookkeeping that only the
    /// caller (e.g. `uniquify`) knows how to do, so callers must match on
    /// `Block` themselves before falling back to this traversal.
    ///
    /// This is the Rust analogue of a `Traversal'` from the Haskell `lens`
    /// library, specialised to the `Either e` / `Result<_, E>` applicative
    /// (Rust has no higher-kinded types, so we cannot be polymorphic over
    /// the effect functor the way `lens` is):
    ///
    /// ```haskell
    /// subexprs :: Traversal' Expr Expr
    /// subexprs f (If c t mf) = If <$> f c <*> f t <*> traverse f mf
    /// subexprs f (BinOp l op r) = (\l' r' -> BinOp l' op r') <$> f l <*> f r
    /// subexprs _ leaf@(Var _) = pure leaf
    /// ...
    /// ```
    ///
    /// Using this traversal, a recursive rewrite pass collapses to:
    ///
    /// ```ignore
    /// fn rewrite(e: &Expr) -> Result<Expr, E> {
    ///     match &e.expr {
    ///         Expression::Var(_) | Expression::Block { .. } => /* handle specially */,
    ///         _ => e.traverse_subexprs(rewrite),
    ///     }
    /// }
    /// ```
    ///
    /// i.e. the traversal *is* the boilerplate "recurse into every child and
    /// rebuild the node" logic, leaving only the structurally-interesting
    /// cases to be written out by hand. It borrows `self` (mirroring the
    /// existing borrow-and-rebuild style of the passes) and calls `f` once
    /// per immediate child, cloning only the non-recursive payload of each
    /// node
    pub fn traverse_subexprs<E>(
        &self,
        mut f: impl FnMut(&Self) -> Result<Self, E>,
    ) -> Result<Self, E> {
        let expr = match &self.expr {
            Expression::If { cond, t, f: fb } => Expression::If {
                cond: Box::new(f(cond)?),
                t: Box::new(f(t)?),
                f: fb.as_deref().map(&mut f).transpose()?.map(Box::new),
            },
            Expression::While { cond, body } => Expression::While {
                cond: Box::new(f(cond)?),
                body: Box::new(f(body)?),
            },
            Expression::BinOp { left, op, right } => Expression::BinOp {
                left: Box::new(f(left)?),
                op: *op,
                right: Box::new(f(right)?),
            },
            Expression::UnOp { op, right } => Expression::UnOp {
                op: *op,
                right: Box::new(f(right)?),
            },
            Expression::Borrow(inner, m) => Expression::Borrow(Box::new(f(inner)?), *m),
            Expression::Deref(inner) => Expression::Deref(Box::new(f(inner)?)),
            Expression::Call {
                fn_name,
                args,
                type_args,
            } => Expression::Call {
                fn_name: fn_name.clone(),
                args: args.iter().map(&mut f).collect::<Result<_, _>>()?,
                type_args: type_args.clone(),
            },
            Expression::Match { scrutinee, arms } => Expression::Match {
                scrutinee: Box::new(f(scrutinee)?),
                arms: arms
                    .iter()
                    .map(|arm| {
                        Ok(HirMatchArm {
                            pattern: arm.pattern.clone(),
                            body: f(&arm.body)?,
                            range: arm.range,
                        })
                    })
                    .collect::<Result<_, _>>()?,
            },
            Expression::Constructor {
                type_name,
                variant,
                payload,
            } => Expression::Constructor {
                type_name: type_name.clone(),
                variant: variant.clone(),
                payload: payload.as_deref().map(&mut f).transpose()?.map(Box::new),
            },
            Expression::ExternalConstructor {
                mod_name,
                type_name,
                variant,
                payload,
            } => Expression::ExternalConstructor {
                mod_name: mod_name.clone(),
                type_name: type_name.clone(),
                variant: variant.clone(),
                payload: payload.as_deref().map(&mut f).transpose()?.map(Box::new),
            },
            Expression::Tuple(elems) => {
                Expression::Tuple(elems.iter().map(&mut f).collect::<Result<_, _>>()?)
            }
            Expression::Tag { variant, payload } => Expression::Tag {
                variant: variant.clone(),
                payload: payload.as_deref().map(&mut f).transpose()?.map(Box::new),
            },
            // Terminal nodes: no sub-expressions to traverse into, just
            // clone the (cheap) payload through unchanged.
            leaf @ (Expression::Var(_)
            | Expression::Int(_)
            | Expression::Bool(_)
            | Expression::Unit) => (*leaf).clone(),
            // `Block` mixes `Statement` children and scoping concerns.
            // callers must handle it before delegating here.
            block @ Expression::Block { .. } => (*block).clone(),
        };
        Ok(Expr {
            expr,
            range: self.range,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
        fn_name: HirFnCall,
        args: Vec<Expr<'tcx>>,
        /// Explicit type arguments from a turbofish `f::<T, …>(…)` (Memory Step
        /// C). Empty for an ordinary call. Resolved against the active
        /// type-param scope, so a `T` inside a generic function is its `Param`.
        type_args: Vec<Ty<'tcx>>,
    },
    Var(HirVar<'tcx>),
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
        type_name: String,
        variant: String,
        payload: Option<Box<Expr<'tcx>>>,
    },
    ExternalConstructor {
        mod_name: String,
        type_name: String,
        variant: String,
        payload: Option<Box<Expr<'tcx>>>,
    },
    Tag {
        variant: String,
        payload: Option<Box<Expr<'tcx>>>,
    },
    Match {
        scrutinee: Box<Expr<'tcx>>,
        arms: Vec<HirMatchArm<'tcx>>,
    },
    Tuple(Vec<Expr<'tcx>>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HirMatchArm<'tcx> {
    pub pattern: HirPattern<'tcx>,
    pub body: Expr<'tcx>,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirPattern<'tcx> {
    Constructor {
        type_name: String,
        variant: String,
        payload: Option<Box<HirPattern<'tcx>>>,
    },
    Tag {
        variant: String,
        payload: Option<Box<HirPattern<'tcx>>>,
    },
    Tuple(Vec<HirPattern<'tcx>>),
    /// integer literal in pattern position: `42` or `-7`.
    IntLit(i64),
    /// boolean literal in pattern position: `true` or `false`.
    BoolLit(bool),
    /// a variable binding that destructures part of the scrutinee, e.g. the
    /// `r` in `Circle(r)` or `a`/`b` in `(a, b)`. carries its own `Range`
    /// since it is a declaration site (needed for "declared at..."
    /// diagnostics, mirroring `OriginalVarRef`/`UniqVar` declarations
    /// elsewhere).
    Binding {
        var: HirVar<'tcx>,
        range: Range,
    },
    Wildcard,
}

impl HirVar<'_> {
    pub fn is_uniq(&self) -> bool {
        matches!(self, HirVar::Uniq(_))
    }
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
