//! interpreters module

use std::process::ExitCode;

use crate::interpreter::mir::MirValue;
use crate::ir_types::typed_hir::Expression;

pub mod mir;
pub mod typed_hir;

/// What exit code should the interpreter process result in,
/// based on the expression returned by the main function
///
/// Note that only u8 exit codes are allowed by rust std,
/// so this cannot exactly mimic what happens in LLVM
pub fn thir_exit_code(expr: &Expression) -> ExitCode {
    match expr {
        Expression::Int(n) => ExitCode::from(*n as u8),
        Expression::Bool(b) => {
            if *b {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Expression::Unit => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

/// What exit code should the interpreter process result in,
/// based on the value returned by the main function.
///
/// Note that only u8 exit codes are allowed by rust std,
/// so this cannot exactly mimic what happens in LLVM
pub fn mir_exit_code(val: &MirValue) -> ExitCode {
    match val {
        MirValue::Int(n) => ExitCode::from(*n as u8),
        MirValue::Bool(b) => {
            if *b {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        MirValue::Unit => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
