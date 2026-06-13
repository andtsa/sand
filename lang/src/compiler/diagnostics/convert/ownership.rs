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
        OwnershipError::ConflictingBorrow {
            name,
            mutable,
            existing_mutable,
            range,
        } => {
            let new_kind = if *mutable { "mutably" } else { "immutably" };
            let old_kind = if *existing_mutable {
                "mutably"
            } else {
                "immutably"
            };
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "cannot borrow '{name}' {new_kind}: it is already borrowed {old_kind} \
                         (a mutable borrow requires exclusive access)"
                    ),
                    range: *range,
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        OwnershipError::MoveOutOfBorrow { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: "cannot move a non-`Copy` value out of a borrow: dereferencing \
                              only reads the value when its type is `Copy`"
                        .to_string(),
                    range: *range,
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        OwnershipError::MoveWhileBorrowed { name, used_at } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "cannot move '{name}' while it is borrowed: a borrow of '{name}' is \
                         still live in this scope"
                    ),
                    range: *used_at,
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
    }
    diagnostics
}
