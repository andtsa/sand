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

    /// Flat tuple-pattern binding: `let (a, mut b) = expr`.
    ///
    /// Each element is `(name, is_mutable, range)`.  `ty` is the optional
    /// type annotation on the whole binding (`let (a, b): (Int, Bool) = ...`).
    LetTuple {
        elems: Vec<(HirVar, bool, Range)>,
        ty: Option<Ty>,
        val: Expr,
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
        pattern: HirPattern,
        ty: Option<Ty>,
        val: Expr,
        else_branch: Expr,
        range: Range,
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

impl Expr {
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
    /// caller (e.g. `uniquify`) knows how to do — so callers must match on
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
    /// node — exactly as much cloning as the hand-written version did.
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
            Expression::Call { fn_name, args } => Expression::Call {
                fn_name: fn_name.clone(),
                args: args.iter().map(&mut f).collect::<Result<_, _>>()?,
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
            // `Block` mixes `Statement` children and scoping concerns —
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
        payload: Option<Box<Expr>>,
    },
    ExternalConstructor {
        mod_name: String,
        type_name: String,
        variant: String,
        payload: Option<Box<Expr>>,
    },
    Tag {
        variant: String,
        payload: Option<Box<Expr>>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<HirMatchArm>,
    },
    Tuple(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub body: Expr,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirPattern {
    Constructor {
        type_name: String,
        variant: String,
        payload: Option<Box<HirPattern>>,
    },
    Tag {
        variant: String,
        payload: Option<Box<HirPattern>>,
    },
    Tuple(Vec<HirPattern>),
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
        var: HirVar,
        range: Range,
    },
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
