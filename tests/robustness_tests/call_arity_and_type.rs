//! Tests for function call arity and argument type validation
//!
//! Verifies that the type checker correctly validates function call argument counts
//! and types, rejecting mismatches.

use crate::common::*;
use sand::interpreter::mir::MirValue;

/// Too few arguments must be a compile error.
#[test]
fn too_few_arguments_is_error() {
    typecheck_fails(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(1)",
    );
}

/// Too many arguments must be a compile error.
#[test]
fn too_many_arguments_is_error() {
    typecheck_fails(
        "def id(x: Int): Int := x
         def main(): Int := id(1, 2)",
    );
}

/// Passing a Bool where an Int is expected must fail type-check.
#[test]
fn bool_passed_as_int_argument_is_type_error() {
    typecheck_fails(
        "def inc(x: Int): Int := x + 1
         def main(): Int := inc(true)",
    );
}

/// Passing an Int where a Bool is expected must fail type-check.
#[test]
fn int_passed_as_bool_argument_is_type_error() {
    typecheck_fails(
        "def neg(b: Bool): Bool := !b
         def main(): Bool := neg(42)",
    );
}

/// Zero-argument call to a zero-parameter function is fine.
#[test]
fn zero_arg_call_is_ok() {
    assert_eq!(
        run_mir("def answer(): Int := 42  def main(): Int := answer()"),
        MirValue::Int(42)
    );
}
