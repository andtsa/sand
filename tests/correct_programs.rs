//! assert that well-typed programs succeed in all passes of the compiler

mod common;

use std::hint::black_box;

use common::open_example_from_file;
use sand::ir_types::hhir::Program;
use sand::ir_types::typed_hir::TypedProgram;

fn test_layers(file: &str) {
    let code = open_example_from_file(file);

    let p = Program::parse(&code).unwrap();
    let u = p.uniquify().unwrap();

    let t = TypedProgram::from_ast_program(&u).unwrap();

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
