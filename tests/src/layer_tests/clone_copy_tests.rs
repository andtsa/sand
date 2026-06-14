//! Step 14 — `Clone` / `Copy` integration (Calculus §7.4).
//!
//! `Copy` (a marker requiring `Clone`) drives implicit duplication: the
//! ownership pass treats a `Copy` value as not-consumed on use, so it can be
//! used multiple times. A non-`Copy` value is moved; using it twice is an
//! error, and `clone(&x)` produces a fresh owned value to keep a second copy.
//! `Copy` is structural (every field must be `Copy`) and a `where T : Copy`
//! bound makes a generic parameter copyable.

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

// ── primitives are Copy (builtin)
// ─────────────────────────────────────────────

#[test]
fn an_int_is_used_twice_without_clone() {
    assert_eq!(
        run_both("def use2(n: Int): Int := n + n \n def main(): Int := use2(21)"),
        Expression::Int(42)
    );
}

// ── user `Copy` types
// ─────────────────────────────────────────────────────────

#[test]
fn a_copy_enum_is_used_twice() {
    assert_eq!(
        run_both(
            "type Color = Red | Green \n \
             impl Clone for Color { def clone(x: &Color): Color := *x } \n \
             impl Copy for Color { } \n \
             def use2(c: Color): Bool := c == c \n \
             def main(): Int := if use2(Color#Red) then 1 else 0"
        ),
        Expression::Int(1)
    );
}

#[test]
fn a_copy_tuple_is_used_twice() {
    assert_eq!(
        run_both(
            "def use2(p: (Int, Int)): Bool := p == p \n \
             def main(): Int := if use2((1, 2)) then 5 else 0"
        ),
        Expression::Int(5)
    );
}

#[test]
fn a_non_copy_value_used_twice_is_rejected() {
    typecheck_fails(
        "type Box = B(Int) \n \
         def use2(b: Box): Bool := b == b \n \
         def main(): Int := 0",
    );
}

// ── `clone` produces a fresh owned value
// ──────────────────────────────────────

#[test]
fn clone_returns_a_fresh_value() {
    assert_eq!(
        run_both(
            "type Color = Red | Green \n \
             impl Clone for Color { def clone(x: &Color): Color := *x } \n \
             impl Copy for Color { } \n \
             def main(): Int := { let c = Color#Red; let d = clone(&c); if c == d then 1 else 0 }"
        ),
        Expression::Int(1)
    );
}

// ── `Copy` is structural
// ──────────────────────────────────────────────────────

#[test]
fn copy_for_an_all_copy_enum_is_accepted() {
    typecheck(
        "type Color = Red | Green \n \
         impl Clone for Color { def clone(x: &Color): Color := *x } \n \
         impl Copy for Color { } \n \
         def main(): Int := 0",
    );
}

#[test]
fn copy_for_an_enum_with_a_non_copy_field_is_rejected() {
    typecheck_fails(
        "type Box = B(Int) \n \
         type Wrap = W(Box) \n \
         impl Clone for Wrap { def clone(x: &Wrap): Wrap := *x } \n \
         impl Copy for Wrap { } \n \
         def main(): Int := 0",
    );
}

#[test]
fn copy_for_a_generic_type_is_rejected() {
    typecheck_fails(
        "type Holder<T> = H(T) \n \
         impl Clone for Holder { def clone(x: &Holder): Holder := *x } \n \
         impl Copy for Holder { } \n \
         def main(): Int := 0",
    );
}

#[test]
fn copy_without_clone_is_rejected() {
    // `Copy requires Clone`, so the superclass instance is mandatory.
    typecheck_fails(
        "type Color = Red | Green \n \
         impl Copy for Color { } \n \
         def main(): Int := 0",
    );
}

// ── generic `where T : Copy`
// ──────────────────────────────────────────────────

#[test]
fn where_t_copy_allows_using_a_generic_value_twice() {
    assert_eq!(
        run_both(
            "typeclass ToInt<T> { def to_int(x: T): Int } \n \
             impl ToInt for Int { def to_int(x: Int): Int := x } \n \
             def doubled<T>(x: T): Int where T : ToInt, T : Copy := to_int(x) + to_int(x) \n \
             def main(): Int := doubled(5)"
        ),
        Expression::Int(10)
    );
}

#[test]
fn using_a_generic_value_twice_without_copy_is_rejected() {
    typecheck_fails(
        "typeclass ToInt<T> { def to_int(x: T): Int } \n \
         impl ToInt for Int { def to_int(x: Int): Int := x } \n \
         def doubled<T>(x: T): Int where T : ToInt := to_int(x) + to_int(x) \n \
         def main(): Int := 0",
    );
}
