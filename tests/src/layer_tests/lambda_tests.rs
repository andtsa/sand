//! Lambda values + indirect calls. A `fn (x: T) -> e`
//! evaluates to a closure (lifted to a top-level function during
//! monomorphisation); calling a local of function type (`g(arg)`) applies it.
//! Non-capturing only: a lambda body that references an enclosing variable is
//! rejected. Lifting + MIR + codegen are in, so these check HIR/MIR agreement.

use lang::ir_types::typed_hir::Expression;

use crate::common::run_hir_and_mir;
use crate::common::typecheck;
use crate::common::typecheck_fails;

fn run_both(src: &str) -> Expression<'static> {
    let (hir, mir) = run_hir_and_mir(src);
    assert_eq!(hir, mir, "HIR and MIR disagree for:\n  {src}");
    hir
}

#[test]
fn lambda_bound_and_called() {
    assert_eq!(
        run_both("def main(): Int := { let g = fn (x: Int) -> x + 1; g(41) }"),
        Expression::Int(42)
    );
}

#[test]
fn lambda_passed_to_a_function_and_applied() {
    // `apply` calls its function-typed parameter indirectly; the caller passes a
    // lambda.
    assert_eq!(
        run_both(
            "def apply(f: Int -> Int, x: Int): Int := f(x) \n \
             def main(): Int := apply(fn (n: Int) -> n * 2, 21)",
        ),
        Expression::Int(42)
    );
}

#[test]
fn lambda_returned_then_called() {
    // A function returning a lambda, then the result applied.
    assert_eq!(
        run_both(
            "def adder(): Int -> Int := fn (n: Int) -> n + 10 \n \
             def main(): Int := { let g = adder(); g(32) }",
        ),
        Expression::Int(42)
    );
}

#[test]
fn lambda_captures_a_local_by_move() {
    assert_eq!(
        run_both(
            "def apply(f: Int -> Int, x: Int): Int := f(x) \n \
             def main(): Int := { let y: Int = 10; let g = fn (x: Int) -> x + y; apply(g, 32) }",
        ),
        Expression::Int(42)
    );
}

#[test]
fn lambda_captures_multiple_locals() {
    assert_eq!(
        run_both(
            "def apply(f: Int -> Int, x: Int): Int := f(x) \n \
             def main(): Int := { \n \
                 let a: Int = 10; let b: Int = 100; \n \
                 let g = fn (x: Int) -> x + a + b; \n \
                 apply(g, 1) }",
        ),
        Expression::Int(111)
    );
}

#[test]
fn escaping_closure_captures_a_parameter() {
    // The returned closure outlives `make_adder`'s frame, capturing its
    // parameter `n` (the environment is heap-allocated).
    assert_eq!(
        run_both(
            "def apply(f: Int -> Int, x: Int): Int := f(x) \n \
             def make_adder(n: Int): Int -> Int := fn (x: Int) -> x + n \n \
             def main(): Int := apply(make_adder(30), 12)",
        ),
        Expression::Int(42)
    );
}

#[test]
fn lambda_typechecks_as_a_value() {
    let (ctx, _p) = typecheck("def main(): Int := { let g: Int -> Int = fn (x: Int) -> x; 0 }");
    std::mem::forget(ctx);
}

// --- calling modes ---
// `->` (reusable), `-[Owned]>` (consuming), `-[BorrowedMut]>`
#[test]
fn reusable_closure_is_callable_repeatedly() {
    assert_eq!(
        run_both("def main(): Int := { let g = fn (x: Int) -> x + 1; g(10) + g(20) }"),
        Expression::Int(32)
    );
}

#[test]
fn consuming_closure_is_callable_once() {
    assert_eq!(
        run_both("def main(): Int := { let g = fn (x: Int) -[Owned]> x + 1; g(41) }"),
        Expression::Int(42)
    );
}

#[test]
fn consuming_closure_called_twice_is_rejected() {
    // `-[Owned]>` (FnOnce) is consumed by the call, so a second call is a
    // use-after-move.
    typecheck_fails("def main(): Int := { let g = fn (x: Int) -[Owned]> x + 1; g(1) + g(2) }");
}

#[test]
fn reusable_subsumes_consuming_at_a_call_site() {
    // A reusable lambda may be passed where a consuming arrow is expected
    // (`Fn ⊆ FnOnce`).
    assert_eq!(
        run_both(
            "def use_once(f: Int -[Owned]> Int): Int := f(5) \n \
             def main(): Int := use_once(fn (x: Int) -> x + 37)",
        ),
        Expression::Int(42)
    );
}

#[test]
fn applying_a_non_function_is_rejected() {
    typecheck_fails("def main(): Int := { let x: Int = 5; x(1) }");
}
