//! Interpreter tests
//!
//! Each test compiles a program and runs it through **both** the typed-HIR and MIR
//! interpreters, asserting:
//!   1. Both produce the expected result value.
//!   2. Both agree with each other (caught by `run_both`).
//!
//! MIR-specific failure cases (division by zero, overflow) live in
//! `robustness_tests/` since they require checking `MirInterpError`.
//! MIR structural invariants (block counts, terminators, locals) live in
//! `robustness_tests/mir_structure`.

use sand::interpreter::mir::MirValue;
use sand::ir_types::typed_hir::Expression;
use crate::common::{run_hir, run_mir};

/// Run `src` through both interpreters, assert they agree, and return the result.
fn run_both(src: &str) -> Expression {
    let hir = run_hir(src);
    let mir = match run_mir(src) {
        MirValue::Int(i) => Expression::Int(i),
        MirValue::Bool(b) => Expression::Bool(b),
        MirValue::Unit => Expression::Unit,
    };
    assert_eq!(hir, mir, "HIR and MIR interpreters disagree for:\n  {src}");
    hir
}

// ── literals ─────────────────────────────────────────────────────────────────

#[test]
fn interpret_int_literal() {
    assert_eq!(run_both("def main(): Int := 7"), Expression::Int(7));
}

#[test]
fn interpret_bool_true() {
    assert_eq!(run_both("def main(): Bool := true"), Expression::Bool(true));
}

#[test]
fn interpret_bool_false() {
    assert_eq!(run_both("def main(): Bool := false"), Expression::Bool(false));
}

#[test]
fn interpret_unit() {
    assert_eq!(run_both("def main(): Unit := { }"), Expression::Unit);
}

// ── arithmetic ────────────────────────────────────────────────────────────────

#[test]
fn interpret_addition() {
    assert_eq!(run_both("def main(): Int := 3 + 4"), Expression::Int(7));
}

#[test]
fn interpret_subtraction() {
    assert_eq!(run_both("def main(): Int := 10 - 3"), Expression::Int(7));
}

#[test]
fn interpret_multiplication() {
    assert_eq!(run_both("def main(): Int := 6 * 7"), Expression::Int(42));
}

#[test]
fn interpret_division() {
    assert_eq!(run_both("def main(): Int := 20 / 4"), Expression::Int(5));
}

#[test]
fn interpret_power() {
    assert_eq!(run_both("def main(): Int := 2 ^ 8"), Expression::Int(256));
}

#[test]
fn interpret_unary_negation() {
    assert_eq!(run_both("def main(): Int := -(5)"), Expression::Int(-5));
}

#[test]
fn interpret_operator_precedence() {
    assert_eq!(run_both("def main(): Int := 2 + 3 * 4"), Expression::Int(14));
}

#[test]
fn interpret_parenthesised_expr() {
    assert_eq!(run_both("def main(): Int := (2 + 3) * 4"), Expression::Int(20));
}

// ── boolean operations ────────────────────────────────────────────────────────

#[test]
fn interpret_bool_and_true() {
    assert_eq!(run_both("def main(): Bool := true & true"), Expression::Bool(true));
}

#[test]
fn interpret_bool_and_false() {
    assert_eq!(run_both("def main(): Bool := true & false"), Expression::Bool(false));
}

#[test]
fn interpret_bool_or() {
    assert_eq!(run_both("def main(): Bool := false | true"), Expression::Bool(true));
}

#[test]
fn interpret_bool_not() {
    assert_eq!(run_both("def main(): Bool := !false"), Expression::Bool(true));
}

// ── comparisons ───────────────────────────────────────────────────────────────

#[test]
fn interpret_eq_true() {
    assert_eq!(run_both("def main(): Bool := 5 == 5"), Expression::Bool(true));
}

#[test]
fn interpret_eq_false() {
    assert_eq!(run_both("def main(): Bool := 5 == 6"), Expression::Bool(false));
}

#[test]
fn interpret_ne() {
    assert_eq!(run_both("def main(): Bool := 5 != 6"), Expression::Bool(true));
}

#[test]
fn interpret_lt() {
    assert_eq!(run_both("def main(): Bool := 3 < 5"), Expression::Bool(true));
}

#[test]
fn interpret_gt() {
    assert_eq!(run_both("def main(): Bool := 5 > 3"), Expression::Bool(true));
}

#[test]
fn interpret_le_equal() {
    assert_eq!(run_both("def main(): Bool := 5 <= 5"), Expression::Bool(true));
    assert_eq!(run_both("def main(): Bool := 5 ≤ 5"), Expression::Bool(true));
}

#[test]
fn interpret_ge_greater() {
    assert_eq!(run_both("def main(): Bool := 6 >= 5"), Expression::Bool(true));
    assert_eq!(run_both("def main(): Bool := 6 ≥ 5"), Expression::Bool(true));
}

// ── if / else ─────────────────────────────────────────────────────────────────

#[test]
fn interpret_if_takes_true_branch() {
    assert_eq!(run_both("def main(): Int := if true then 1 else 2"), Expression::Int(1));
}

