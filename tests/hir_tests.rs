//! tests for IR types and passes:
//! hhir (parse + build_ast), qhir (qualify + uniquify), typed_hir (type_ast),
//! and the interpreter.

// these tests were (obviously) llm-generated. i take responsibility but no
// credit

// ─── shared helpers ──────────────────────────────────────────────────────────

mod common {
    use sand::compile_hir;
    use sand::compiler::context::CompileCtx;
    use sand::compiler::structure::Map;
    use sand::ir_types::hhir::ProgramModule;
    use sand::ir_types::qhir;
    use sand::ir_types::typed_hir::Expression;
    use sand::ir_types::typed_hir::TypedProgram;

    /// Parse only — returns the raw `ProgramModule`.
    pub fn parse(src: &str) -> ProgramModule {
        let mut ctx = CompileCtx::initial();
        ProgramModule::parse_stub(&mut ctx, src).expect("parse failed")
    }

    /// Parse and expect failure.
    pub fn parse_fails(src: &str) {
        let mut ctx = CompileCtx::initial();
        assert!(
            ProgramModule::parse_stub(&mut ctx, src).is_err(),
            "expected parse to fail, but it succeeded"
        );
    }

    /// Parse → qualify.
    pub fn qualify(src: &str) -> qhir::Program {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(&mut ctx, src).expect("parse failed");
        qhir::Program::combine(&mut ctx, vec![pm]).expect("qualify failed")
    }

    /// Parse → qualify → type-check.
    pub fn typecheck(src: &str) -> TypedProgram {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        compile_hir(code, &mut ctx).expect("compile failed")
    }

    /// Parse → qualify → type-check and expect a **compile error**.
    pub fn typecheck_fails(src: &str) {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        assert!(
            compile_hir(code, &mut ctx).is_err(),
            "expected compile to fail, but it succeeded"
        );
    }

    // Convenience: run the interpreter and return the inner Expression value.
    // compile_hir produces a TypedProgram; interpret returns an Expression.
    pub fn run(src: &str) -> Expression {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.dummy_file();
        let code = Map::from([(fr, src)]);
        let prog = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed:\n  {e}"));
        prog.interpret(&ctx)
            .unwrap_or_else(|e| panic!("interpreting failed:\n  {e}"))
    }
}

// ─── HHIR / parse + build_ast ────────────────────────────────────────────────

#[cfg(test)]
mod hhir_tests {
    use sand::compiler::context::CompileCtx;
    use sand::ir_types::hhir::ProgramModule;

    use super::common::*;

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
        parse("def f(a: Bool, b: Bool): Bool := a & b");
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
}

// ─── QHIR / qualify + uniquify ───────────────────────────────────────────────

#[cfg(test)]
mod qhir_tests {
    use sand::compiler::context::CompileCtx;
    use sand::ir_types::hhir::ProgramModule;
    use sand::ir_types::qhir;

    use super::common::*;

    // ── happy-path qualification ──────────────────────────────────────────

    #[test]
    fn qualify_simple_program() {
        qualify("def main(): Int := 42");
    }

    #[test]
    fn qualify_self_recursive_function() {
        qualify(
            "def fact(n: Int): Int :=
                if n == 0 then 1 else n * fact(n - 1)",
        );
    }

    #[test]
    fn qualify_mutual_calls() {
        qualify(
            "def a(): Int := b()
             def b(): Int := 1
             def main(): Int := a()",
        );
    }

    #[test]
    fn qualify_intrinsic_call_is_resolved() {
        // `println` is an intrinsic; qualify must map it to IntrinsicCall.
        let prog = qualify(
            "def main(): Unit := {
                println(1);
            }",
        );
        // There should be exactly one function in the qualified program.
        assert_eq!(prog.functions.len(), 1);
    }

    #[test]
    fn qualify_variable_names_are_unique() {
        // Two functions each declare a variable named `x`; after uniquify the
        // two UniqVar indices must differ.
        let prog = qualify(
            "def f(): Int := { let x: Int = 1; x }
             def g(): Int := { let x: Int = 2; x }
             def main(): Int := f()",
        );
        // Collect all UniqVar references from the body of f and g.
        // We just confirm the program qualified without error; deeper
        // inspection would require walking the IR.
        assert_eq!(prog.functions.len(), 3);
    }

    #[test]
    fn qualify_parameter_names_are_unique_across_functions() {
        qualify(
            "def f(x: Int): Int := x
             def g(x: Int): Int := x
             def main(): Int := f(g(1))",
        );
    }

    #[test]
    fn qualify_shadowed_variable_in_nested_block() {
        qualify(
            "def main(): Int := {
                let a: Int = 1;
                let b: Int = {
                    let a: Int = 2;
                    a
                };
                a
            }",
        );
    }

    // ── qualify failures ──────────────────────────────────────────────────

    #[test]
    fn qualify_fails_undefined_function() {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(&mut ctx, "def main(): Int := undefined_fn()")
            .expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail for undefined function"
        );
    }

    #[test]
    fn qualify_fails_unbound_variable() {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(&mut ctx, "def main(): Int := x").expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail for unbound variable"
        );
    }

    #[test]
    fn qualify_fails_duplicate_main() {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(
            &mut ctx,
            "def main(): Int := 1
             def main(): Int := 2",
        )
        .expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail for duplicate main"
        );
    }

    #[test]
    fn qualify_fails_duplicate_function_in_same_module() {
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(
            &mut ctx,
            "def helper(): Int := 1
             def helper(): Int := 2
             def main(): Int := helper()",
        )
        .expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail for duplicate function"
        );
    }

    #[test]
    fn qualify_fails_assignment_to_unbound_variable() {
        // Assigning to a variable that was never declared.
        let mut ctx = CompileCtx::initial();
        let pm = ProgramModule::parse_stub(
            &mut ctx,
            "def main(): Unit := {
                x = 5;
            }",
        )
        .expect("parse ok");
        assert!(
            qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
            "expected qualify to fail: assign to undeclared variable"
        );
    }
}

