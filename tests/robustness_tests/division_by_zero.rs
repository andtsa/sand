//! Tests for division by zero error handling
//!
//! Verifies that both HIR and MIR interpreters correctly detect and report
//! division by zero errors without panicking.

use crate::common::*;
use sand::interpreter::mir::MirInterpError;

/// MIR interpreter surfaces a clean DivisionByZero error.
#[test]
fn mir_division_by_zero_returns_error() {
    let result = run_mir_result("def main(): Int := 1 / 0");
    assert!(
        matches!(result, Err(MirInterpError::DivisionByZero)),
        "expected DivisionByZero, got: {:?}",
        result
    );
}

/// HIR interpreter must also surface a clean error, not panic.
/// Wrapping this in catch_unwind detects panics.
#[test]
fn hir_division_by_zero_does_not_panic() {
    use std::panic;
    let result = panic::catch_unwind(|| {
        let mut ctx = sand::compiler::context::CompileCtx::initial();
        let fr = ctx.stub_file();
        let code = sand::compiler::structure::Map::from([(fr, "def main(): Int := 1 / 0")]);
        let prog = sand::compile_hir(code, &mut ctx).expect("compile ok");
        let _ = prog.interpret(&ctx);
    });
    assert!(
        result.is_ok(),
        "HIR interpreter panicked on division by zero — should return Err instead"
    );
}

/// Division by zero via a variable (not a literal) — same contract.
#[test]
fn mir_runtime_division_by_zero_via_variable() {
    let result = run_mir_result(
        "def main(): Int := {
            let zero: Int = 0;
            10 / zero
        }",
    );
    assert!(
        matches!(result, Err(MirInterpError::DivisionByZero)),
        "expected DivisionByZero, got {:?}",
        result
    );
}
