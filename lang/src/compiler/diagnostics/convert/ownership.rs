//! turn OwnershipError into SandDiagnostics.

use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::DiagnosticSeverity;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::diagnostics::SdRelatedInfo;
use crate::compiler::structure::FileRef;
use crate::passes::ownership::errors::OwnershipError;

pub fn ownership_error_to_diagnostic(
    _ctx: &CompileCtx,
    file: FileRef,
    err: &OwnershipError,
) -> SandDiagnostics {
    let mut diagnostics = SandDiagnostics::default();
    match err {
        OwnershipError::UseAfterMove {
            name,
            moved_at,
            used_at,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!("use of moved value '{name}'"),
                    range: *used_at,
                    related: vec![SdRelatedInfo {
                        file,
                        range: *moved_at,
                        message: format!("'{name}' was moved here"),
                    }],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        OwnershipError::MoveInLoop {
            name,
            moved_at,
            loop_range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "value '{name}' is moved inside a loop with no guarantee \
                         of re-initialization on every iteration"
                    ),
                    range: *loop_range,
                    related: vec![SdRelatedInfo {
                        file,
                        range: *moved_at,
                        message: format!("'{name}' is moved here"),
                    }],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
    }
    diagnostics
}
