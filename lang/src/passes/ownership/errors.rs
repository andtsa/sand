//! errors produced by the ownership checker.

use thiserror::Error;

use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;

/// an ownership violation detected during the affine-type check
#[derive(Debug, Error)]
pub enum OwnershipError {
    #[error("use of moved value '{name}' at {used_at}; value was moved at {moved_at}")]
    UseAfterMove {
        name: String,
        moved_at: Range,
        used_at: Range,
    },

    #[error(
        "value '{name}' is moved inside a loop (at {moved_at}) with no guarantee \
         of re-initialization on every iteration (loop at {loop_range})"
    )]
    MoveInLoop {
        name: String,
        moved_at: Range,
        loop_range: Range,
    },

    #[error(
        "cannot borrow '{name}' {} at {range} because it is already borrowed {}; \
         a mutable borrow requires exclusive access (Calculus §1.2)",
        if *mutable { "as mutable" } else { "as immutable" },
        if *existing_mutable { "mutably" } else { "immutably" }
    )]
    ConflictingBorrow {
        name: String,
        /// whether the *new* borrow being introduced is mutable.
        mutable: bool,
        /// whether the *existing* (conflicting) borrow is mutable.
        existing_mutable: bool,
        range: Range,
    },

    #[error(
        "cannot move a non-`Copy` value out of a borrow at {range}: dereferencing \
         `&T`/`&mut T` only reads the value when `T` is `Copy` (Int, Bool, Unit)"
    )]
    MoveOutOfBorrow { range: Range },

    #[error(
        "cannot move '{name}' at {used_at} while it is borrowed: a borrow of '{name}' \
         is still live in this scope (Calculus §6.2 — a value may not be moved while \
         borrowed)"
    )]
    MoveWhileBorrowed { name: String, used_at: Range },
}

/// [`OwnershipError`] with the context of the source module it came from
#[derive(Debug)]
pub struct OwnershipCheckError<'tcx> {
    pub error: OwnershipError,
    pub module: ModuleRef<'tcx>,
}
