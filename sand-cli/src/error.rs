//! The user-facing error type for the Sand CLI

use lang::castles::project::init::FatalProjectCreationError;
use lang::interpreter::mir::MirInterpError;
use lang::interpreter::typed_hir::InterpError;
use lang::passes::llvm_codegen::CodegenError;
use lang::util::fs::error::FsError;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("fs error: {0}")]
    Fs(Box<FsError>),
    #[error("error during project initialization: {0}")]
    ProjectInit(Box<FatalProjectCreationError>),
    #[error("compiler error: {diagnostic}")]
    CompilerError { diagnostic: String },
    #[error("llvm error: {0}")]
    Llvm(#[from] CodegenError),
    #[error(transparent)]
    HirInterpError(#[from] InterpError),
    #[error(transparent)]
    MirInterpError(#[from] MirInterpError),
}

impl From<FsError> for CliError {
    fn from(value: FsError) -> Self {
        CliError::Fs(Box::new(value))
    }
}

impl From<FatalProjectCreationError> for CliError {
    fn from(value: FatalProjectCreationError) -> Self {
        CliError::ProjectInit(Box::new(value))
    }
}
