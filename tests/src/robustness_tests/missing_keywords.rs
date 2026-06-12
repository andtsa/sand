//! Tests for reserved keyword handling
//!
//! Verifies that reserved keywords (like `while`, `do`, `if`, `let`) cannot be
//! used as function or variable names.

use crate::common::*;

/// `while` is not properly guarded as a keyword, so this currently parses as a
/// function named `while`. After the grammar is fixed it should fail.
#[test]
fn while_cannot_be_used_as_function_name() {
    parse_fails("def while(): Int := 1  def main(): Int := while()");
}

#[test]
fn do_cannot_be_used_as_variable_name() {
    parse_fails(
        "def main(): Int := {
            let do: Int = 5;
            do
        }",
    );
}

/// Confirmed existing keywords are still rejected as identifiers.
#[test]
fn if_cannot_be_function_name() {
    parse_fails("def if(): Int := 1  def main(): Int := if()");
}

#[test]
fn let_cannot_be_function_name() {
    parse_fails("def let(): Int := 1  def main(): Int := let()");
}
