//! # mtl but in rust
//!
//! Rust has no higher-kinded types, so a transformer that is *generic over its
//! base monad* (`StateT s m`) cannot be written the naïve Haskell way: the bind
//! of `StateT<S, M>` would need `for<A, B> M::Of<(A,S)>: Monad<Of<(B,S)> =
//! ..>`, and Rust's `for<..>` quantifies only lifetimes, never types. So this
//! module uses two ideas together:
//!
//! 1. "Defunctionalised" base
//!    (Yallop & White, "lightweight higher-kinded polymorphism",
//!    https://www.cl.cam.ac.uk/~jdy22/papers/lightweight-higher-kinded-polymorphism.pdf)
//!    a base monad is named by a zero-sized *witness* `W`, and
//!    [`Apply`] maps `W` + element `A` to the concrete type `W` applied to `A`
//!    ([`Ap<W, A>`]). The [`Monad`] class carries `pure`/`bind` as *static*
//!    methods on the witness, generic in the element types at the **method**
//!    level, so no `for<..>`-over-types bound is ever required.
//! 2. Inherent methods on the concrete transformer [`StateT`] is a real struct
//!    with inherent `bind`/`map`/`lift`/.. Calling `m.bind(f)` resolves the
//!    base witness from `m`'s own type, so usage stays ergonomic and the macro
//!    [`mdo!`] desugars `x <- m; rest` to `m.bind(move |x| rest)`.
//!
//! The result `StateT<S, MW>` is reusable over any base monad witness `MW`
//! (here [`ExW`] for `Result` and [`IdW`] for `Identity`), it composes, and it
//! captures borrows.
//!
//! The implementation here is obviously for educational purpose.
//! the `higher` crate (https://docs.rs/crate/higher/latest) provides
//! largely the same structure, with some more functionality, and
//! an equally cursed result.

#![allow(dead_code)]

// HKT witness

/// `W: Apply<A>` means "the type constructor named by witness `W`, applied to
/// element `A`, is the concrete type `W::T`". E.g. `ExW<E>: Apply<A>` with
/// `T = Result<A, E>`.
pub trait Apply<A> {
    type T;
}

/// The concrete type of base-monad witness `W` applied to element `A`.
pub type Ap<W, A> = <W as Apply<A>>::T;

/// The monad class, defined on the **witness** as static methods (so the
/// element types are method-generic and no higher-kinded bound is needed).
pub trait Monad {
    fn pure<A>(a: A) -> Ap<Self, A>
    where
        Self: Apply<A> + Sized;

    fn bind<A, B, F>(m: Ap<Self, A>, f: F) -> Ap<Self, B>
    where
        Self: Apply<A> + Apply<B> + Sized,
        F: FnOnce(A) -> Ap<Self, B>;

    fn map<A, B, F>(m: Ap<Self, A>, f: F) -> Ap<Self, B>
    where
        Self: Apply<A> + Apply<B> + Sized,
        F: FnOnce(A) -> B,
    {
        // associated types are not injective, so `B` must be given explicitly.
        Self::bind::<A, B, _>(m, move |a| Self::pure(f(a)))
    }
}

// base monad: Except<E>  (= Result<_, E>)

/// Witness for the `Result<_, E>` monad (a short-circuiting "except" effect).
pub struct ExW<E>(core::marker::PhantomData<E>);

impl<A, E> Apply<A> for ExW<E> {
    type T = Result<A, E>;
}
impl<E> Monad for ExW<E> {
    fn pure<A>(a: A) -> Ap<Self, A> {
        Ok(a)
    }
    fn bind<A, B, F>(m: Ap<Self, A>, f: F) -> Ap<Self, B>
    where
        F: FnOnce(A) -> Ap<Self, B>,
    {
        m.and_then(f)
    }
}

// base monad: Identity

/// Witness for the identity monad (no effect); `Ap<IdW, A> = A`.
pub struct IdW;

impl<A> Apply<A> for IdW {
    type T = A;
}
impl Monad for IdW {
    fn pure<A>(a: A) -> Ap<Self, A> {
        a
    }
    fn bind<A, B, F>(m: Ap<Self, A>, f: F) -> Ap<Self, B>
    where
        F: FnOnce(A) -> Ap<Self, B>,
    {
        f(m)
    }
}

// transformer: StateT<'a, S, MW>

