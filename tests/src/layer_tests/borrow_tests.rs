//! Step 7 — shared borrows: the `Borrowed` kind, `&'r T` reference types, and
//! `&e` borrow expressions (Calculus §1, §2.3, §3.2, §6.2).
//!
//! Borrows are immutable and have no distinct runtime representation yet, so
//! monomorphisation erases `&'r T` to `T` and borrows lower transparently.
//! The block-region escape check is deferred to Step 8.

use lang::ir_types::typed_hir::Expression;
use lang::lang::types::Kind;
use lang::lang::types::Region;

use crate::common::parse;
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

// ── kind lattice: Owned <: Borrowed
// ───────────────────────────────────────────

#[test]
fn owned_is_subkind_of_borrowed() {
    let r = Region::Static;
    assert!(Kind::Owned.is_subkind(Kind::Borrowed(r)));
    assert!(Kind::Never.is_subkind(Kind::Borrowed(r)));
    assert!(!Kind::Borrowed(r).is_subkind(Kind::Owned));
}

// ── reference types and borrow expressions parse and type-check
// ───────────────

#[test]
fn reference_type_parameter_parses() {
    parse("def f(r: &Int): Int := 0");
}

#[test]
fn explicit_lifetime_reference_parses() {
    parse("def f<'r>(r: &'r Int): Int := 0");
}

#[test]
fn borrow_expression_type_checks() {
    typecheck("def f(x: Int): Int := { let r = &x; x } \n def main(): Int := 0");
}

#[test]
fn passing_a_borrow_to_a_reference_parameter_type_checks() {
    typecheck("def takes(r: &Int): Int := 0 \n def main(): Int := takes(&5)");
}

#[test]
fn borrowing_an_int_then_a_bool() {
    typecheck("def f(b: Bool): Int := { let r = &b; 0 } \n def main(): Int := 0");
}

// ── borrows do not consume their referent (Var-Borrow)
// ────────────────────────

#[test]
fn borrowing_does_not_move_a_non_copy_value() {
    // `&e` borrows `e` without moving it, so `e` may still be consumed later.
    typecheck(
        "type E = A | B \n \
         def f(e: E): Int := { let r = &e; match e { E#A => 1, E#B => 2 } } \n \
         def main(): Int := 0",
    );
}

#[test]
fn double_move_without_borrow_still_fails() {
    // the control case: moving a non-copy value twice is still an ownership
    // error (the borrow above is what makes the difference).
    typecheck_fails(
        "type E = A | B \n \
         def f(e: E): Int := { let a = e; let b = e; 0 } \n \
         def main(): Int := 0",
    );
}

#[test]
fn multiple_borrows_of_the_same_value() {
    typecheck(
        "type E = A | B \n \
         def f(e: E): Int := { let a = &e; let b = &e; match e { E#A => 1, E#B => 2 } } \n \
         def main(): Int := 0",
    );
}

// ── borrows compile and run (erased transparently)
// ────────────────────────────

#[test]
fn borrow_program_runs() {
    assert_eq!(
        run_both("def takes(r: &Int): Int := 0 \n def main(): Int := takes(&7)"),
        Expression::Int(0)
    );
}

#[test]
fn borrowed_value_still_usable_at_runtime() {
    // borrow `x`, then return `x` — the borrow is transparent, so the value is
    // unaffected.
    assert_eq!(
        run_both("def f(x: Int): Int := { let r = &x; x } \n def main(): Int := f(42)"),
        Expression::Int(42)
    );
}

// ── `let &x` borrow binding (desugars to `let x = &e`)
// ────────────────────────

#[test]
fn let_borrow_binding_type_checks() {
    typecheck("def f(x: Int): Int := { let &r = x; x } \n def main(): Int := 0");
}

#[test]
fn let_borrow_binding_does_not_consume() {
    // `let &r = e` borrows `e`, so a non-copy `e` stays usable afterwards.
    typecheck(
        "type E = A | B \n \
         def f(e: E): Int := { let &r = e; match e { E#A => 1, E#B => 2 } } \n \
         def main(): Int := 0",
    );
}

#[test]
fn let_borrow_binding_runs() {
    assert_eq!(
        run_both("def f(x: Int): Int := { let &r = x; x } \n def main(): Int := f(9)"),
        Expression::Int(9)
    );
}