#[test]
fn interpret_if_takes_false_branch() {
    assert_eq!(run_both("def main(): Int := if false then 1 else 2"), Expression::Int(2));
}

#[test]
fn interpret_nested_if() {
    let src = "def main(): Int :=
        if true then
            if false then 10 else 20
        else 30";
    assert_eq!(run_both(src), Expression::Int(20));
}

// ── while ─────────────────────────────────────────────────────────────────────

#[test]
fn interpret_while_not_entered_when_false() {
    let src = "def main(): Int := {
        let x: Int = 0;
        while false do { x = x + 1; };
        x
    }";
    assert_eq!(run_both(src), Expression::Int(0));
}

#[test]
fn interpret_while_runs_correct_iterations() {
    let src = "def main(): Int := {
        let x: Int = 0;
        while x < 5 do { x = x + 1; };
        x
    }";
    assert_eq!(run_both(src), Expression::Int(5));
}

#[test]
fn interpret_while_accumulator() {
    // Sum 1..=10 = 55
    let src = "def main(): Int := {
        let i: Int = 1;
        let s: Int = 0;
        while i <= 10 do {
            s = s + i;
            i = i + 1;
        };
        s
    }";
    assert_eq!(run_both(src), Expression::Int(55));
}

// ── blocks and let-bindings ───────────────────────────────────────────────────

#[test]
fn interpret_block_trailing_expression() {
    let src = "def main(): Int := {
        let a: Int = 3;
        let b: Int = 4;
        a + b
    }";
    assert_eq!(run_both(src), Expression::Int(7));
}

#[test]
fn interpret_nested_block_shadowing() {
    let src = "def main(): Int := {
        let a: Int = 1;
        let b: Int = {
            let a: Int = 100;
            a + 1
        };
        a + b
    }";
    // outer a=1, inner block returns 101, b=101, result = 1+101 = 102
    assert_eq!(run_both(src), Expression::Int(102));
}

#[test]
fn interpret_assignment_updates_value() {
    let src = "def main(): Int := {
        let x: Int = 1;
        x = 42;
        x
    }";
    assert_eq!(run_both(src), Expression::Int(42));
}

// ── function calls ────────────────────────────────────────────────────────────

#[test]
fn interpret_function_call_no_args() {
    let src = "def answer(): Int := 42
               def main(): Int := answer()";
    assert_eq!(run_both(src), Expression::Int(42));
}

#[test]
fn interpret_function_call_with_args() {
    let src = "def add(a: Int, b: Int): Int := a + b
               def main(): Int := add(10, 32)";
    assert_eq!(run_both(src), Expression::Int(42));
}

#[test]
fn interpret_fibonacci_10() {
    let src = "
        def fib(n: Int): Int :=
            if n <= 1 then n
            else fib(n - 1) + fib(n - 2)
        def main(): Int := fib(10)";
    assert_eq!(run_both(src), Expression::Int(55));
}

#[test]
fn interpret_factorial_9() {
    let src = "
        def fact(n: Int): Int :=
            if n == 0 then 1 else n * fact(n - 1)
        def main(): Int := fact(9)";
    assert_eq!(run_both(src), Expression::Int(362880));
}

#[test]
fn interpret_gcd() {
    let src = "
        def gcd(a: Int, b: Int): Int :=
            if b == 0 then a else gcd(b, a - (a / b) * b)
        def main(): Int := gcd(48, 18)";
    assert_eq!(run_both(src), Expression::Int(6));
}

#[test]
fn interpret_higher_order_via_explicit_call() {
    let src = "
        def double(x: Int): Int := x * 2
        def quad(x: Int): Int := double(double(x))
        def main(): Int := quad(5)";
    assert_eq!(run_both(src), Expression::Int(20));
}

#[test]
fn interpret_mutual_recursion() {
    let src = "
        def is_odd(n: Int): Bool :=
            if n == 0 then false else is_even(n - 1)
        def is_even(n: Int): Bool :=
            if n == 0 then true else is_odd(n - 1)
        def main(): Bool := is_even(10)";
    assert_eq!(run_both(src), Expression::Bool(true));
}

// ── edge cases ────────────────────────────────────────────────────────────────

#[test]
fn interpret_zero_power_is_one() {
    assert_eq!(run_both("def main(): Int := 99 ^ 0"), Expression::Int(1));
}

#[test]
fn interpret_negative_zero_is_zero() {
    assert_eq!(run_both("def main(): Int := -(0)"), Expression::Int(0));
}

#[test]
fn interpret_chained_comparisons_via_bool_ops() {
    assert_eq!(
        run_both("def main(): Bool := (1 < 2) & (3 > 2)"),
        Expression::Bool(true)
    );
}

#[test]
fn interpret_bool_equality() {
    assert_eq!(run_both("def main(): Bool := true == true"), Expression::Bool(true));
    assert_eq!(run_both("def main(): Bool := true == false"), Expression::Bool(false));
}

#[test]
fn interpret_block_with_only_statements_returns_unit() {
    assert_eq!(
        run_both("def main(): Unit := { let x: Int = 1; }"),
        Expression::Unit
    );
}
