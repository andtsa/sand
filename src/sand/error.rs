//! The user-facing error type for the Sand CLI

use sand::castles::project::init::FatalProjectCreationError;
use sand::util::fs::error::FsError;

use crate::compile::CompileCliError;

impl From<FsError> for CompileCliError {
    fn from(value: FsError) -> Self {
        CompileCliError::Fs(Box::new(value))
    }
}

impl From<FatalProjectCreationError> for CompileCliError {
    fn from(value: FatalProjectCreationError) -> Self {
        CompileCliError::ProjectInit(Box::new(value))
    }
}
