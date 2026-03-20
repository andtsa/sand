//! assert that well-typed programs succeed in all passes of the compiler

mod common;

use std::hint::black_box;

use common::open_example_from_file;
use sand::compiler::context::CompileCtx;
use sand::ir_types::hhir::ProgramModule;
use sand::ir_types::qhir;
use sand::ir_types::typed_hir::TypedProgram;

fn test_layers(file: &str) {
    let code = open_example_from_file(file);
    let mut ctx = CompileCtx::initial();

    let p = ProgramModule::parse_stub(&mut ctx, &code).unwrap();
    let q = qhir::Program::combine(&mut ctx, vec![p]).unwrap();
    let t = TypedProgram::from_ast_program(&mut ctx, q).unwrap();

    // println!("{t:?}");
    black_box(t);
}

#[test]
fn test_rsa() {
    test_layers("RSA");
}

#[test]
fn test_prime() {
    test_layers("prime");
}

#[test]
fn test_fib() {
    test_layers("fib");
}

#[test]
fn test_fact() {
    test_layers("fact");
}

#[test]
fn test_gcd() {
    test_layers("gcd");
}
