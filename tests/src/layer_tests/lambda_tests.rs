//! Step 13 (milestone 2) — lambda values + indirect calls. A `fn (x: T) -> e`
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
fn lambda_typechecks_as_a_value() {
    let (ctx, _p) = typecheck("def main(): Int := { let g: Int -> Int = fn (x: Int) -> x; 0 }");
    std::mem::forget(ctx);
}

#[test]
fn capturing_an_enclosing_variable_is_rejected() {
    // `y` is not in scope inside the lambda (non-capturing milestone).
    typecheck_fails(
        "def main(): Int := { let y: Int = 5; let g = fn (x: Int) -> x + y; g(1) }",
    );
}

#[test]
fn applying_a_non_function_is_rejected() {
    typecheck_fails("def main(): Int := { let x: Int = 5; x(1) }");
}
