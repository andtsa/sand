//! Integration tests for example programs
//!
//! Verifies that canonical example programs compile and execute correctly,
//! producing expected outputs from both HIR and MIR interpreters.

use crate::common::*;

#[test]
fn fact() -> anyhow::Result<()> {
    let out = interpret_example("algorithms/fact")?;
    assert_eq!(out, lang::ir_types::typed_hir::Expression::Int(362880));
    Ok(())
}

#[test]
fn fib() -> anyhow::Result<()> {
    let out = interpret_example("algorithms/fib")?;
    assert_eq!(out, lang::ir_types::typed_hir::Expression::Int(55));
    Ok(())
}

#[test]
fn test_layers_rsa() {
    test_layers("algorithms/RSA");
}

#[test]
fn test_layers_prime() {
    test_layers("algorithms/prime");
}

#[test]
fn test_layers_fib() {
    test_layers("algorithms/fib");
}

#[test]
fn test_layers_fact() {
    test_layers("algorithms/fact");
}

#[test]
fn test_layers_gcd() {
    test_layers("algorithms/gcd");
}

// Helper for layer tests
fn test_layers(file: &str) {
    use std::hint::black_box;

    use lang::compiler::context::CompileCtx;
    use lang::ir_types::hhir::ProgramModule;
    use lang::ir_types::qhir;
    use lang::ir_types::typed_hir::TypedProgram;

    let code = open_example_from_file(file);
    let mut ctx = CompileCtx::initial();

    let p = ProgramModule::parse_stub(&mut ctx, &code).unwrap();
    let q = qhir::Program::combine(&mut ctx, vec![p]).unwrap();
    let (t, errors) = TypedProgram::from_ast_program(&mut ctx, q);
    assert!(errors.is_empty(), "type errors: {errors:?}");

    // Just verify it doesn't panic, the value itself doesn't matter for this test
    black_box(t);
}
