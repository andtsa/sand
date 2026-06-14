//! Step 11 — higher-kinded type parameters.
//!
//! A type parameter may have a constructor kind (`F : Owned -> Owned`) and be
//! applied (`F<A>`). A typeclass can then quantify over a type constructor, with
//! instances given per constructor (`impl Container for Opt`). The instance is
//! recovered from an argument's type by unifying `F<A>` against the concrete
//! `Opt<Int>`. (Functor/Monad — whose methods need function types — wait for
//! Step 13; these arrow-free classes exercise the HKT machinery itself.)

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

// ── kind errors ──────────────────────────────────────────────────────────

#[test]
fn applying_a_value_parameter_is_rejected() {
    // `T : Owned` is not a constructor, so `T<A>` is a kind error.
    typecheck_fails(
        "typeclass Bad<T> { def f<A>(x: T<A>): A } \n def main(): Int := 0",
    );
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
