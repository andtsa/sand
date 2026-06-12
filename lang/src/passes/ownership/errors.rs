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
}

/// [`OwnershipError`] with the context of the source module it came from
#[derive(Debug)]
pub struct OwnershipCheckError<'tcx> {
    pub error: OwnershipError,
    pub module: ModuleRef<'tcx>,
}
