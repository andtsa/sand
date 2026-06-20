//! turn OwnershipError into SandDiagnostics.

use crate::compiler::context::CompileCtx;
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
            is_clone,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *used_at,
                    if *is_clone {
                        format!("use of moved value '{name}' (use `clone(&{name})` to keep a copy)")
                    } else {
                        format!("use of moved value '{name}'")
                    },
                    SdRelatedInfo {
                        file,
                        range: *moved_at,
                        message: format!("'{name}' was moved here"),
                    },
                ),
            );
        }
        OwnershipError::MoveInLoop {
            name,
            moved_at,
            loop_range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *loop_range,
                    format!(
                        "value '{name}' is moved inside a loop with no guarantee \
                         of re-initialization on every iteration"
                    ),
                    SdRelatedInfo {
                        file,
                        range: *moved_at,
                        message: format!("'{name}' is moved here"),
                    },
                ),
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
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "cannot borrow '{name}' {new_kind}: it is already borrowed {old_kind} \
                         (a mutable borrow requires exclusive access)"
                    ),
                ),
            );
        }
        OwnershipError::MoveOutOfBorrow { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    "cannot move a non-`Copy` value out of a borrow: dereferencing \
                     only reads the value when its type is `Copy`"
                        .to_string(),
                ),
            );
        }
        OwnershipError::MoveWhileBorrowed { name, used_at } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *used_at,
                    format!(
                        "cannot move '{name}' while it is borrowed: a borrow of '{name}' is \
                         still live in this scope"
                    ),
                ),
            );
        }
    }
    diagnostics
}
