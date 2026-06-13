//! Dereference (`*e`): reading through a reference (`&T`/`&mut T` -> T).
//!
//! Borrows are erased before codegen, so `*e` lowers transparently. Reading a
//! `Copy` value out of a borrow duplicates it (fine); reading a non-`Copy`
//! value would move it out of the borrow, which the ownership pass rejects.

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

// ── a function can read its borrowed parameters (the usability win)
// ───────────

#[test]
fn function_reads_a_borrowed_parameter() {
    assert_eq!(
        run_both("def get(r: &Int): Int := *r \n def main(): Int := get(&7)"),
        Expression::Int(7)
    );
}

#[test]
fn function_adds_two_borrowed_parameters() {
    // a genuinely useful borrow program: sum two ints by reference.
    assert_eq!(
        run_both("def add(a: &Int, b: &Int): Int := *a + *b \n def main(): Int := add(&3, &4)"),
        Expression::Int(7)
    );
}

// ── deref round-trips
// ─────────────────────────────────────────────────────────

#[test]
fn deref_of_borrow_round_trips() {
    assert_eq!(
        run_both("def f(x: Int): Int := *(&x) \n def main(): Int := f(5)"),
        Expression::Int(5)
    );
}

#[test]
fn deref_of_a_let_bound_borrow() {
    assert_eq!(
        run_both("def main(): Int := { let x = 41; let r = &x; *r + 1 }"),
        Expression::Int(42)
    );
}

#[test]
fn deref_of_a_mut_borrow_reads() {
    assert_eq!(
        run_both("def f(mut x: Int): Int := *(&mut x) \n def main(): Int := f(9)"),
        Expression::Int(9)
    );
}

#[test]
fn deref_of_a_borrowed_bool() {
    typecheck("def not(b: &Bool): Bool := if *b then false else true \n def main(): Int := 0");
}

// ── ill-typed / unsound derefs are rejected
// ───────────────────────────────────

#[test]
fn deref_of_a_non_reference_is_rejected() {
    typecheck_fails("def main(): Int := *5");
}

#[test]
fn deref_moving_a_non_copy_value_out_of_a_borrow_is_rejected() {
    // `*e` of a non-`Copy` enum would move it out of the shared borrow.
    typecheck_fails("type E = A | B \n def consume(e: &E): E := *e \n def main(): Int := 0");
}
