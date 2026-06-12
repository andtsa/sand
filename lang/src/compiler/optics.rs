//! Optics for the compiler's intermediate representations.
//!
//! Optics are composable abstractions for focusing on parts of data
//! structures — "functional references" that let you `get`, `set`, or
//! `traverse` into a substructure without writing the surrounding
//! pattern-match boilerplate by hand. This module collects the optics used
//! by the `qualify`/`uniquify` passes, following the classic Haskell `lens`
//! hierarchy:
//!
//! ```text
//!                 Traversal
//!                 /        \
//!              Lens        Prism
//!                 \        /
//!                  Getter / Fold (read-only)
//! ```
//!
//! In `lens`, every one of these is secretly the same type, parameterised
//! over the choice of functor `f`:
//!
//! ```haskell
//! type Lens      s t a b = forall f. Functor     f => (a -> f b) -> s -> f t
//! type Prism     s t a b = forall f. Choice      p => p a (f b)  -> p s (f t)
//! type Traversal s t a b = forall f. Applicative f => (a -> f b) -> s -> f t
//! ```
//!
//! Rust has no higher-kinded types, so we cannot write one polymorphic
//! definition and instantiate `f` to `Identity`, `Const`, or `Either e` as
//! needed the way `lens` does. Instead we specialise directly to the
//! `Result<_, E>` ("Either e") applicative/monad — the one instantiation
//! that actually matters for compiler passes, since every pass is an
//! effectful, fallible, tree-rewriting traversal. This is exactly the
//! `traverseOf`/`mapMOf` use of a `Traversal` in Haskell:
//!
//! ```haskell
//! traverseOf subexprs qualifyExpr :: Expr -> Either QualifyError Expr
//! ```
//!
//! ## What lives where
//!
//! * **Traversal** — `hhir::Expr::traverse_subexprs` (in `ir_types::hhir`)
//!   focuses on every immediate child expression of an `Expr` and rebuilds the
//!   parent around the (transformed) results. It is the "recurse-and-rebuild"
//!   boilerplate of a tree rewrite, factored out once so that passes only need
//!   to special-case the structurally interesting nodes (see `uniquify_expr`).
//!
//! * **Prism** — [`UniqPrism`] focuses on the `Uniq` variant of `HirVar`, the
//!   only variant that should remain once the uniquify stage has run. `preview`
//!   safely extracts the payload (`Option`), `review` safely re-injects it;
//!   together they replace the repeated `let HirVar::Uniq(u) = v else {
//!   internal_bug!(..) }` idiom with a single named, reusable optic.
//!
//! * **Getter (composed)** — `QualfiyCtx::resolve_constructor` (in
//!   `passes::qualify`) composes the two-step `lookup_enum_by_name` then
//!   `lookup_variant` lookup into one `(type_name, variant) -> (EnumRef,
//!   variant_idx)` getter, the way you would write `to enumByName . to
//!   variantByEnum` in `lens`.

use crate::compiler::structure::UniqVar;
use crate::internal_bug;
use crate::ir_types::hhir::HirVar;

/// A **prism** focusing on the [`HirVar::Uniq`] variant.
///
/// ```haskell
/// _Uniq :: Prism' HirVar UniqVar
/// _Uniq = prism' Uniq (\case Uniq u -> Just u; _ -> Nothing)
/// --              ^review        ^preview
/// ```
///
/// By the time the qualify stage runs, `uniquify` has already rewritten
/// every `HirVar::Decl`/`HirVar::Unqualified` into `HirVar::Uniq`, so a
/// `Decl`/`Unqualified` surviving to this point is an internal-compiler-error,
/// not a user-facing one
pub struct UniqPrism;

impl UniqPrism {
    /// `preview`: attempt to focus on the `Uniq` payload of a `HirVar`.
    /// Returns `None` for any other variant, mirroring `Prism`'s partial `get`.
    pub fn preview<'tcx>(v: HirVar<'tcx>) -> Option<UniqVar<'tcx>> {
        match v {
            HirVar::Uniq(u) => Some(u),
            _ => None,
        }
    }

    /// `review`: construct a `HirVar` from a `UniqVar`. This direction of a
    /// prism never fails — it is just the variant constructor.
    pub fn review<'tcx>(u: UniqVar<'tcx>) -> HirVar<'tcx> {
        HirVar::Uniq(u)
    }

    /// `preview` composed with an internal-bug panic on mismatch.
    ///
    /// this is the idiom this module exists to replace:
    /// ```ignore
    /// let HirVar::Uniq(u) = v else { internal_bug!("...: {v:?}") }
    /// ```
    /// becomes
    /// ```ignore
    /// UniqPrism::expect(v, "...")
    /// ```
    /// the `msg` should describe
    /// *where* the unqualified variable was encountered
    pub fn expect<'tcx>(v: HirVar<'tcx>, msg: &str) -> UniqVar<'tcx> {
        match Self::preview(v.clone()) {
            Some(u) => u,
            None => internal_bug!("{msg}: {v:?}"),
        }
    }
}
