//! Higher-kinded type parameters.
//!
//! A type parameter may have a constructor kind (`F : Owned -> Owned`) and be
//! applied (`F<A>`). A typeclass can then quantify over a type constructor,
//! with instances given per constructor (`impl Container for Opt`). The
//! instance is recovered from an argument's type by unifying `F<A>` against the
//! concrete `Opt<Int>`. (Functor/Monad, whose methods need function types, are
//! exercised elsewhere; these arrow-free classes exercise the HKT machinery
//! itself.)

use lang::ir_types::typed_hir::Expression;

use crate::common::run_hir_and_mir;
use crate::common::typecheck;
use crate::common::typecheck_fails;

fn run_both(src: &str) -> Expression<'static> {
    let (hir, mir) = run_hir_and_mir(src);
    assert_eq!(hir, mir, "HIR and MIR disagree for:\n  {src}");
    hir
}

/// A higher-kinded `Container` class + an `Opt` instance, reused by several
/// tests. `unwrap_or` puts the constructor parameter `F` in argument position,
/// so the instance resolves from the call's first argument.
const CONTAINER: &str = "\
    type Opt<a> = Nothing | Just(a) \n \
    typeclass Container<F : Owned -> Owned> { \n \
        def unwrap_or<A>(x: F<A>, fallback: A): A \n \
    } \n \
    impl Container for Opt { \n \
        def unwrap_or<A>(x: Opt<A>, fallback: A): A := match x { \n \
            Opt#Nothing => fallback, \n \
            Opt#Just(v) => v, \n \
        } \n \
    } \n";

#[test]
fn hkt_class_and_instance_typecheck() {
    let (ctx, _p) = typecheck(&format!("{CONTAINER} def main(): Int := 0"));
    std::mem::forget(ctx);
}

#[test]
fn instance_resolves_from_argument_present() {
    assert_eq!(
        run_both(&format!(
            "{CONTAINER} def main(): Int := unwrap_or(Opt#Just(42), 0)"
        )),
        Expression::Int(42)
    );
}

#[test]
fn instance_resolves_from_argument_absent() {
    // `Opt#Nothing` is annotated because a bare nullary generic constructor
    // can't infer its element type (a pre-existing limitation, unrelated to HKT).
    assert_eq!(
        run_both(&format!(
            "{CONTAINER} def main(): Int := \n \
             {{ let x: Opt<Int> = Opt#Nothing; unwrap_or(x, 7) }}"
        )),
        Expression::Int(7)
    );
}

#[test]
fn dispatch_picks_the_right_instance() {
    // Two constructors, two `Container` instances; each call dispatches on its
    // argument's constructor.
    let src = "\
        type Opt<a> = Nothing | Just(a) \n \
        type Cell<a> = Wrap(a) \n \
        typeclass Container<F : Owned -> Owned> { def unwrap_or<A>(x: F<A>, fallback: A): A } \n \
        impl Container for Opt { \n \
            def unwrap_or<A>(x: Opt<A>, fallback: A): A := match x { \n \
                Opt#Nothing => fallback, Opt#Just(v) => v } } \n \
        impl Container for Cell { \n \
            def unwrap_or<A>(x: Cell<A>, fallback: A): A := match x { Cell#Wrap(v) => v } } \n \
        def main(): Int := \n \
            { let n: Opt<Int> = Opt#Nothing; unwrap_or(n, 1) + unwrap_or(Cell#Wrap(40), 0) }";
    assert_eq!(run_both(src), Expression::Int(41));
}

#[test]
fn hkt_over_a_nested_constructor_argument() {
    // `A` itself is instantiated to an aggregate, exercising arg recovery
    // through `F<(Int, Int)>`.
    assert_eq!(
        run_both(&format!(
            "{CONTAINER} def main(): Int := \n \
             match unwrap_or(Opt#Just((3, 4)), (0, 0)) {{ (a, b) => a + b }}"
        )),
        Expression::Int(7)
    );
}

// --- kind errors ---
#[test]
fn applying_a_value_parameter_is_rejected() {
    // `T : Owned` is not a constructor, so `T<A>` is a kind error.
    typecheck_fails("typeclass Bad<T> { def f<A>(x: T<A>): A } \n def main(): Int := 0");
}

