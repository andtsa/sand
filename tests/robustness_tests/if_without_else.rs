//! Tests for if expressions without else clauses
//!
//! Verifies that `if` without `else` is only allowed in Unit contexts, and that
//! using it where a non-Unit type is expected fails type checking.

use crate::common::*;

/// `if` without `else` as a statement (Unit context) is fine.
#[test]
fn if_without_else_as_statement_is_ok() {
    let _ = run_mir(
        "def main(): Unit := {
            let x: Int = 0;
            if x == 0 then { x = 1; };
        }",
    );
}

/// Using an `if`-without-`else` where an Int is expected must fail
/// type-checking because the implicit else-branch produces Unit.
#[test]
fn if_without_else_used_as_int_is_type_error() {
    typecheck_fails("def main(): Int := if true then 1");
}

/// Assigning an `if`-without-`else` to an Int variable must fail.
#[test]
fn if_without_else_assigned_to_int_is_type_error() {
    typecheck_fails(
        "def main(): Int := {
            let x: Int = if true then 5;
            x
        }",
    );
}
