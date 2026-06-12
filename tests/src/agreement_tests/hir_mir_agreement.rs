//! HIR and MIR interpreter agreement tests
//!
//! These tests verify that both the typed-HIR and MIR interpreters produce
//! identical results for the same programs. This catches divergence between
//! the two execution paths without having to hard-code expected values.

use lang::compile_hir;
use lang::compiler::context::CompileCtx;
use lang::compiler::structure::Map;
use lang::ir_types::mir::MirProgram;

use crate::common::mir_value_to_expr;

/// Helper: Run both the typed-HIR and MIR interpreters and assert they agree.
fn assert_hir_mir_agree(src: &str) {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, src)]);
    let ast = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed:\n  {e}"));

    let hir_result = ast
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("HIR interpret failed:\n  {e}"));

    let mir = MirProgram::from_typed_program(&ast, &ctx);
    let mir_result = mir
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("MIR interpret failed:\n  {e}"));

    // convert MirValue → Expression for comparison
    let mir_as_expr = mir_value_to_expr(mir_result, &ctx);

    assert_eq!(
        hir_result, mir_as_expr,
        "HIR and MIR interpreters disagreed on:\n  {src}"
    );
}

#[test]
fn agree_int_literal() {
    assert_hir_mir_agree("def main(): Int := 42");
}

#[test]
fn agree_bool_literal() {
    assert_hir_mir_agree("def main(): Bool := true");
}

#[test]
fn agree_unit_literal() {
    assert_hir_mir_agree("def main(): Unit := { }");
}

#[test]
fn agree_arithmetic_expression() {
    assert_hir_mir_agree("def main(): Int := (3 + 4) * 2 - 1");
}

#[test]
fn agree_chained_booleans() {
    assert_hir_mir_agree("def main(): Bool := (1 < 2) and (3 > 2) | false");
}

#[test]
fn agree_if_true_branch() {
    assert_hir_mir_agree("def main(): Int := if 2 > 1 then 100 else 200");
}

#[test]
fn agree_if_false_branch() {
    assert_hir_mir_agree("def main(): Int := if 1 > 2 then 100 else 200");
}

#[test]
fn agree_nested_if() {
    assert_hir_mir_agree(
        "def main(): Int :=
            if true then if false then 1 else 2 else 3",
    );
}

#[test]
fn agree_while_sum() {
    assert_hir_mir_agree(
        "def main(): Int := {
            let mut i: Int = 1;
            let mut s: Int = 0;
            while i <= 10 do {
                s = s + i;
                i = i + 1;
            };
            s
        }",
    );
}

#[test]
fn agree_nested_blocks() {
    assert_hir_mir_agree(
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
fn agree_variable_assignment() {
    assert_hir_mir_agree(
        "def main(): Int := {
            let mut x: Int = 10;
            x = x + 5;
            x
        }",
    );
}

#[test]
fn agree_recursive_factorial() {
    assert_hir_mir_agree(
        "def fact(n: Int): Int :=
            if n == 0 then 1 else n * fact(n - 1)
         def main(): Int := fact(7)",
    );
}

#[test]
fn agree_fibonacci() {
    assert_hir_mir_agree(
        "def fib(n: Int): Int :=
            if n <= 1 then n else fib(n - 1) + fib(n - 2)
         def main(): Int := fib(10)",
    );
}

#[test]
fn agree_gcd() {
    assert_hir_mir_agree(
        "def gcd(a: Int, b: Int): Int :=
            if b == 0 then a else gcd(b, a - (a / b) * b)
         def main(): Int := gcd(48, 18)",
    );
}

#[test]
fn agree_mutual_recursion() {
    assert_hir_mir_agree(
        "def is_odd(n: Int): Bool :=
            if n == 0 then false else is_even(n - 1)
         def is_even(n: Int): Bool :=
            if n == 0 then true else is_odd(n - 1)
         def main(): Bool := is_even(12)",
    );
}

#[test]
fn agree_power_operator() {
    assert_hir_mir_agree("def main(): Int := 2 ^ 10");
}

#[test]
fn agree_complex_boolean_expression() {
    assert_hir_mir_agree("def main(): Bool := !false and (1 == 1) | (2 != 3)");
}

#[test]
fn agree_multiple_function_calls() {
    assert_hir_mir_agree(
        "def inc(x: Int): Int := x + 1
         def double(x: Int): Int := x * 2
         def main(): Int := double(inc(inc(5)))",
    );
}

#[test]
fn xor_operator_agrees() {
    assert_hir_mir_agree("def main(): Bool := true ¡ false");
    assert_hir_mir_agree("def main(): Bool := true ¡ true");
    assert_hir_mir_agree("def main(): Bool := false ¡ false");
}

#[test]
fn not_equal_operator_agrees() {
    assert_hir_mir_agree("def main(): Bool := 1 != 2");
    assert_hir_mir_agree("def main(): Bool := 1 != 1");
}

#[test]
fn ge_le_operators_agree() {
    assert_hir_mir_agree("def main(): Bool := 3 >= 3");
    assert_hir_mir_agree("def main(): Bool := 2 >= 3");
    assert_hir_mir_agree("def main(): Bool := 2 <= 3");
    assert_hir_mir_agree("def main(): Bool := 3 <= 3");
}

#[test]
fn power_of_one_is_identity() {
    assert_hir_mir_agree("def main(): Int := 42 ^ 1");
}

#[test]
fn deeply_nested_arithmetic_agrees() {
    assert_hir_mir_agree("def main(): Int := ((1 + 2) * (3 - 1)) ^ 2");
}

#[test]
fn while_result_is_unit_in_both() {
    assert_hir_mir_agree(
        "def main(): Unit := {
            let mut i: Int = 0;
            while i < 5 do { i = i + 1; };
        }",
    );
}
