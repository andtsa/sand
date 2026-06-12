//! Tests for explicit mutability annotations
//!
//! Verifies that `let x = 5` is immutable (assignment is a type error) and
//! that `let mut x = 5` allows subsequent assignment.

use lang::interpreter::mir::MirValue;

use crate::common::*;

/// Assigning to a plain `let` binding must fail type checking.
#[test]
fn assignment_to_immutable_let_is_type_error() {
    typecheck_fails(
        "def main(): Int := {
            let x: Int = 5;
            x = 6;
            x
        }",
    );
}

/// Assigning to a `let mut` binding must succeed and produce the new value.
#[test]
fn assignment_to_mutable_let_succeeds() {
    assert_eq!(
        run_mir(
            "def main(): Int := {
                let mut x: Int = 5;
                x = 6;
                x
            }"
        ),
        MirValue::Int(6)
    );
}

/// Multiple assignments to the same `mut` variable all take effect.
#[test]
fn repeated_assignment_to_mutable_let_succeeds() {
    assert_eq!(
        run_mir(
            "def main(): Int := {
                let mut x: Int = 0;
                x = 1;
                x = 2;
                x = 3;
                x
            }"
        ),
        MirValue::Int(3)
    );
}

/// Immutability check applies to Bool variables too.
#[test]
fn assignment_to_immutable_bool_is_type_error() {
    typecheck_fails(
        "def main(): Bool := {
            let flag: Bool = false;
            flag = true;
            flag
        }",
    );
}

/// Function parameters are immutable: assigning to one must fail.
#[test]
fn assignment_to_parameter_is_type_error() {
    typecheck_fails(
        "def f(x: Int): Int := {
            x = 99;
            x
        }
        def main(): Int := f(1)",
    );
}

/// Inferring the type of an immutable binding (no annotation) still forbids
/// reassignment.
#[test]
fn assignment_to_inferred_immutable_let_is_type_error() {
    typecheck_fails(
        "def main(): Int := {
            let x = 5;
            x = 6;
            x
        }",
    );
}

/// `let mut` with inferred type allows reassignment.
#[test]
fn assignment_to_inferred_mutable_let_succeeds() {
    assert_eq!(
        run_mir(
            "def main(): Int := {
                let mut x = 10;
                x = 20;
                x
            }"
        ),
        MirValue::Int(20)
    );
}

// ── mutable parameters
// ────────────────────────────────────────────────────────

/// A `mut` parameter can be reassigned inside the function body.
#[test]
fn mutable_parameter_can_be_reassigned() {
    assert_eq!(
        run_mir(
            "def f(mut x: Int): Int := { x = 99; x }
             def main(): Int := f(1)"
        ),
        MirValue::Int(99)
    );
}

/// A plain (immutable) parameter cannot be reassigned: type error.
#[test]
fn immutable_parameter_assignment_is_type_error() {
    typecheck_fails(
        "def f(x: Int): Int := { x = 99; x }
         def main(): Int := f(1)",
    );
}

/// Reassigning a `mut` parameter does not affect the caller's value.
#[test]
fn mutable_parameter_mutation_is_local() {
    assert_eq!(
        run_mir(
            "def bump(mut n: Int): Int := { n = n + 1; n }
             def main(): Int := {
                 let a: Int = 5;
                 let b: Int = bump(a);
                 a + b
             }"
        ),
        // a stays 5; b = 6; result = 11
        MirValue::Int(11)
    );
}

/// A `mut` parameter works alongside immutable ones in the same signature.
#[test]
fn mixed_mut_and_immutable_parameters() {
    assert_eq!(
        run_mir(
            "def f(x: Int, mut y: Int): Int := { y = y + x; y }
             def main(): Int := f(3, 10)"
        ),
        MirValue::Int(13)
    );
}

/// Assigning to the immutable parameter in a mixed signature still fails.
#[test]
fn assignment_to_immutable_in_mixed_signature_is_type_error() {
    typecheck_fails(
        "def f(x: Int, mut y: Int): Int := { x = 0; y }
         def main(): Int := f(1, 2)",
    );
}
