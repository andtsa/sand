//! Tests for duplicate parameter name handling
//!
//! Verifies that the compiler correctly rejects functions with duplicate parameter names.

use crate::common::*;

/// Two parameters with identical names should be rejected during
/// the uniquify pass
#[test]
fn qualify_rejects_duplicate_parameter_names() {
    qualify_fails(
        "def f(x: Int, x: Int): Int := x
         def main(): Int := f(1, 2)",
    );
}

/// Three params, all distinct — must still compile fine.
#[test]
fn three_distinct_params_accepted() {
    qualify(
        "def f(a: Int, b: Int, c: Int): Int := a + b + c
         def main(): Int := f(1, 2, 3)",
    );
}
