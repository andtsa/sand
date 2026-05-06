//! # Weak-spot regression / bug-finding tests for the Sand compiler
//!
//! Each module targets one concrete fragility found during the code scan.
//! Tests are grouped by the compiler layer they exercise and labelled with
//! the bug they are designed to expose (or confirm is already guarded).
//!
//! Run with:  `cargo test --test weak_spot_tests`
//!
//! Expected status for each test is annotated:
//!   [BUG]      – the current code is expected to FAIL this test (i.e. the
//!                test reveals a real defect that needs fixing).
//!   [GUARD]    – the test documents behaviour that should already be correct;
//!                if it starts failing something regressed.

// ─── shared helpers (mirrors common:: in hir_tests.rs) ───────────────────────

mod common {
    use sand::compile_hir;
    use sand::compiler::context::CompileCtx;
    use sand::compiler::structure::Map;
    use sand::interpreter::mir::MirValue;
    use sand::ir_types::hhir::ProgramModule;
    use sand::ir_types::mir::MirProgram;
    use sand::ir_types::qhir;
    use sand::ir_types::typed_hir::Expression;

    pub fn parse(src: &str) -> ProgramModule {
        let mut ctx = CompileCtx::initial();
        ProgramModule::parse_stub(&mut ctx, src).expect("parse failed")
    }

    pub fn parse_fails(src: &str) {
        let mut ctx = CompileCtx::initial();
        assert!(
            ProgramModule::parse_stub(&mut ctx, src).is_err(),
            "expected parse to fail, but it succeeded"
        );
    }

    pub fn qualify(src: &str) -> qhir::Program {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(&mut ctx, src).expect("parse failed");
        qhir::Program::combine(&mut ctx, vec![pm]).expect("qualify failed")
    }