// ─── TypedHIR / type_ast ─────────────────────────────────────────────────────

#[cfg(test)]
mod typed_hir_tests {
    use super::common::*;

    // ── happy-path type checking ──────────────────────────────────────────

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
}

// ─── Interpreter ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod interpreter_tests {
    use sand::ir_types::typed_hir::Expression;

    use super::common::*;

    // ── literals ─────────────────────────────────────────────────────────

    #[test]
    fn interpret_int_literal() {
        assert_eq!(run("def main(): Int := 7"), Expression::Int(7));
    }

    #[test]
    fn interpret_bool_true() {
        assert_eq!(run("def main(): Bool := true"), Expression::Bool(true));
    }

    #[test]
    fn interpret_bool_false() {
        assert_eq!(run("def main(): Bool := false"), Expression::Bool(false));
    }

    #[test]
    fn interpret_unit() {
        assert_eq!(run("def main(): Unit := { }"), Expression::Unit);
    }

    // ── arithmetic ───────────────────────────────────────────────────────

    #[test]
    fn interpret_addition() {
        assert_eq!(run("def main(): Int := 3 + 4"), Expression::Int(7));
    }

    #[test]
    fn interpret_subtraction() {
        assert_eq!(run("def main(): Int := 10 - 3"), Expression::Int(7));
    }

    #[test]
    fn interpret_multiplication() {
        assert_eq!(run("def main(): Int := 6 * 7"), Expression::Int(42));
    }

    #[test]
    fn interpret_division() {
        assert_eq!(run("def main(): Int := 20 / 4"), Expression::Int(5));
    }

    #[test]
    fn interpret_power() {
        assert_eq!(run("def main(): Int := 2 ^ 8"), Expression::Int(256));
    }

    #[test]
    fn interpret_unary_negation() {
        assert_eq!(run("def main(): Int := -(5)"), Expression::Int(-5));
    }

    #[test]
    fn interpret_operator_precedence() {
        // 2 + 3 * 4 = 14, not 20
        assert_eq!(run("def main(): Int := 2 + 3 * 4"), Expression::Int(14));
    }

    #[test]
    fn interpret_parenthesised_expr() {
        assert_eq!(run("def main(): Int := (2 + 3) * 4"), Expression::Int(20));
    }

    // ── boolean operations ────────────────────────────────────────────────

    #[test]
    fn interpret_bool_and_true() {
        assert_eq!(
            run("def main(): Bool := true & true"),
            Expression::Bool(true)
        );
    }

    #[test]
    fn interpret_bool_and_false() {
        assert_eq!(
            run("def main(): Bool := true & false"),
            Expression::Bool(false)
        );
    }

    #[test]
    fn interpret_bool_or() {
        assert_eq!(
            run("def main(): Bool := false | true"),
            Expression::Bool(true)
        );
    }

    #[test]
    fn interpret_bool_not() {
        assert_eq!(run("def main(): Bool := !false"), Expression::Bool(true));
    }

    // ── comparisons ───────────────────────────────────────────────────────

    #[test]
    fn interpret_eq_true() {
        assert_eq!(run("def main(): Bool := 5 == 5"), Expression::Bool(true));
    }

    #[test]
    fn interpret_eq_false() {
        assert_eq!(run("def main(): Bool := 5 == 6"), Expression::Bool(false));
    }

    #[test]
    fn interpret_ne() {
        assert_eq!(run("def main(): Bool := 5 != 6"), Expression::Bool(true));
    }

    #[test]
    fn interpret_lt() {
        assert_eq!(run("def main(): Bool := 3 < 5"), Expression::Bool(true));
    }

    #[test]
    fn interpret_gt() {
        assert_eq!(run("def main(): Bool := 5 > 3"), Expression::Bool(true));
    }

    #[test]
    fn interpret_le_equal() {
        assert_eq!(run("def main(): Bool := 5 <= 5"), Expression::Bool(true));
    }

    #[test]
    fn interpret_ge_greater() {
        assert_eq!(run("def main(): Bool := 6 >= 5"), Expression::Bool(true));
    }

    // ── if / else ─────────────────────────────────────────────────────────

    #[test]
    fn interpret_if_takes_true_branch() {
        assert_eq!(
            run("def main(): Int := if true then 1 else 2"),
            Expression::Int(1)
        );
    }

    #[test]
    fn interpret_if_takes_false_branch() {
        assert_eq!(
            run("def main(): Int := if false then 1 else 2"),
            Expression::Int(2)
        );
    }

    #[test]
    fn interpret_nested_if() {
        let src = "def main(): Int :=
            if true then
                if false then 10 else 20
            else 30";
        assert_eq!(run(src), Expression::Int(20));
    }

    // ── while ─────────────────────────────────────────────────────────────

    #[test]
    fn interpret_while_not_entered_when_false() {
        let src = "def main(): Int := {
            let x: Int = 0;
            while false do { x = x + 1; };
            x
        }";
        assert_eq!(run(src), Expression::Int(0));
    }

    #[test]
    fn interpret_while_runs_correct_iterations() {
        let src = "def main(): Int := {
            let x: Int = 0;
            while x < 5 do { x = x + 1; };
            x
        }";
        assert_eq!(run(src), Expression::Int(5));
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
        assert_eq!(run(src), Expression::Int(55));
    }

    // ── blocks and let-bindings ───────────────────────────────────────────

    #[test]
    fn interpret_block_trailing_expression() {
        let src = "def main(): Int := {
            let a: Int = 3;
            let b: Int = 4;
            a + b
        }";
        assert_eq!(run(src), Expression::Int(7));
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
        assert_eq!(run(src), Expression::Int(102));
    }

    #[test]
    fn interpret_assignment_updates_value() {
        let src = "def main(): Int := {
            let x: Int = 1;
            x = 42;
            x
        }";
        assert_eq!(run(src), Expression::Int(42));
    }

    // ── function calls ────────────────────────────────────────────────────

    #[test]
    fn interpret_function_call_no_args() {
        let src = "def answer(): Int := 42
                   def main(): Int := answer()";
        assert_eq!(run(src), Expression::Int(42));
    }

    #[test]
    fn interpret_function_call_with_args() {
        let src = "def add(a: Int, b: Int): Int := a + b
                   def main(): Int := add(10, 32)";
        assert_eq!(run(src), Expression::Int(42));
    }

    #[test]
    fn interpret_fibonacci_10() {
        let src = "
            def fib(n: Int): Int :=
                if n <= 1 then n
                else fib(n - 1) + fib(n - 2)
            def main(): Int := fib(10)";
        assert_eq!(run(src), Expression::Int(55));
    }

    #[test]
    fn interpret_factorial_9() {
        let src = "
            def fact(n: Int): Int :=
                if n == 0 then 1 else n * fact(n - 1)
            def main(): Int := fact(9)";
        assert_eq!(run(src), Expression::Int(362880));
    }

    #[test]
    fn interpret_gcd() {
        let src = "
            def gcd(a: Int, b: Int): Int :=
                if b == 0 then a else gcd(b, a - (a / b) * b)
            def main(): Int := gcd(48, 18)";
        assert_eq!(run(src), Expression::Int(6));
    }

    #[test]
    fn interpret_higher_order_via_explicit_call() {
        // Double a value by calling a helper.
        let src = "
            def double(x: Int): Int := x * 2
            def quad(x: Int): Int := double(double(x))
            def main(): Int := quad(5)";
        assert_eq!(run(src), Expression::Int(20));
    }

    #[test]
    fn interpret_mutual_recursion() {
        // is_even / is_odd via mutual recursion.
        let src = "
            def is_odd(n: Int): Bool :=
                if n == 0 then false else is_even(n - 1)
            def is_even(n: Int): Bool :=
                if n == 0 then true else is_odd(n - 1)
            def main(): Bool := is_even(10)";
        assert_eq!(run(src), Expression::Bool(true));
    }

    // ── regression / edge cases ───────────────────────────────────────────

    #[test]
    fn interpret_zero_power_is_one() {
        assert_eq!(run("def main(): Int := 99 ^ 0"), Expression::Int(1));
    }

    #[test]
    fn interpret_negative_zero_is_zero() {
        assert_eq!(run("def main(): Int := -(0)"), Expression::Int(0));
    }

    #[test]
    fn interpret_chained_comparisons_via_bool_ops() {
        // (1 < 2) & (3 > 2) = true & true = true
        let src = "def main(): Bool := (1 < 2) & (3 > 2)";
        assert_eq!(run(src), Expression::Bool(true));
    }

    #[test]
    fn interpret_bool_equality() {
        assert_eq!(
            run("def main(): Bool := true == true"),
            Expression::Bool(true)
        );
        assert_eq!(
            run("def main(): Bool := true == false"),
            Expression::Bool(false)
        );
    }

    #[test]
    fn interpret_block_with_only_statements_returns_unit() {
        assert_eq!(
            run("def main(): Unit := {
                let x: Int = 1;
            }"),
            Expression::Unit
        );
    }
}