#[test]
fn bare_higher_kinded_parameter_is_rejected() {
    // A constructor parameter `F` cannot stand alone as a type; it must be
    // applied (`F<A>`).
    typecheck_fails(
        "typeclass Bad<F : Owned -> Owned> { def f(x: F): Int } \n def main(): Int := 0",
    );
}

#[test]
fn constructor_arity_mismatch_is_rejected() {
    // `F : Owned -> Owned` is unary; applying it to two arguments (`F<A, B>`)
    // is an arity error.
    typecheck_fails(
        "typeclass C<F : Owned -> Owned> { def f<A, B>(x: F<A, B>): A } \n \
         def main(): Int := 0",
    );
}

// --- the `Functor`/`Applicative`/`Monad` hierarchy (from `core.sand`) ---
// instantiated for `Option`, exercising HKT instances whose methods take and
// return lambdas. The codegen path is covered by `examples/monad.sand`; these
// assert HIR/MIR interpreter agreement.

/// An `Option` with `Functor`/`Applicative`/`Monad` instances, plus an
/// `or_else` to project the result back to an `Int` for assertions.
const MONAD: &str = "\
    type Option<a> = None | Some(a) \n \
    impl Functor for Option { \n \
        def fmap<A, B>(x: Option<A>, f: A -> B): Option<B> := match x { \n \
            Option#None => Option#None, \n \
            Option#Some(v) => Option#Some(f(v)), \n \
        } \n \
    } \n \
    impl Applicative for Option { \n \
        def pure<A>(x: A): Option<A> := Option#Some(x) \n \
        def ap<A, B>(f: Option<A -> B>, x: Option<A>): Option<B> := match f { \n \
            Option#None => Option#None, \n \
            Option#Some(g) => match x { \n \
                Option#None => Option#None, \n \
                Option#Some(v) => Option#Some(g(v)), \n \
            }, \n \
        } \n \
    } \n \
    impl Monad for Option { \n \
        def bind<A, B>(x: Option<A>, f: A -> Option<B>): Option<B> := match x { \n \
            Option#None => Option#None, \n \
            Option#Some(v) => f(v), \n \
        } \n \
    } \n \
    def or_else(x: Option<Int>, d: Int): Int := match x { \n \
        Option#None => d, \n \
        Option#Some(v) => v, \n \
    } \n";

#[test]
fn functor_fmap_over_present_value() {
    assert_eq!(
        run_both(&format!(
            "{MONAD} def main(): Int := \n \
             or_else(fmap(Option#Some(21), fn (n: Int) -> n * 2), 0)"
        )),
        Expression::Int(42)
    );
}

#[test]
fn functor_fmap_over_absent_short_circuits() {
    assert_eq!(
        run_both(&format!(
            "{MONAD} def main(): Int := \n \
             {{ let none: Option<Int> = Option#None; \n \
                or_else(fmap(none, fn (n: Int) -> n * 2), 7) }}"
        )),
        Expression::Int(7)
    );
}

#[test]
fn applicative_pure_dispatches_from_expected_type() {
    // `pure` has no `F<_>` argument; its instance is recovered from the annotated
    // expected type (`Option<Int>`).
    assert_eq!(
        run_both(&format!(
            "{MONAD} def main(): Int := \n \
             {{ let lifted: Option<Int> = pure(42); or_else(lifted, 0) }}"
        )),
        Expression::Int(42)
    );
}

#[test]
fn applicative_ap_applies_wrapped_function() {
    assert_eq!(
        run_both(&format!(
            "{MONAD} def main(): Int := \n \
             {{ let wf: Option<Int -> Int> = Option#Some(fn (n: Int) -> n + 1); \n \
                or_else(ap(wf, Option#Some(21)), 0) }}"
        )),
        Expression::Int(22)
    );
}

#[test]
fn monad_bind_chains_computations() {
    assert_eq!(
        run_both(&format!(
            "{MONAD} def main(): Int := \n \
             or_else(bind(Option#Some(21), fn (n: Int) -> Option#Some(n + 22)), 0)"
        )),
        Expression::Int(43)
    );
}

#[test]
fn monad_instance_requires_superclasses() {
    // `Monad requires Applicative requires Functor`; an `impl Monad` without the
    // superclass instances is rejected.
    typecheck_fails(
        "type Option<a> = None | Some(a) \n \
         impl Monad for Option { \n \
             def bind<A, B>(x: Option<A>, f: A -> Option<B>): Option<B> := match x { \n \
                 Option#None => Option#None, \n \
                 Option#Some(v) => f(v), \n \
             } \n \
         } \n def main(): Int := 0",
    );
}

