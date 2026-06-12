//! The split of the old overloaded `&` into bitwise `&&` (Int) and logical
//! `and` (Bool), which frees single `&` for borrow syntax (Step 7 pre-step).

use lang::ir_types::typed_hir::Expression;

use crate::common::parse_fails;
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

// ── logical AND: `and` on Bool
// ────────────────────────────────────────────────

#[test]
fn logical_and_true() {
    assert_eq!(
        run_both("def main(): Bool := true and true"),
        Expression::Bool(true)
    );
}

#[test]
fn logical_and_false() {
    assert_eq!(
        run_both("def main(): Bool := true and false"),
        Expression::Bool(false)
    );
}

#[test]
fn logical_and_chains_comparisons() {
    assert_eq!(
        run_both("def main(): Bool := (1 < 2) and (3 > 2)"),
        Expression::Bool(true)
    );
}

// ── bitwise AND: `&&` on Int
// ──────────────────────────────────────────────────

#[test]
fn bitwise_and_ints() {
    // 6 = 0b110, 5 = 0b101, 6 & 5 = 0b100 = 4
    assert_eq!(run_both("def main(): Int := 6 && 5"), Expression::Int(4));
}

#[test]
fn bitwise_and_odd_check() {
    assert_eq!(run_both("def main(): Int := 7 && 1"), Expression::Int(1));
}

// ── the operators are type-restricted
// ─────────────────────────────────────────

#[test]
fn logical_and_on_int_is_rejected() {
    typecheck_fails("def main(): Int := 5 and 3");
}

#[test]
fn bitwise_and_on_bool_is_rejected() {
    typecheck_fails("def main(): Bool := true && false");
}

#[test]
fn both_operators_type_check_in_their_domain() {
    typecheck("def main(): Bool := true and false");
    typecheck("def main(): Int := 5 && 3");
}

// ── single `&` is no longer an operator (freed for borrows)
// ───────────────────

#[test]
fn single_ampersand_is_not_an_operator() {
    parse_fails("def main(): Bool := true & false");
    parse_fails("def main(): Int := 5 & 3");
}
