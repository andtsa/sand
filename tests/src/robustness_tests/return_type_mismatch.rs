//! Tests for function return type validation
//!
//! Verifies that the type checker correctly validates return type expressions
//! against the declared function return type.

use crate::common::*;

/// Returning Bool from an Int function is a type error.
#[test]
fn returning_bool_from_int_function_is_error() {
    typecheck_fails("def main(): Int := true");
}

/// Returning Int from a Bool function is a type error.
#[test]
fn returning_int_from_bool_function_is_error() {
    typecheck_fails("def main(): Bool := 42");
}

/// An empty block (Unit) returned from an Int function must fail.
#[test]
fn empty_block_in_int_function_is_type_error() {
    typecheck_fails("def main(): Int := { }");
}

/// A block whose only content is statements (no trailing expr)
/// returns Unit; that must fail for a non-Unit return type.
#[test]
fn statement_only_block_in_int_function_is_type_error() {
    typecheck_fails(
        "def main(): Int := {
            let x: Int = 1;
        }",
    );
}
