//! Tests for the MIR:
//!   - structural tests on the lowered MIR (block counts, locals, entry point)
//!   - MIR interpreter correctness
//!   - cross-checks that the MIR and typed-HIR interpreters agree

// these tests were (obviously) llm-generated. i take responsibility but no
// credit

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::interpreter::mir::MirInterpError;
use sand::interpreter::mir::MirValue;
use sand::ir_types::mir::MirProgram;
use sand::ir_types::mir::Terminator;
use sand::ir_types::typed_hir::Expression;

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Compile `src` all the way to a `MirProgram`.
fn lower(src: &str) -> (MirProgram, CompileCtx<'static>) {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let code = Map::from([(fr, src)]);
    let ast = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed:\n  {e}"));
    (MirProgram::from_typed_program(&ast), ctx)
}

/// Compile and run via the MIR interpreter; return the `MirValue`.
fn run_mir(src: &str) -> MirValue {
    let (mir, ctx) = lower(src);
    mir.interpret(&ctx)
        .unwrap_or_else(|e| panic!("MIR interpret failed:\n  {e}"))
}

/// Compile and run via the MIR interpreter; expect an error.
fn run_mir_fails(src: &str) -> MirInterpError {
    let (mir, ctx) = lower(src);
    mir.interpret(&ctx)
        .expect_err("expected MIR interpret to fail, but it succeeded")
}

/// Run both the typed-HIR and MIR interpreters and assert they agree.
fn assert_hir_mir_agree(src: &str) {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let code = Map::from([(fr, src)]);
    let ast = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed:\n  {e}"));

    let hir_result = ast
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("HIR interpret failed:\n  {e}"));

    let mir = MirProgram::from_typed_program(&ast);
    let mir_result = mir
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("MIR interpret failed:\n  {e}"));

    // convert MirValue → Expression for comparison
    let mir_as_expr = match mir_result {
        MirValue::Int(i) => Expression::Int(i),
        MirValue::Bool(b) => Expression::Bool(b),
        MirValue::Unit => Expression::Unit,
    };

    assert_eq!(
        hir_result, mir_as_expr,
        "HIR and MIR interpreters disagreed on:\n  {src}"
    );
}

// ─── structural tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod mir_structure_tests {
    use super::*;

    #[test]
    fn function_count_matches_source() {
        let (mir, _ctx) = lower("def main(): Int := 42");
        assert_eq!(mir.functions.len(), 1);

        let (mir, _ctx) = lower(
            "def helper(): Int := 1
             def main(): Int := helper()",
        );
        assert_eq!(mir.functions.len(), 2);

        let (mir, _ctx) = lower(
            "def a(): Int := 1
             def b(): Int := 2
             def c(): Int := 3
             def main(): Int := a()",
        );
        assert_eq!(mir.functions.len(), 4);
    }

    #[test]
    fn simple_literal_has_at_least_one_block() {
        let (mir, ctx) = lower("def main(): Int := 99");
        for func in mir.functions.values() {
            assert!(
                !func.blocks.is_empty(),
                "function {} has no blocks",
                ctx.original_fun_name(func.name)
            );
        }
    }

    #[test]
    fn all_blocks_have_a_terminator() {
        // Every basic block must end with a non-Unreachable terminator
        // (Unreachable is only for provably dead code paths).
        let cases = [
            "def main(): Int := 1 + 2",
            "def main(): Bool := if true then false else true",
            "def main(): Int := {
                let i: Int = 0;
                while i < 5 do { i = i + 1; };
                i
            }",
            "def f(x: Int): Int := x * 2
             def main(): Int := f(21)",
        ];
        for src in cases {
            let (mir, _ctx) = lower(src);
            for func in mir.functions.values() {
                for block in &func.blocks {
                    assert!(
                        !matches!(block.terminator, Terminator::Unreachable),
                        "block {} in function {:?} has Unreachable terminator for:\n  {src}",
                        block.id.0,
                        func.name
                    );
                }
            }
        }
    }

    #[test]
    fn parameters_have_corresponding_locals() {
        // Every MirParam must correspond to an entry in func.locals.
        let (mir, _ctx) = lower(
            "def add(a: Int, b: Int): Int := a + b
                                  def main(): Int := add(1, 2)",
        );
        for func in mir.functions.values() {
            for param in &func.params {
                assert!(
                    func.locals.iter().any(|l| l.id == param.local),
                    "param local {:?} not found in locals for {:?}",
                    param.local,
                    func.name
                );
            }
        }
    }

    #[test]
    fn if_expression_produces_branch_terminator() {
        // condition must be a runtime value — a constant bool is short-circuited
        // in lower_pred and produces no Branch terminator
        let (mir, _ctx) = lower(
            "def main(): Int := {
            let b: Bool = true;
            if b then 1 else 2
        }",
        );
        let func = mir.functions.values().next().unwrap();
        let has_branch = func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Branch { .. }));
        assert!(
            has_branch,
            "expected at least one Branch terminator for if expression"
        );
    }

    #[test]
    fn while_loop_produces_branch_and_goto() {
        let src = "def main(): Int := {
            let i: Int = 0;
            while i < 3 do { i = i + 1; };
            i
        }";
        let (mir, _ctx) = lower(src);
        let func = mir.functions.values().next().unwrap();
        let has_branch = func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Branch { .. }));
        let has_goto = func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Goto { .. }));
        assert!(has_branch, "while loop should produce a Branch terminator");
        assert!(
            has_goto,
            "while loop should produce a Goto (back-edge) terminator"
        );
    }

    #[test]
    fn no_function_has_zero_locals_when_it_has_params() {
        let (mir, _ctx) = lower(
            "def f(x: Int, y: Bool): Int := 0
                                  def main(): Int := f(1, true)",
        );
        for func in mir.functions.values() {
            if !func.params.is_empty() {
                assert!(
                    !func.locals.is_empty(),
                    "function {:?} has params but no locals",
                    func.name
                );
            }
        }
    }

    #[test]
    fn let_bindings_produce_locals() {
        // A function with two let bindings should have at least two user locals.
        let src = "def main(): Int := {
            let a: Int = 1;
            let b: Int = 2;
            a + b
        }";
        let (mir, _ctx) = lower(src);
        let func = mir.functions.values().next().unwrap();
        let user_locals = func
            .locals
            .iter()
            .filter(|l| matches!(l.name, sand::ir_types::mir::LocalName::User(_)))
            .count();
        assert!(
            user_locals >= 2,
            "expected at least 2 user locals, got {user_locals}"
        );
    }
}

