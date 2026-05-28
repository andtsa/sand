//! TypedHIR type-checking tests for the Sand compiler
//! 
//! Tests cover the type-checking phase, including:
//! - Literal type checking (int, bool, unit)
//! - Arithmetic and comparison operators
//! - Boolean operators
//! - Control flow (if/while)
//! - Functions and recursion
//! - Variable binding and assignment
//! - Error cases for type mismatches


// ── happy-path type checking ──────────────────────────────────────────

use crate::common::{typecheck, typecheck_fails};

#[test]
fn typecheck_int_literal() {
    typecheck("def main(): Int := 0");
}

#[test]
fn typecheck_bool_literal() {
    typecheck("def main(): Bool := true");
}

#[test]
fn typecheck_unit_literal() {
    typecheck("def main(): Unit := { }");
}

#[test]
fn typecheck_arithmetic() {
    typecheck("def main(): Int := 1 + 2 * 3 - 4 / 2");
}

#[test]
fn typecheck_comparison_returns_bool() {
    typecheck("def main(): Bool := 1 < 2");
}

#[test]
fn typecheck_equality_returns_bool() {
    typecheck("def main(): Bool := 1 == 1");
}

#[test]
fn typecheck_boolean_and() {
    typecheck("def main(): Bool := true & false");
}

#[test]
fn typecheck_boolean_or() {
    typecheck("def main(): Bool := true | false");
}

#[test]
fn typecheck_boolean_not() {
    typecheck("def main(): Bool := !false");
}

#[test]
fn typecheck_unary_negation() {
    typecheck("def main(): Int := -(3)");
}

#[test]
fn typecheck_if_branches_same_type() {
    typecheck("def main(): Int := if true then 1 else 2");
}

#[test]
fn typecheck_while_loop() {
    typecheck(
        "def main(): Unit := {
            let i: Int = 0;
            while i < 3 do {
                i = i + 1;
            };
        }",
    );
}

#[test]
fn typecheck_let_binding_and_return() {
    typecheck(
        "def main(): Int := {
            let x: Int = 10;
            x
        }",
    );
}

#[test]
fn typecheck_function_call_correct_arg_types() {
    typecheck(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(1, 2)",
    );
}

#[test]
fn typecheck_recursive_function() {
    typecheck(
        "def fact(n: Int): Int :=
            if n == 0 then 1 else n * fact(n - 1)
         def main(): Int := fact(5)",
    );
}

#[test]
fn typecheck_block_with_assignment() {
    typecheck(
        "def main(): Int := {
            let x: Int = 5;
            x = x + 1;
            x
        }",
    );
}

#[test]
fn typecheck_nested_blocks() {
    typecheck(
        "def main(): Int := {
            let a: Int = {
                let b: Int = 3;
                b * 2
            };
            a + 1
        }",
    );
}

#[test]
fn typecheck_intrinsic_println() {
    typecheck(
        "def main(): Unit := {
            println(42);
        }",
    );
}

#[test]
fn typecheck_power_operator() {
    typecheck("def main(): Int := 2 ^ 10");
}

// ── type-error cases ──────────────────────────────────────────────────

#[test]
fn typecheck_fails_wrong_return_type() {
    // Function declares Int return, body produces Bool.
    typecheck_fails("def main(): Int := true");
}

#[test]
fn typecheck_fails_wrong_return_type_bool_for_int() {
    typecheck_fails("def main(): Bool := 42");
}

#[test]
fn typecheck_fails_add_bool_and_int() {
    typecheck_fails("def main(): Int := true + 1");
}

#[test]
fn typecheck_fails_negate_int() {
    // `!` is boolean NOT; applying it to Int should fail.
    typecheck_fails("def main(): Bool := !1");
}

#[test]
fn typecheck_fails_arithmetic_negate_bool() {
    // Unary minus on Bool.
    typecheck_fails("def main(): Int := -(true)");
}

#[test]
fn typecheck_fails_if_condition_not_bool() {
    typecheck_fails("def main(): Int := if 1 then 2 else 3");
}

#[test]
fn typecheck_fails_if_branches_different_types() {
    typecheck_fails("def main(): Int := if true then 1 else false");
}

#[test]
fn typecheck_fails_while_condition_not_bool() {
    typecheck_fails(
        "def main(): Unit := {
            while 1 do { };
        }",
    );
}

#[test]
fn typecheck_fails_declare_wrong_type() {
    typecheck_fails(
        "def main(): Int := {
            let x: Bool = 42;
            0
        }",
    );
}

#[test]
fn typecheck_fails_assign_wrong_type() {
    typecheck_fails(
        "def main(): Int := {
            let x: Int = 0;
            x = true;
            x
        }",
    );
}

#[test]
fn typecheck_fails_wrong_argument_type() {
    typecheck_fails(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(true, 2)",
    );
}

#[test]
fn typecheck_fails_wrong_argument_count_too_few() {
    typecheck_fails(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(1)",
    );
}

#[test]
fn typecheck_fails_wrong_argument_count_too_many() {
    typecheck_fails(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(1, 2, 3)",
    );
}

#[test]
fn typecheck_fails_comparison_mixed_types() {
    // `<` only accepts Int operands.
    typecheck_fails("def main(): Bool := true < false");
}
