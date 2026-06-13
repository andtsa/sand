//! Step 4 the `{Owned, Never}` kind system and divergence.
//!
//! Every ordinary value is `Owned`; a statically-infinite loop (`while true`,
//! the language has no `break`) is `Never`, the uninhabited kind, and so
//! inhabits any type (Calculus §6.1, `Never <: k`).

use lang::ir_types::typed_hir::Expression;
use lang::lang::types::Kind;

use crate::common::run_hir;
use crate::common::run_mir_as_expr;
use crate::common::typecheck;
use crate::common::typecheck_fails;

/// Run through both interpreters (executing the monomorphised, lowered program)
/// and assert they agree.
fn run_both(src: &str) -> Expression<'static> {
    let hir = run_hir(src);
    let mir = run_mir_as_expr(src);
    assert_eq!(hir, mir, "HIR and MIR disagree for:\n  {src}");
    hir
}

// ── kind lattice (Calculus §1.2 / §1.4)
// ───────────────────────────────────────

#[test]
fn subkinding() {
    assert!(Kind::Never.is_subkind(Kind::Owned)); // Never is the bottom
    assert!(Kind::Owned.is_subkind(Kind::Owned)); // reflexive
    assert!(Kind::Never.is_subkind(Kind::Never));
    assert!(!Kind::Owned.is_subkind(Kind::Never)); // Owned is not below Never
}

#[test]
fn kind_join() {
    assert_eq!(Kind::Owned.join(Kind::Owned), Kind::Owned);
    assert_eq!(Kind::Owned.join(Kind::Never), Kind::Owned); // Never is identity
    assert_eq!(Kind::Never.join(Kind::Owned), Kind::Owned);
    assert_eq!(Kind::Never.join(Kind::Never), Kind::Never);
}

// ── divergence type-checks against any type
// ───────────────────────────────────

#[test]
fn infinite_loop_inhabits_int() {
    // `while true` is `Never`, so it satisfies the `Int` return type.
    typecheck("def f(): Int := while true do {} \n def main(): Int := 0");
}

#[test]
fn infinite_loop_inhabits_bool() {
    typecheck("def f(): Bool := while true do {} \n def main(): Int := 0");
}

#[test]
fn divergent_if_branch_takes_other_type() {
    // the diverging `else` does not constrain the result type.
    typecheck("def f(c: Bool): Int := if c then 5 else (while true do {}) \n def main(): Int := 0");
}

#[test]
fn both_branches_diverge_inhabits_any_type() {
    typecheck(
        "def f(c: Bool): Int := if c then (while true do {}) else (while true do {}) \n \
         def main(): Int := 0",
    );
}

#[test]
fn divergent_match_arm_takes_other_type() {
    typecheck(
        "type T = A | B \n \
         def f(x: T): Int := match x { T#A => 1, T#B => (while true do {}) } \n \
         def main(): Int := 0",
    );
}

#[test]
fn non_diverging_while_is_still_unit() {
    // a `while` with a non-literal-true condition terminates: kind `Owned`,
    // type `Unit`, so it cannot stand in for `Int`.
    typecheck_fails("def f(c: Bool): Int := while c do {} \n def main(): Int := 0");
}

#[test]
fn non_diverging_while_as_unit_ok() {
    typecheck("def f(c: Bool): Unit := while c do {} \n def main(): Int := 0");
}

// ── divergence compiles and runs (guarded so the loop is never executed)
// ──────

#[test]
fn guarded_divergence_runs() {
    // the diverging branch compiles but is never taken at runtime.
    assert_eq!(
        run_both("def main(): Int := if true then 5 else (while true do {})"),
        Expression::Int(5)
    );
}

#[test]
fn diverging_function_compiles_and_guarded_call_runs() {
    // `diverge` is a function whose whole body diverges; it must compile even
    // though calling it would never return. Here the call is on a dead path.
    let src = "def diverge(): Int := while true do {} \n \
        def main(): Int := if true then 9 else diverge()";
    assert_eq!(run_both(src), Expression::Int(9));
}
