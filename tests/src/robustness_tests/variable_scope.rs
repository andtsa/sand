//! Tests for variable scoping and shadowing
//!
//! Verifies that variables declared in nested scopes (branches, blocks) are not
//! visible outside those scopes, and that shadowed variables don't mutate outer
//! bindings.

use lang::interpreter::mir::MirValue;

use crate::common::*;

/// Variable declared inside `then` branch is not visible after the if
/// expression.
#[test]
fn variable_in_then_branch_is_not_visible_after_if() {
    qualify_fails(
        "def main(): Int := {
            if true then { let secret: Int = 42; } else { };
            secret
        }",
    );
}

/// Variable declared inside a nested block is not visible outside.
#[test]
fn variable_in_nested_block_is_not_visible_outside() {
    qualify_fails(
        "def main(): Int := {
            let _ : Int = {
                let inner: Int = 7;
                inner
            };
            inner
        }",
    );
}

/// Variable declared inside `else` branch is not visible after.
#[test]
fn variable_in_else_branch_not_visible_after_if() {
    qualify_fails(
        "def main(): Int := {
            if false then { } else { let hidden: Int = 9; };
            hidden
        }",
    );
}

/// The outer variable with the same name as a shadowed inner one
/// retains its original value after the block exits.
#[test]
fn shadow_does_not_mutate_outer_binding() {
    let result = run_mir(
        "def main(): Int := {
            let a: Int = 1;
            let _ignored: Int = {
                let a: Int = 99;
                a
            };
            a
        }",
    );
    let expected = MirValue::Int(1);
    assert_eq!(
        result, expected,
        "shadowed variable mutates outer binding, was {result:?} should have been {expected:?}"
    );
}