/// `StateT<'a, S, MW, A>` — a stateful computation `S -> MW (A, S)` over base
/// monad witness `MW`, boxed so it can capture borrows of lifetime `'a` (e.g.
/// `&'tcx` IR nodes). The state `S` is threaded by move, so it may itself hold
/// a `&mut` without being `Clone`.
pub struct StateT<'a, S, MW, A>
where
    MW: Apply<(A, S)>,
{
    // the `S -> MW (A, S)` action; boxed so it can capture `'a` borrows.
    #[allow(clippy::type_complexity)]
    run: Box<dyn FnOnce(S) -> Ap<MW, (A, S)> + 'a>,
}

impl<'a, S: 'a, MW, A: 'a> StateT<'a, S, MW, A>
where
    MW: Monad + Apply<(A, S)>,
    Ap<MW, (A, S)>: 'a,
{
    /// Build a computation from its `S -> MW (A, S)` action.
    pub fn new(f: impl FnOnce(S) -> Ap<MW, (A, S)> + 'a) -> Self {
        StateT { run: Box::new(f) }
    }

    /// Run the computation against an initial state, yielding the base-monad
    /// action `MW (A, S)`.
    pub fn run(self, s: S) -> Ap<MW, (A, S)> {
        (self.run)(s)
    }

    /// Inject a pure value, leaving the state untouched (`return`/`pure`).
    pub fn pure(a: A) -> Self {
        StateT::new(move |s| MW::pure((a, s)))
    }

    /// Monadic bind: run `self`, feed its result to `f`, thread the state, and
    /// short-circuit through the base monad on the way.
    pub fn bind<B: 'a>(self, f: impl FnOnce(A) -> StateT<'a, S, MW, B> + 'a) -> StateT<'a, S, MW, B>
    where
        MW: Apply<(B, S)>,
        Ap<MW, (B, S)>: 'a,
    {
        StateT::new(move |s| {
            let inner = (self.run)(s); // Ap<MW, (A, S)>
            MW::bind::<(A, S), (B, S), _>(inner, move |(a, s2)| (f(a).run)(s2))
        })
    }

    /// Functorial map over the result value.
    pub fn map<B: 'a>(self, f: impl FnOnce(A) -> B + 'a) -> StateT<'a, S, MW, B>
    where
        MW: Apply<(B, S)>,
        Ap<MW, (B, S)>: 'a,
    {
        self.bind(move |a| StateT::pure(f(a)))
    }

    /// `MonadTrans::lift` — lift a base-monad action into the transformer.
    pub fn lift(m: Ap<MW, A>) -> Self
    where
        MW: Apply<A>,
        Ap<MW, A>: 'a,
    {
        StateT::new(move |s| MW::bind::<A, (A, S), _>(m, move |a| MW::pure((a, s))))
    }
}

// state capability (any base)

/// General state access: a step `\s -> (a, s')` returning a value and the next
/// state. `get`/`gets`/`put` are special cases; this one form covers a pass
/// that reads-and-updates in one go (e.g. mint a fresh name *and* record it).
pub fn state<'a, S: 'a, MW, A: 'a>(f: impl FnOnce(S) -> (A, S) + 'a) -> StateT<'a, S, MW, A>
where
    MW: Monad + Apply<(A, S)>,
    Ap<MW, (A, S)>: 'a,
{
    StateT::new(move |s| MW::pure(f(s)))
}

impl<'a, S: 'a, MW> StateT<'a, S, MW, ()>
where
    MW: Monad + Apply<((), S)>,
    Ap<MW, ((), S)>: 'a,
{
    /// Transform the state, returning unit (`modify`).
    pub fn modify(f: impl FnOnce(S) -> S + 'a) -> Self {
        StateT::new(move |s| MW::pure(((), f(s))))
    }
}

// traversal combinators

/// Run a list of actions left-to-right, threading the state through each and
/// collecting their results (`sequence`). The base monad short-circuits, so an
/// error in any action aborts the whole sequence.
pub fn sequence<'a, S: 'a, MW, A: 'a>(
    actions: Vec<StateT<'a, S, MW, A>>,
) -> StateT<'a, S, MW, Vec<A>>
where
    MW: Monad + Apply<(A, S)> + Apply<(Vec<A>, S)> + 'a,
    Ap<MW, (A, S)>: 'a,
    Ap<MW, (Vec<A>, S)>: 'a,
{
    let mut acc: StateT<'a, S, MW, Vec<A>> = StateT::pure(Vec::with_capacity(actions.len()));
    for act in actions {
        acc = acc.bind(move |mut v| {
            act.bind(move |a| {
                v.push(a);
                StateT::pure(v)
            })
        });
    }
    acc
}