// ─── MIR interpreter ─────────────────────────────────────────────────────────

#[cfg(test)]
mod mir_interpreter_tests {
    use super::*;

    // ── literals ─────────────────────────────────────────────────────────

    #[test]
    fn int_literal() {
        assert_eq!(run_mir("def main(): Int := 7"), MirValue::Int(7));
    }

    #[test]
    fn bool_true() {
        assert_eq!(run_mir("def main(): Bool := true"), MirValue::Bool(true));
    }

    #[test]
    fn bool_false() {
        assert_eq!(run_mir("def main(): Bool := false"), MirValue::Bool(false));
    }

    #[test]
    fn unit_literal() {
        assert_eq!(run_mir("def main(): Unit := { }"), MirValue::Unit);
    }

    // ── arithmetic ───────────────────────────────────────────────────────

    #[test]
    fn addition() {
        assert_eq!(run_mir("def main(): Int := 3 + 4"), MirValue::Int(7));
    }

    #[test]
    fn subtraction() {
        assert_eq!(run_mir("def main(): Int := 10 - 3"), MirValue::Int(7));
    }

    #[test]
    fn multiplication() {
        assert_eq!(run_mir("def main(): Int := 6 * 7"), MirValue::Int(42));
    }

    #[test]
    fn division_exact() {
        assert_eq!(run_mir("def main(): Int := 20 / 4"), MirValue::Int(5));
    }

    #[test]
    fn division_truncates() {
        // Integer division truncates toward zero.
        assert_eq!(run_mir("def main(): Int := 7 / 2"), MirValue::Int(3));
    }

    #[test]
    fn power() {
        assert_eq!(run_mir("def main(): Int := 2 ^ 8"), MirValue::Int(256));
    }

    #[test]
    fn unary_negation() {
        assert_eq!(run_mir("def main(): Int := -(5)"), MirValue::Int(-5));
    }

    #[test]
    fn operator_precedence_mul_before_add() {
        assert_eq!(run_mir("def main(): Int := 2 + 3 * 4"), MirValue::Int(14));
    }

    #[test]
    fn parentheses_override_precedence() {
        assert_eq!(run_mir("def main(): Int := (2 + 3) * 4"), MirValue::Int(20));
    }

    // ── division by zero ──────────────────────────────────────────────────

    #[test]
    fn division_by_zero_is_an_error() {
        let err = run_mir_fails("def main(): Int := 1 / 0");
        assert!(
            matches!(err, MirInterpError::DivisionByZero),
            "expected DivisionByZero, got {err}"
        );
    }