// --- do-notation ---
// any block containing a top-level `<-` desugars to nested `bind` calls.
// Reuses the `MONAD` Option instance above.

#[test]
fn do_notation_single_bind() {
    // `{ x <- Some(20); Some(x + 1) }` ≡ `bind(Some(20), fn x -> Some(x + 1))`.
    assert_eq!(
        run_both(&format!(
            "{MONAD} def main(): Int := \n \
             or_else({{ x: Int <- Option#Some(20); Option#Some(x + 1) }}, 0)"
        )),
        Expression::Int(21)
    );
}

#[test]
fn do_notation_chains_and_keeps_pure_lets() {
    // Two binds with an ordinary `let` between them; all bind to the same monad.
    assert_eq!(
        run_both(&format!(
            "{MONAD} def main(): Int := or_else({{ \n \
                 x: Int <- Option#Some(20); \n \
                 let doubled = x * 2; \n \
                 y: Int <- Option#Some(doubled + 2); \n \
                 Option#Some(x + y) \n \
             }}, 0)"
        )),
        // x=20, doubled=40, y=42, 20+42 = 62
        Expression::Int(62)
    );
}

#[test]
fn do_notation_short_circuits_on_none() {
    // A `None` bound by `<-` short-circuits the whole block (Monad bind for
    // Option).
    assert_eq!(
        run_both(&format!(
            "{MONAD} def main(): Int := or_else({{ \n \
                 let none: Option<Int> = Option#None; \n \
                 x: Int <- none; \n \
                 Option#Some(x + 1) \n \
             }}, -1)"
        )),
        Expression::Int(-1)
    );
}

#[test]
fn do_notation_block_without_trailing_expr_is_rejected() {
    // A block using `<-` must end in a trailing expression (its monadic result).
    typecheck_fails(&format!(
        "{MONAD} def main(): Int := \n \
         or_else({{ x: Int <- Option#Some(1); }}, 0)"
    ));
}

#[test]
fn ordinary_block_without_bind_is_unaffected() {
    // No `<-` → an ordinary block; type is its trailing expression, not `F<_>`.
    assert_eq!(
        run_both("def main(): Int := { let x = 20; x + 1 }"),
        Expression::Int(21)
    );
}

// --- soundness: method-call argument validation + unsolved-param guards// ---
// ---
#[test]
fn method_call_rejects_wrong_arg_type() {
    // A `Bool` in an `Int` parameter of a typeclass method must be rejected
    // (previously slipped through: the receiver-solving unify ignored failures).
    typecheck_fails(
        "typeclass Same<T> { def same(x: T, y: T): Bool } \n \
         impl Same for Int { def same(x: Int, y: Int): Bool := x == y } \n \
         def main(): Int := if same(5, true) then 1 else 0",
    );
}

#[test]
fn method_call_rejects_non_function_where_function_expected() {
    // Passing `5` where `fmap` expects `A -> B`.
    typecheck_fails(&format!(
        "{MONAD} def main(): Int := or_else(fmap(Option#Some(1), 5), 0)"
    ));
}

#[test]
fn method_call_rejects_wrong_arity() {
    typecheck_fails(
        "typeclass Same<T> { def same(x: T, y: T): Bool } \n \
         impl Same for Int { def same(x: Int, y: Int): Bool := x == y } \n \
         def main(): Int := if same(5) then 1 else 0",
    );
}

#[test]
fn generic_fn_with_uninferable_return_param_is_rejected() {
    // A type parameter that appears only in the return type, called with no
    // expected type, must be a clean error, not an unbound-Param crash in mono.
    typecheck_fails(
        "type Opt<a> = None | Some(a) \n \
         def mkempty<A>(c: Int): Opt<A> := Opt#None \n \
         def main(): Int := { let _x = mkempty(0); 0 }",
    );
}

#[test]
fn generic_fn_return_param_pinned_by_annotation_is_ok() {
    // The same call type-checks when the result type is annotated (the param is
    // then solved from the expected type).
    run_both(
        "type Opt<a> = None | Some(a) \n \
         def mkempty<A>(c: Int): Opt<A> := Opt#None \n \
         def or0(x: Opt<Int>): Int := match x { Opt#Some(v) => v, Opt#None => 0 } \n \
         def main(): Int := { let e: Opt<Int> = mkempty(0); or0(e) }",
    );
}
