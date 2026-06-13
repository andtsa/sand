//! Call-site region inference: a function with explicit lifetime parameters
//! (`def f<'r>(x: &'r T)`) is callable with an ordinary borrow. Reference
//! regions carry no type-level constraint (safety is the lexical escape check),
//! so they are inferred away at the call boundary — `&'r T` accepts any `&_ T`.

use lang::ir_types::typed_hir::Expression;

use crate::common::run_hir;
use crate::common::run_mir_as_expr;
use crate::common::typecheck;
use crate::common::typecheck_fails;

fn run_both(src: &str) -> Expression<'static> {
    let hir = run_hir(src);
    let mir = run_mir_as_expr(src);
    assert_eq!(hir, mir, "HIR and MIR disagree for:\n  {src}");
    hir
}

#[test]
fn explicit_region_function_is_callable_with_a_borrow() {
    typecheck("def takes<'r>(x: &'r Int): Int := *x \n def main(): Int := takes(&7)");
}

#[test]
fn explicit_region_function_runs() {
    assert_eq!(
        run_both("def takes<'r>(x: &'r Int): Int := *x \n def main(): Int := takes(&7)"),
        Expression::Int(7)
    );
}

#[test]
fn explicit_mut_region_function_is_callable() {
    assert_eq!(
        run_both("def takes<'r>(x: &'r mut Int): Int := *x \n def main(): Int := takes(&mut 9)"),
        Expression::Int(9)
    );
}

#[test]
fn single_lifetime_longest_runs() {
    // returns one of two borrows tied to the same lifetime, then reads it.
    assert_eq!(
        run_both(
            "def longest<'a>(x: &'a Int, y: &'a Int): &'a Int := if true then x else y \n \
             def main(): Int := { let a = 3; let b = 4; *longest(&a, &b) }"
        ),
        Expression::Int(3)
    );
}

#[test]
fn two_lifetimes_with_where_clause_runs() {
    // `'a >= 'b`; returns the `'a`-borrow. The `where` clause parses and the
    // call type-checks (regions inferred away).
    assert_eq!(
        run_both(
            "def pick<'a, 'b>(x: &'a Int, y: &'b Int): &'a Int where 'a >= 'b := x \n \
             def main(): Int := { let a = 1; let b = 2; *pick(&a, &b) }"
        ),
        Expression::Int(1)
    );
}

#[test]
fn region_parametric_result_flows_into_a_binding() {
    // the inferred-region result binds to a `&Int` local and is then read.
    assert_eq!(
        run_both(
            "def id_ref<'r>(x: &'r Int): &'r Int := x \n \
             def main(): Int := { let a = 5; let r = id_ref(&a); *r + 1 }"
        ),
        Expression::Int(6)
    );
}

#[test]
fn returning_a_call_result_over_a_local_is_rejected() {
    // the call result's region is the `meet` of its argument regions (item 8), so
    // a result tied to a *local* borrow names a local region and cannot be
    // returned (Calculus §6.3, frame boundary).
    typecheck_fails(
        "def longest<'a>(x: &'a Int, y: &'a Int): &'a Int := if true then x else y \n \
         def f(): &Int := { let a = 1; longest(&a, &a) } \n \
         def main(): Int := 0",
    );
}

#[test]
fn returning_a_call_result_tied_to_a_parameter_is_accepted() {
    // `wrapper` forwards a reference *parameter* through a call; the `meet`
    // instantiates the result to the parameter's region, which outlives the call,
    // so it is returnable (Calculus §6.3, item 8 reconciliation).
    typecheck(
        "def id_ref<'r>(x: &'r Int): &'r Int := x \n \
         def wrapper<'r>(r: &'r Int): &'r Int := id_ref(r) \n \
         def main(): Int := 0",
    );
}
