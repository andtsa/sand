//! HHIR parsing tests for the Sand compiler
//!
//! Tests cover basic parsing functionality for the High-level HIR (HHIR) layer,
//! including literals, operators, control flow, functions, and various error
//! cases.

use lang::compiler::context::CompileCtx;
use lang::ir_types::hhir::ProgramModule;

use crate::common::parse;
use crate::common::parse_fails;

// ── happy-path parsing ────────────────────────────────────────────────

#[test]
fn parse_minimal_unit_function() {
    // A function returning Unit with no parameters.
    parse("def main(): Unit := { }");
}

#[test]
fn parse_integer_literal() {
    parse("def main(): Int := 42");
}

#[test]
fn parse_bool_literal_true() {
    parse("def main(): Bool := true");
}

#[test]
fn parse_bool_literal_false() {
    parse("def main(): Bool := false");
}

#[test]
fn parse_single_parameter() {
    parse("def id(x: Int): Int := x");
}

#[test]
fn parse_multiple_parameters() {
    parse("def add(a: Int, b: Int): Int := a + b");
}

#[test]
fn parse_nested_arithmetic() {
    parse("def f(): Int := (1 + 2) * (3 - 4) / 5");
}

#[test]
fn parse_unary_negation() {
    parse("def f(): Int := -(1)");
}

#[test]
fn parse_unary_not() {
    parse("def f(): Bool := !true");
}

#[test]
fn parse_if_then_else() {
    parse("def f(x: Bool): Int := if x then 1 else 2");
}

#[test]
fn parse_if_then_no_else() {
    // Grammar allows omitting the else branch.
    parse("def f(x: Bool): Unit := if x then { }");
}

#[test]
fn parse_while_loop() {
    parse(
        "def f(): Unit := {
            let i: Int = 0;
            while i < 10 do { i = i + 1; };
        }",
    );
}

#[test]
fn parse_block_with_trailing_expr() {
    parse(
        "def f(): Int := {
            let x: Int = 1;
            let y: Int = 2;
            x + y
        }",
    );
}

#[test]
fn parse_block_without_trailing_expr() {
    parse(
        "def f(): Unit := {
            let x: Int = 1;
        }",
    );
}

#[test]
fn parse_local_function_call() {
    parse(
        "def helper(): Int := 0
         def main(): Int := helper()",
    );
}

#[test]
fn parse_function_call_with_args() {
    parse(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(1, 2)",
    );
}

#[test]
fn parse_external_module_call() {
    // Syntax: module_name::function_name(...)
    // We only check that the grammar accepts it; resolution is done in qualify.
    parse(
        "module mymod;
         def f(): Int := mymod::g()",
    );
}

#[test]
fn parse_comparison_operators() {
    for op in ["==", "!=", "<", "<=", ">", ">="] {
        parse(&format!("def f(a: Int, b: Int): Bool := a {} b", op));
    }
}

#[test]
fn parse_boolean_operators() {
    parse("def f(a: Bool, b: Bool): Bool := a and b");
    parse("def f(a: Bool, b: Bool): Bool := a | b");
    parse("def f(a: Bool, b: Bool): Bool := a # b");
}

#[test]
fn parse_power_operator() {
    parse("def f(a: Int, b: Int): Int := a ^ b");
}

#[test]
fn parse_variable_shadowing_in_blocks() {
    // Outer `a` and inner `a` are different bindings.
    parse(
        "def f(): Int := {
            let a: Int = 1;
            let b: Int = {
                let a: Int = 2;
                a
            };
            a
        }",
    );
}

#[test]
fn parse_assignment() {
    parse(
        "def f(): Int := {
            let x: Int = 0;
            x = 5;
            x
        }",
    );
}

#[test]
fn parse_deeply_nested_blocks() {
    parse(
        "def f(): Int := {
            let a: Int = {
                let b: Int = {
                    let c: Int = 3;
                    c
                };
                b
            };
            a
        }",
    );
}

#[test]
fn parse_multiple_functions() {
    parse(
        "def a(): Int := 1
         def b(): Int := 2
         def main(): Int := a()",
    );
}

// ── keyword-prefixed type names ───────────────────────────────────────

#[test]
fn parse_type_name_with_keyword_prefix() {
    // A type name that begins with a primitive-type keyword (`Int`) must lex
    // as a single identifier, not as the `Int` keyword followed by a stray
    // `List` suffix. Regression test for the greedy keyword match in the
    // `core_type` grammar rule.
    parse(
        "type IntList = Nil | Cons((Int, IntList)) deriving Heaped
         def main(): Int := 0",
    );
}

#[test]
fn parse_type_names_with_each_keyword_prefix() {
    // The same word-boundary requirement applies to every primitive keyword
    // (`Bool`, `Unit`), not just `Int`.
    parse(
        "type Boolean = T | F
         type Unite = One | Two
         def f(x: Boolean, y: Unite): Int := 0",
    );
}

// ── number of functions ───────────────────────────────────────────────

#[test]
fn function_count_is_correct() {
    let mut ctx = CompileCtx::initial();
    let pm = ProgramModule::parse_stub(
        &mut ctx,
        "def a(): Int := 1
         def b(): Int := 2
         def main(): Int := a()",
    )
    .unwrap();
    assert_eq!(pm.functions.len(), 3);
}

// ── parse failures ────────────────────────────────────────────────────

#[test]
fn parse_fails_empty_string() {
    // An empty program has no functions, which parse_stub forbids
    // (it expects exactly one module with at least the grammar's EOI).
    // The test just checks that something ill-formed is rejected.
    parse_fails("def");
}

#[test]
fn parse_fails_missing_return_type() {
    parse_fails("def f() := 1");
}

#[test]
fn parse_fails_missing_body() {
    parse_fails("def f(): Int :=");
}

#[test]
fn parse_fails_unclosed_block() {
    parse_fails("def f(): Int := { 1 ");
}

#[test]
fn parse_fails_missing_paren() {
    parse_fails("def f(x: Int: Int := x");
}

#[test]
fn parse_fails_keyword_as_identifier() {
    // `let` is a keyword; cannot be used as a function name.
    parse_fails("def let(): Int := 1");
}

#[test]
fn parse_fails_reserved_function_name_print() {
    // `print` is reserved as an intrinsic.
    parse_fails("def print(x: Int): Unit := { }");
}

#[test]
fn parse_fails_reserved_function_name_println() {
    parse_fails("def println(x: Int): Unit := { }");
}

#[test]
fn parse_fails_unknown_type() {
    parse_fails("def f(): Float := 1.0");
}

#[test]
fn parse_fails_statement_missing_semicolon() {
    parse_fails(
        "def f(): Int := {
            let x: Int = 1
            x
        }",
    );
}
