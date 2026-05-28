//! Tests for while loop type checking
//!
//! Verifies that while loops, which always have type Unit, cannot be used where
//! non-Unit values are expected.

use crate::common::*;
use sand::interpreter::mir::MirValue;

/// A while loop used as a statement in a Unit function is fine.
#[test]
fn while_loop_in_unit_function_is_ok() {
    assert_eq!(
        run_mir(
            "def main(): Unit := {
                let i: Int = 0;
                while i < 3 do { i = i + 1; };
            }"
        ),
        MirValue::Unit
    );
}

/// Assigning the result of a while loop to an Int variable must fail.
#[test]
fn while_loop_result_used_as_int_is_type_error() {
    typecheck_fails(
        "def main(): Int := {
            let x: Int = while false do { };
            x
        }",
    );
}

/// Returning a while loop from an Int function must fail.
#[test]
fn while_loop_returned_as_int_is_type_error() {
    typecheck_fails("def main(): Int := while false do { }");
}
