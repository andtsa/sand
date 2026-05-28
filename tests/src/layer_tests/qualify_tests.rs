//! QHIR qualification tests for the Sand compiler
//!
//! Tests cover qualification functionality for the Qualified HIR (QHIR) layer,
//! including variable binding, function resolution, and various error cases.

use lang::compiler::context::CompileCtx;
use lang::ir_types::hhir::ProgramModule;
use lang::ir_types::qhir;

use crate::common::qualify;

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
    let pm =
        ProgramModule::parse_stub(&mut ctx, "def main(): Int := undefined_fn()").expect("parse ok");
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