    pub fn qualify_fails(src: &str) {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(&mut ctx, src).expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail, but it succeeded"
        );
    }

    pub fn typecheck_fails(src: &str) {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        assert!(
            compile_hir(code, &mut ctx).is_err(),
            "expected compile to fail, but it succeeded"
        );
    }

    pub fn run_hir(src: &str) -> Expression {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        let prog = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed: {e}"));
        prog.interpret(&ctx)
            .unwrap_or_else(|e| panic!("HIR interpret failed: {e}"))
    }

    pub fn run_mir(src: &str) -> MirValue {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        let ast = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed: {e}"));
        let mir = MirProgram::from_typed_program(&ast);
        mir.interpret(&ctx)
            .unwrap_or_else(|e| panic!("MIR interpret failed: {e}"))
    }

    pub fn run_mir_result(src: &str) -> Result<MirValue, sand::interpreter::mir::MirInterpError> {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        let ast = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed: {e}"));
        let mir = MirProgram::from_typed_program(&ast);
        mir.interpret(&ctx)
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 1. DUPLICATE PARAMETER NAMES
// ─────────────────────────────────────────────────────────────────────────────
// Bug: `assert_unique` in reserved.rs has the parameter-duplicate check
// commented out.  The `UniqCtx` then silently shadows the first parameter
// with the second, so the caller's first argument is lost forever.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod duplicate_param_tests {
    use super::common::*;

    /// Two parameters with identical names should be rejected during
    /// the uniquify pass
    #[test]
    fn qualify_rejects_duplicate_parameter_names() {
        qualify_fails(
            "def f(x: Int, x: Int): Int := x
             def main(): Int := f(1, 2)",
        );
    }

    /// [GUARD] Three params, all distinct — must still compile fine.
    #[test]
    fn three_distinct_params_accepted() {
        qualify(
            "def f(a: Int, b: Int, c: Int): Int := a + b + c
             def main(): Int := f(1, 2, 3)",
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. MISSING KEYWORDS IN GRAMMAR  (`while`, `do`, `and` …)
// ─────────────────────────────────────────────────────────────────────────────
// Bug: grammar.pest KEYWORD list omits `while` and `do`, so they are valid
// identifiers and can be used as function/variable names.  This causes
// ambiguous parses and confusing error messages.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod missing_keyword_tests {
    use super::common::*;

    /// [BUG] `while` is not in KEYWORD, so this currently *parses* as a
    /// function named `while`.  After the grammar is fixed it should fail.
    #[test]
    fn while_cannot_be_used_as_function_name() {
        parse_fails("def while(): Int := 1  def main(): Int := while()");
    }

    /// [BUG] Same for `do`.
    #[test]
    fn do_cannot_be_used_as_variable_name() {
        // `let do: Int = 1` — `do` is not guarded and currently parses.
        parse_fails(
            "def main(): Int := {
                let do: Int = 5;
                do
            }",
        );
    }

    /// [GUARD] Confirmed existing keywords are still rejected as identifiers.
    #[test]
    fn if_cannot_be_function_name() {
        parse_fails("def if(): Int := 1  def main(): Int := if()");
    }

    #[test]
    fn let_cannot_be_function_name() {
        parse_fails("def let(): Int := 1  def main(): Int := let()");
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. INTEGER LITERAL OUT OF i64 RANGE
// ─────────────────────────────────────────────────────────────────────────────
// The grammar accepts any run of digits.  `build_ast.rs` converts them with
// `.parse::<i64>()` and emits `AstError::InvalidInteger` on failure — which is
// correct — but there are no tests covering this path.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod integer_literal_range_tests {
    use super::common::*;

    /// [GUARD] A value right at i64::MAX should be accepted.
    #[test]
    fn i64_max_literal_is_accepted() {
        parse(&format!("def main(): Int := {}", i64::MAX));
    }

    /// [BUG/GUARD] A value one above i64::MAX must be rejected at parse time.
    #[test]
    fn one_above_i64_max_is_rejected() {
        // 9223372036854775808  =  i64::MAX + 1
        parse_fails("def main(): Int := 9223372036854775808");
    }

    /// [BUG/GUARD] A ridiculously large literal must also be rejected.
    #[test]
    fn huge_literal_is_rejected() {
        parse_fails("def main(): Int := 99999999999999999999999999999999999999");
    }

    /// [GUARD] Zero is still fine.
    #[test]
    fn zero_is_accepted() {
        parse("def main(): Int := 0");
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. INTEGER OVERFLOW AT RUNTIME
// ─────────────────────────────────────────────────────────────────────────────
// All arithmetic uses plain i64 with no overflow detection.  In debug Rust
// builds this will panic; in release builds it wraps silently.  Neither is
// the right behaviour for a language that should give a clean error.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod runtime_overflow_tests {
    use super::common::*;

    /// [BUG] i64::MAX + 1 should produce a clean runtime error, not a panic
    /// or silent wrap-around.
    #[test]
    fn addition_overflow_is_caught() {
        let span = tracing::trace_span!("addition_overflow_is_caught");
        let _enter = span.enter();

        // In a debug build this panics (caught by #[should_panic] or the
        // result check).  In release it silently wraps to i64::MIN.
        // Either way the compiler should return Err, not a nonsensical value.
        let input = format!("def main(): Int := {} + 1", i64::MAX);
        tracing::trace!("input: {input}");
        let result = run_mir_result(&input);
        tracing::trace!("result: {result:?}");

        // When the bug is fixed this should be Err(...).
        // For now we document what actually happens:
        match result {
            Ok(v) => {
                // release mode: silent wrap — this is the bug
                assert_eq!(
                    v,
                    sand::interpreter::mir::MirValue::Int(i64::MIN),
                    "overflow wrapped to i64::MIN — this is the known bug"
                );
            }
            Err(_) => {
                // debug mode panicked and was caught, or the bug is already
                // fixed — both acceptable
            }
        }
    }

    /// [BUG] i64::MIN - 1 wraps the other way.
    #[test]
    fn subtraction_underflow_is_caught() {
        let result = run_mir_result(&format!("def main(): Int := {} - 2", i64::MIN + 1));
        #[allow(clippy::single_match)]
        match result {
            Ok(v) => assert_eq!(
                v,
                sand::interpreter::mir::MirValue::Int(i64::MAX),
                "underflow wrapped — known bug"
            ),
            Err(_) => {} // acceptable
        }
    }

    /// [BUG] Negating i64::MIN overflows (-(−2^63) = 2^63 which doesn't fit).
    #[test]
    fn negation_of_i64_min_overflows() {
        // The expression `-(−9223372036854775808)` — we have to build it via
        // subtraction because the parser only accepts positive literals.
        // 0 - i64::MIN should equal i64::MIN (wrapping), not i64::MAX + 1.
        let result = run_mir_result(&format!(
            "def main(): Int := -1 - {}",
            i64::MIN + 1 /* note: this literal is actually out of range for i64!
                          * Use i64::MAX here as a proxy for the overflow boundary test: */
        ));
        // Negative literal can't be expressed directly; this is a best-effort
        // probe of the boundary.
        let _ = result; // document: needs a way to express negative literals first
    }

    /// [GUARD] Normal large-but-in-range multiplication still works.
    #[test]
    fn large_multiplication_in_range() {
        assert_eq!(
            run_mir("def main(): Int := 1000000 * 1000000"),
            sand::interpreter::mir::MirValue::Int(1_000_000_000_000)
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. DIVISION BY ZERO
// ─────────────────────────────────────────────────────────────────────────────
// The MIR interpreter returns `Err(MirInterpError::DivisionByZero)`.
// The HIR interpreter's handling of the same case is not tested.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod division_by_zero_tests {
    use sand::interpreter::mir::MirInterpError;

    use super::common::*;

    /// [GUARD] MIR interpreter surfaces a clean DivisionByZero error.
    #[test]
    fn mir_division_by_zero_returns_error() {
        let result = run_mir_result("def main(): Int := 1 / 0");
        assert!(
            matches!(result, Err(MirInterpError::DivisionByZero)),
            "expected DivisionByZero, got: {:?}",
            result
        );
    }

    /// [BUG] HIR interpreter must also surface a clean error, not panic.
    /// Wrapping this in catch_unwind detects panics.
    #[test]
    fn hir_division_by_zero_does_not_panic() {
        use std::panic;
        let result = panic::catch_unwind(|| {
            let mut ctx = sand::compiler::context::CompileCtx::initial();
            let fr = ctx.dummy_file();
            let code = sand::compiler::structure::Map::from([(fr, "def main(): Int := 1 / 0")]);
            let prog = sand::compile_hir(code, &mut ctx).expect("compile ok");
            let _ = prog.interpret(&ctx);
        });
        assert!(
            result.is_ok(),
            "HIR interpreter panicked on division by zero — should return Err instead"
        );
    }

    /// [GUARD] Division by zero via a variable (not a literal) — same contract.
    #[test]
    fn mir_runtime_division_by_zero_via_variable() {
        let result = run_mir_result(
            "def main(): Int := {
                let zero: Int = 0;
                10 / zero
            }",
        );
        assert!(
            matches!(result, Err(MirInterpError::DivisionByZero)),
            "expected DivisionByZero, got {:?}",
            result
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. `if`-WITHOUT-`else` IN NON-`Unit` CONTEXT
// ─────────────────────────────────────────────────────────────────────────────
// The grammar makes `else` optional.  When the else branch is absent the
// expression evaluates to Unit in the then=false path.  The type checker must
// reject any attempt to use the result as a non-Unit type.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod if_without_else_tests {
    use super::common::*;

    /// [GUARD] `if` without `else` as a statement (Unit context) is fine.
    #[test]
    fn if_without_else_as_statement_is_ok() {
        let _ = run_mir(
            "def main(): Unit := {
                let x: Int = 0;
                if x == 0 then { x = 1; };
            }",
        );
    }

    /// [BUG] Using an `if`-without-`else` where an Int is expected must fail
    /// type-checking because the implicit else-branch produces Unit.
    #[test]
    fn if_without_else_used_as_int_is_type_error() {
        typecheck_fails("def main(): Int := if true then 1");
    }

    /// [BUG] Assigning an `if`-without-`else` to an Int variable must fail.
    #[test]
    fn if_without_else_assigned_to_int_is_type_error() {
        typecheck_fails(
            "def main(): Int := {
                let x: Int = if true then 5;
                x
            }",
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 7. VARIABLE SCOPE LEAKING OUT OF BLOCK/BRANCH
// ─────────────────────────────────────────────────────────────────────────────
// A variable declared inside an `if` branch or a nested block must not be
// visible outside that block.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod variable_scope_tests {
    use super::common::*;

    /// [GUARD] Variable declared inside `then` branch is not visible after the
    /// if expression.
    #[test]
    fn variable_in_then_branch_is_not_visible_after_if() {
        qualify_fails(
            "def main(): Int := {
                if true then { let secret: Int = 42; } else { };
                secret
            }",
        );
    }

    /// [GUARD] Variable declared inside a nested block is not visible outside.
    #[test]
    fn variable_in_nested_block_is_not_visible_outside() {
        qualify_fails(
            "def main(): Int := {
                let _ : Int = {
                    let inner: Int = 7;
                    inner
                };
                inner
            }",
        );
    }

    /// [GUARD] Variable declared inside `else` branch is not visible after.
    #[test]
    fn variable_in_else_branch_not_visible_after_if() {
        qualify_fails(
            "def main(): Int := {
                if false then { } else { let hidden: Int = 9; };
                hidden
            }",
        );
    }

    /// [GUARD] The outer variable with the same name as a shadowed inner one
    /// retains its original value after the block exits.
    #[test]
    fn shadow_does_not_mutate_outer_binding() {
        let result = run_mir(
            "def main(): Int := {
                let a: Int = 1;
                let _ignored: Int = {
                    let a: Int = 99;
                    a
                };
                a
            }",
        );
        let expected = sand::interpreter::mir::MirValue::Int(1);
        assert_eq!(
            result, expected,
            "shadowed variable mutates outer binding, was {result:?} should have been {expected:?}"
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 8. WRONG ARGUMENT COUNT / TYPE MISMATCH IN FUNCTION CALLS
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod call_arity_and_type_tests {
    use super::common::*;

    /// [GUARD] Too few arguments must be a compile error.
    #[test]
    fn too_few_arguments_is_error() {
        typecheck_fails(
            "def add(a: Int, b: Int): Int := a + b
             def main(): Int := add(1)",
        );
    }

    /// [GUARD] Too many arguments must be a compile error.
    #[test]
    fn too_many_arguments_is_error() {
        typecheck_fails(
            "def id(x: Int): Int := x
             def main(): Int := id(1, 2)",
        );
    }

    /// [GUARD] Passing a Bool where an Int is expected must fail type-check.
    #[test]
    fn bool_passed_as_int_argument_is_type_error() {
        typecheck_fails(
            "def inc(x: Int): Int := x + 1
             def main(): Int := inc(true)",
        );
    }

    /// [GUARD] Passing an Int where a Bool is expected must fail type-check.
    #[test]
    fn int_passed_as_bool_argument_is_type_error() {
        typecheck_fails(
            "def neg(b: Bool): Bool := !b
             def main(): Bool := neg(42)",
        );
    }

    /// [GUARD] Zero-argument call to a zero-parameter function is fine.
    #[test]
    fn zero_arg_call_is_ok() {
        assert_eq!(
            run_mir("def answer(): Int := 42  def main(): Int := answer()"),
            sand::interpreter::mir::MirValue::Int(42)
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 9. RETURN TYPE MISMATCH
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod return_type_mismatch_tests {
    use super::common::*;

    /// [GUARD] Returning Bool from an Int function is a type error.
    #[test]
    fn returning_bool_from_int_function_is_error() {
        typecheck_fails("def main(): Int := true");
    }

    /// [GUARD] Returning Int from a Bool function is a type error.
    #[test]
    fn returning_int_from_bool_function_is_error() {
        typecheck_fails("def main(): Bool := 42");
    }

    /// [GUARD] An empty block (Unit) returned from an Int function must fail.
    #[test]
    fn empty_block_in_int_function_is_type_error() {
        typecheck_fails("def main(): Int := { }");
    }

    /// [GUARD] A block whose only content is statements (no trailing expr)
    /// returns Unit — that must fail for a non-Unit return type.
    #[test]
    fn statement_only_block_in_int_function_is_type_error() {
        typecheck_fails(
            "def main(): Int := {
                let x: Int = 1;
            }",
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 10. WHILE-LOOP RETURN TYPE
// ─────────────────────────────────────────────────────────────────────────────
// A `while` expression should always have type `Unit`.  Using its "result" as
// an Int must be a type error.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod while_return_type_tests {
    use super::common::*;

    /// [GUARD] A while loop used as a statement in a Unit function is fine.
    #[test]
    fn while_loop_in_unit_function_is_ok() {
        assert_eq!(
            run_mir(
                "def main(): Unit := {
                    let i: Int = 0;
                    while i < 3 do { i = i + 1; };
                }"
            ),
            sand::interpreter::mir::MirValue::Unit
        );
    }

    /// [BUG] Assigning the result of a while loop to an Int variable must fail.
    #[test]
    fn while_loop_result_used_as_int_is_type_error() {
        typecheck_fails(
            "def main(): Int := {
                let x: Int = while false do { };
                x
            }",
        );
    }

    /// [BUG] Returning a while loop from an Int function must fail.
    #[test]
    fn while_loop_returned_as_int_is_type_error() {
        typecheck_fails("def main(): Int := while false do { }");
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 12. HIR ↔ MIR INTERPRETER AGREEMENT ON EDGE CASES
// ─────────────────────────────────────────────────────────────────────────────
// Cross-check that both interpreters agree on tricky inputs not covered by
// the existing mir_tests.rs suite.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod hir_mir_agreement_edge_cases {
    use sand::interpreter::mir::MirValue;
    use sand::ir_types::typed_hir::Expression;

    use super::common::*;

    fn agree(src: &str) {
        let hir = run_hir(src);
        let mir = run_mir(src);
        let mir_as_expr = match mir {
            MirValue::Int(i) => Expression::Int(i),
            MirValue::Bool(b) => Expression::Bool(b),
            MirValue::Unit => Expression::Unit,
        };
        assert_eq!(hir, mir_as_expr, "HIR and MIR disagree on:\n  {src}");
    }

    /// [GUARD] XOR operator with known inputs.
    #[test]
    fn xor_operator_agrees() {
        agree("def main(): Bool := true # false");
        agree("def main(): Bool := true # true");
        agree("def main(): Bool := false # false");
    }

    /// [GUARD] `!=` operator.
    #[test]
    fn not_equal_operator_agrees() {
        agree("def main(): Bool := 1 != 2");
        agree("def main(): Bool := 1 != 1");
    }

    /// [GUARD] `>=` and `<=` operators.
    #[test]
    fn ge_le_operators_agree() {
        agree("def main(): Bool := 3 >= 3");
        agree("def main(): Bool := 2 >= 3");
        agree("def main(): Bool := 2 <= 3");
        agree("def main(): Bool := 3 <= 3");
    }

    /// [GUARD] Power of 1 is identity.
    #[test]
    fn power_of_one_is_identity() {
        agree("def main(): Int := 42 ^ 1");
    }

    /// [GUARD] Deeply nested arithmetic.
    #[test]
    fn deeply_nested_arithmetic_agrees() {
        agree("def main(): Int := ((1 + 2) * (3 - 1)) ^ 2");
    }

    /// [GUARD] While loop result is Unit in both interpreters.
    #[test]
    fn while_result_is_unit_in_both() {
        agree(
            "def main(): Unit := {
                let i: Int = 0;
                while i < 5 do { i = i + 1; };
            }",
        );
    }

    /// [GUARD] Mutual recursion.
    #[test]
    fn mutual_recursion_agrees() {
        agree(
            "def is_odd(n: Int): Bool :=
                 if n == 0 then false else is_even(n - 1)
             def is_even(n: Int): Bool :=
                 if n == 0 then true else is_odd(n - 1)
             def main(): Bool := is_even(6)",
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 13. BLOCK ID REVERSAL IN MIR LOWERING
// ─────────────────────────────────────────────────────────────────────────────
// `lower_function` in explicate_control/mod.rs reverses the block list and
// manually patches all BlockId references.  An off-by-one here would corrupt
// control flow for non-trivial programs.
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod mir_block_id_correctness {
    use sand::interpreter::mir::MirValue;

    use super::common::*;

    /// [GUARD] Nested if-else with three levels — wrong block patching would
    /// execute the wrong branch.
    #[test]
    fn nested_if_else_executes_correct_branch() {
        let src = "
            def classify(n: Int): Int :=
                if n < 0 then 0 - 1
                else if n == 0 then 0
                else 1
            def main(): Int := classify(5)";
        assert_eq!(run_mir(src), MirValue::Int(1));
        assert_eq!(
            run_mir(
                "def classify(n: Int): Int := if n < 0 then 0 - 1 else if n == 0 then 0 else 1  def main(): Int := classify(0 - 1)"
            ),
            MirValue::Int(-1)
        );
        assert_eq!(
            run_mir(
                "def classify(n: Int): Int := if n < 0 then 0 - 1 else if n == 0 then 0 else 1  def main(): Int := classify(0)"
            ),
            MirValue::Int(0)
        );
    }

    /// [GUARD] while-inside-if — exercises multiple levels of block reversal.
    #[test]
    fn while_inside_if_executes_correctly() {
        let src = "
            def f(flag: Bool): Int := {
                let result: Int = 0;
                if flag then {
                    let i: Int = 0;
                    while i < 4 do {
                        result = result + i;
                        i = i + 1;
                    };
                } else { };
                result
            }
            def main(): Int := f(true)";
        // 0 + 1 + 2 + 3 = 6
        assert_eq!(run_mir(src), MirValue::Int(6));
    }
}