    #[test]
    fn division_by_zero_inside_expression() {
        let src = "def main(): Int := {
            let x: Int = 0;
            10 / x
        }";
        let err = run_mir_fails(src);
        assert!(
            matches!(err, MirInterpError::DivisionByZero),
            "expected DivisionByZero, got {err}"
        );
    }

    // ── boolean operations ────────────────────────────────────────────────

    #[test]
    fn bool_and_tt() {
        assert_eq!(
            run_mir("def main(): Bool := true & true"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn bool_and_tf() {
        assert_eq!(
            run_mir("def main(): Bool := true & false"),
            MirValue::Bool(false)
        );
    }

    #[test]
    fn bool_or_ft() {
        assert_eq!(
            run_mir("def main(): Bool := false | true"),
            MirValue::Bool(true)
        );
    }

    #[test]
    fn bool_or_ff() {
        assert_eq!(
            run_mir("def main(): Bool := false | false"),
            MirValue::Bool(false)
        );
    }

    #[test]
    fn bool_not_true() {
        assert_eq!(run_mir("def main(): Bool := !true"), MirValue::Bool(false));
    }

    #[test]
    fn bool_not_false() {
        assert_eq!(run_mir("def main(): Bool := !false"), MirValue::Bool(true));
    }

    #[test]
    fn bool_xor() {
        assert_eq!(
            run_mir("def main(): Bool := true # false"),
            MirValue::Bool(true)
        );
        assert_eq!(
            run_mir("def main(): Bool := true # true"),
            MirValue::Bool(false)
        );
    }

    // ── comparisons ───────────────────────────────────────────────────────

    #[test]
    fn eq_true() {
        assert_eq!(run_mir("def main(): Bool := 5 == 5"), MirValue::Bool(true));
    }

    #[test]
    fn eq_false() {
        assert_eq!(run_mir("def main(): Bool := 5 == 6"), MirValue::Bool(false));
    }

    #[test]
    fn ne() {
        assert_eq!(run_mir("def main(): Bool := 5 != 6"), MirValue::Bool(true));
    }

    #[test]
    fn lt_true() {
        assert_eq!(run_mir("def main(): Bool := 3 < 5"), MirValue::Bool(true));
    }

    #[test]
    fn lt_false_when_equal() {
        assert_eq!(run_mir("def main(): Bool := 5 < 5"), MirValue::Bool(false));
    }

    #[test]
    fn le_true_when_equal() {
        assert_eq!(run_mir("def main(): Bool := 5 <= 5"), MirValue::Bool(true));
    }

    #[test]
    fn gt_true() {
        assert_eq!(run_mir("def main(): Bool := 5 > 3"), MirValue::Bool(true));
    }

    #[test]
    fn ge_true_when_equal() {
        assert_eq!(run_mir("def main(): Bool := 5 >= 5"), MirValue::Bool(true));
    }

    // ── if / else ─────────────────────────────────────────────────────────

    #[test]
    fn if_true_branch() {
        assert_eq!(
            run_mir("def main(): Int := if true then 1 else 2"),
            MirValue::Int(1)
        );
    }

    #[test]
    fn if_false_branch() {
        assert_eq!(
            run_mir("def main(): Int := if false then 1 else 2"),
            MirValue::Int(2)
        );
    }

    #[test]
    fn if_with_comparison_condition() {
        assert_eq!(
            run_mir("def main(): Int := if 3 < 5 then 10 else 20"),
            MirValue::Int(10)
        );
    }

    #[test]
    fn nested_if() {
        let src = "def main(): Int :=
            if true then
                if false then 10 else 20
            else 30";
        assert_eq!(run_mir(src), MirValue::Int(20));
    }

    // ── while ─────────────────────────────────────────────────────────────

    #[test]
    fn while_never_entered_when_condition_false() {
        let src = "def main(): Int := {
            let x: Int = 0;
            while false do { x = x + 1; };
            x
        }";
        assert_eq!(run_mir(src), MirValue::Int(0));
    }

    #[test]
    fn while_runs_correct_number_of_iterations() {
        let src = "def main(): Int := {
            let x: Int = 0;
            while x < 5 do { x = x + 1; };
            x
        }";
        assert_eq!(run_mir(src), MirValue::Int(5));
    }

    #[test]
    fn while_accumulator_sum() {
        // 1 + 2 + … + 10 = 55
        let src = "def main(): Int := {
            let i: Int = 1;
            let s: Int = 0;
            while i <= 10 do {
                s = s + i;
                i = i + 1;
            };
            s
        }";
        assert_eq!(run_mir(src), MirValue::Int(55));
    }

    #[test]
    fn while_condition_uses_updated_variable() {
        // Make sure the condition is re-evaluated each iteration.
        let src = "def main(): Bool := {
            let flag: Bool = true;
            let i: Int = 0;
            while flag do {
                i = i + 1;
                flag = i < 3;
            };
            i == 3
        }";
        assert_eq!(run_mir(src), MirValue::Bool(true));
    }

    // ── blocks and let-bindings ───────────────────────────────────────────

    #[test]
    fn block_returns_trailing_expression() {
        let src = "def main(): Int := {
            let a: Int = 3;
            let b: Int = 4;
            a + b
        }";
        assert_eq!(run_mir(src), MirValue::Int(7));
    }

    #[test]
    fn block_with_only_statements_returns_unit() {
        let src = "def main(): Unit := {
            let x: Int = 1;
        }";
        assert_eq!(run_mir(src), MirValue::Unit);
    }

    #[test]
    fn nested_block_scoping() {
        let src = "def main(): Int := {
            let a: Int = 1;
            let b: Int = {
                let a: Int = 100;
                a + 1
            };
            a + b
        }";
        // outer a=1, inner block → 101, b=101, result = 102
        assert_eq!(run_mir(src), MirValue::Int(102));
    }

    #[test]
    fn assignment_updates_value() {
        let src = "def main(): Int := {
            let x: Int = 1;
            x = 42;
            x
        }";
        assert_eq!(run_mir(src), MirValue::Int(42));
    }

    #[test]
    fn multiple_assignments_to_same_variable() {
        let src = "def main(): Int := {
            let x: Int = 1;
            x = 2;
            x = 3;
            x
        }";
        assert_eq!(run_mir(src), MirValue::Int(3));
    }

    // ── function calls ────────────────────────────────────────────────────

    #[test]
    fn call_no_args() {
        let src = "def answer(): Int := 42
                   def main(): Int := answer()";
        assert_eq!(run_mir(src), MirValue::Int(42));
    }

    #[test]
    fn call_with_args() {
        let src = "def add(a: Int, b: Int): Int := a + b
                   def main(): Int := add(10, 32)";
        assert_eq!(run_mir(src), MirValue::Int(42));
    }

    #[test]
    fn recursive_factorial() {
        let src = "
            def fact(n: Int): Int :=
                if n == 0 then 1 else n * fact(n - 1)
            def main(): Int := fact(9)";
        assert_eq!(run_mir(src), MirValue::Int(362880));
    }

    #[test]
    fn recursive_fibonacci() {
        let src = "
            def fib(n: Int): Int :=
                if n <= 1 then n
                else fib(n - 1) + fib(n - 2)
            def main(): Int := fib(10)";
        assert_eq!(run_mir(src), MirValue::Int(55));
    }

    #[test]
    fn mutual_recursion() {
        let src = "
            def is_odd(n: Int): Bool :=
                if n == 0 then false else is_even(n - 1)
            def is_even(n: Int): Bool :=
                if n == 0 then true else is_odd(n - 1)
            def main(): Bool := is_even(10)";
        assert_eq!(run_mir(src), MirValue::Bool(true));
    }

    #[test]
    fn chained_calls() {
        let src = "
            def double(x: Int): Int := x * 2
            def quad(x: Int): Int := double(double(x))
            def main(): Int := quad(5)";
        assert_eq!(run_mir(src), MirValue::Int(20));
    }

    // ── edge cases ────────────────────────────────────────────────────────

    #[test]
    fn zero_power_is_one() {
        assert_eq!(run_mir("def main(): Int := 99 ^ 0"), MirValue::Int(1));
    }

    #[test]
    fn negate_zero_is_zero() {
        assert_eq!(run_mir("def main(): Int := -(0)"), MirValue::Int(0));
    }

    #[test]
    fn large_integer() {
        assert_eq!(
            run_mir("def main(): Int := 1000000 * 1000000"),
            MirValue::Int(1_000_000_000_000)
        );
    }

    #[test]
    fn negative_intermediate_value() {
        assert_eq!(run_mir("def main(): Int := 3 - 10 + 4"), MirValue::Int(-3));
    }

    #[test]
    fn bool_equality() {
        assert_eq!(
            run_mir("def main(): Bool := true == true"),
            MirValue::Bool(true)
        );
        assert_eq!(
            run_mir("def main(): Bool := true == false"),
            MirValue::Bool(false)
        );
    }

    #[test]
    fn bool_ne() {
        assert_eq!(
            run_mir("def main(): Bool := true != false"),
            MirValue::Bool(true)
        );
    }
}

// ─── HIR ↔ MIR cross-checks ──────────────────────────────────────────────────
//
// For every program here, both interpreters must produce the same answer.
// This catches divergence between the two execution paths without having
// to hard-code expected values for complex programs.

#[cfg(test)]
mod mir_hir_agreement_tests {
    use super::*;

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
        assert_hir_mir_agree("def main(): Bool := (1 < 2) & (3 > 2) | false");
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
                let i: Int = 1;
                let s: Int = 0;
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
                let x: Int = 10;
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
        assert_hir_mir_agree("def main(): Bool := !false & (1 == 1) | (2 != 3)");
    }

    #[test]
    fn agree_multiple_function_calls() {
        assert_hir_mir_agree(
            "def inc(x: Int): Int := x + 1
             def double(x: Int): Int := x * 2
             def main(): Int := double(inc(inc(5)))",
        );
    }
}