/// `traverse`/`mapM`: apply `f` to each item, threading state, collecting the
/// results into a `Vec` (or short-circuiting on the first error).
pub fn traverse<'a, S: 'a, MW, X, A: 'a>(
    items: impl IntoIterator<Item = X>,
    f: impl FnMut(X) -> StateT<'a, S, MW, A>,
) -> StateT<'a, S, MW, Vec<A>>
where
    MW: Monad + Apply<(A, S)> + Apply<(Vec<A>, S)> + 'a,
    Ap<MW, (A, S)>: 'a,
    Ap<MW, (Vec<A>, S)>: 'a,
{
    sequence(items.into_iter().map(f).collect())
}

// except capability (base = ExW<E>)

impl<'a, S: 'a, E: 'a, A: 'a> StateT<'a, S, ExW<E>, A> {
    /// Abort the whole computation with an error (`throwError`).
    pub fn throw(e: E) -> Self {
        StateT::new(move |_s| Err(e))
    }
}

// do-notation

/// Haskell-style `do` for any of the monads in [this
/// module][`lang::compiler::structure::mtl`].
///
/// ```ignore
/// mdo! {
///     x <- action_a();          // bind
///     let y = pure_expr;        // ordinary let
///     side_effecting_action();  // sequence, discard result
///     final_action(x, y)        // the block's value
/// }
/// ```
#[macro_export]
macro_rules! mdo {
    // bind:  x <- expr; rest
    ($v:ident <- $e:expr ; $($rest:tt)*) => {
        $e.bind(move |$v| $crate::mdo!($($rest)*))
    };
    // ordinary let:  let pat = expr; rest
    (let $v:pat = $e:expr ; $($rest:tt)*) => {
        { let $v = $e; $crate::mdo!($($rest)*) }
    };
    // sequence (discard the unit result):  expr; rest
    ($e:expr ; $($rest:tt)*) => {
        $e.bind(move |_| $crate::mdo!($($rest)*))
    };
    // final expression
    ($e:expr) => { $e };
}

pub use crate::mdo;

#[cfg(test)]
mod tests {
    use super::*;

    type St<'a, A> = StateT<'a, i32, ExW<String>, A>;

    fn tick<'a>() -> St<'a, i32> {
        state(|n: i32| (n, n + 1))
    }

    #[test]
    fn state_threads_and_except_short_circuits() {
        // state threading: two ticks then add both to the state
        let prog: St<()> = mdo! {
            a <- tick();
            b <- tick();
            StateT::<i32, ExW<String>, ()>::modify(move |s| s + a + b)
        };
        assert_eq!(prog.run(10), Ok(((), 33))); // a=10,b=11, state 12 +10+11

        // error aborts and is threaded through the base Except
        let guarded = |x: i32| -> St<()> {
            mdo! {
                cur <- tick();
                (if cur > 2 { St::throw(format!("too big: {cur}")) } else { St::pure(()) });
                StateT::<i32, ExW<String>, ()>::modify(move |s| s + x)
            }
        };
        assert_eq!(guarded(5).run(0), Ok(((), 6)));
        assert_eq!(guarded(5).run(3), Err("too big: 3".to_string()));
    }

    #[test]
    fn reusable_over_identity_base() {
        let pure_state: StateT<i32, IdW, i32> = state(|n: i32| (n * 2, n + 1));
        assert_eq!(pure_state.run(7), (14, 8));
    }

    #[test]
    fn traverse_threads_state_and_collects() {
        // tick three times, collecting the values, leaving state advanced by 3.
        let prog: St<Vec<i32>> = traverse(0..3, |_| tick());
        assert_eq!(prog.run(10), Ok((vec![10, 11, 12], 13)));
    }

    #[test]
    fn captures_borrows_with_a_lifetime() {
        fn labelled<'a>(label: &'a str) -> St<'a, String> {
            state(move |n: i32| (format!("{label}={n}"), n))
        }
        assert_eq!(labelled("score").run(99), Ok(("score=99".to_string(), 99)));
    }
}
