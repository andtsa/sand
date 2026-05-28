//! Tests for MIR block ID correctness and control flow
//!
//! Verifies that MIR block numbering and patching is correct, and that
//! control flow is correctly executed for complex nested structures.

use crate::common::*;
use sand::interpreter::mir::MirValue;

/// Nested if-else with three levels — wrong block patching would
/// execute the wrong branch.
#[test]
fn nested_if_else_executes_correct_branch() {
    let src = "
        def classify(n: Int): Int :=
            if n < 0 then 0 - 1
            else if n == 0 then 0
            else 1
        def main(): Int := classify(5)";
    assert_eq!(run_mir(src), MirValue::Int(1));
    assert_eq!(
        run_mir(
            "def classify(n: Int): Int := if n < 0 then 0 - 1 else if n == 0 then 0 else 1  def main(): Int := classify(0 - 1)"
        ),
        MirValue::Int(-1)
    );
    assert_eq!(
        run_mir(
            "def classify(n: Int): Int := if n < 0 then 0 - 1 else if n == 0 then 0 else 1  def main(): Int := classify(0)"
        ),
        MirValue::Int(0)
    );
}

/// while-inside-if — exercises multiple levels of block reversal.
#[test]
fn while_inside_if_executes_correctly() {
    let src = "
        def f(flag: Bool): Int := {
            let result: Int = 0;
            if flag then {
                let i: Int = 0;
                while i < 4 do {
                    result = result + i;
                    i = i + 1;
                };
            } else { };
            result
        }
        def main(): Int := f(true)";
    // 0 + 1 + 2 + 3 = 6
    assert_eq!(run_mir(src), MirValue::Int(6));
}
