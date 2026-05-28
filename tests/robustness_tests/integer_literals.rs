//! Tests for integer literal range validation
//!
//! Verifies that the compiler correctly rejects integer literals outside the i64 range
//! and accepts those within range.

use crate::common::*;

/// A value right at i64::MAX should be accepted.
#[test]
fn i64_max_literal_is_accepted() {
    parse(&format!("def main(): Int := {}", i64::MAX));
}

/// A value one above i64::MAX must be rejected at parse time.
#[test]
fn one_above_i64_max_is_rejected() {
    // 9223372036854775808  =  i64::MAX + 1
    parse_fails("def main(): Int := 9223372036854775808");
}

/// A ridiculously large literal must also be rejected.
#[test]
fn huge_literal_is_rejected() {
    parse_fails("def main(): Int := 99999999999999999999999999999999999999");
}

/// Zero is still fine.
#[test]
fn zero_is_accepted() {
    parse("def main(): Int := 0");
}
