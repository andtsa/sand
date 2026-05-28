//! errors raised during qualify pass

use thiserror::Error;

use crate::compiler::structure::ModuleInfo;
use crate::compiler::structure::Range;
use crate::passes::qualify::uniquify::error::UniquifyError;

#[derive(Debug, Error)]
pub enum QualifyError {
    #[error("found two modules with the same name: {0}")]
    DuplicateModule(ModuleInfo),

    #[error(
        "found two functions with the same name: {name} at {first_instance} and {second_instance} in module {module}"
    )]
    DuplicateFunction {
        name: String,
        module: ModuleInfo,
        first_instance: Range,
        second_instance: Range,
    },

    #[error("error uniquifying module {module}: {source}")]
    UniquifyError {
        module: ModuleInfo,
        source: UniquifyError,
    },

    #[error("module {module} was not found")]
    ModuleNotFound {
        module: String,
        source_module: ModuleInfo,
        range: Range,
    },

    #[error("tried to call function {func} from module {module} that doesn't exist")]
    FunctionQualFailedModuleNotFound {
        func: String,
        module: String,
        source_module: ModuleInfo,
        range: Range,
    },

    // todo: add range for locating the offending function call
    #[error("could not find function {func} in module {module}")]
    FunctionQualFailedFunctionNotFound {
        func: String,
        module: ModuleInfo,
        source_module: ModuleInfo,
        range: Range,
    },

    #[error("encountered multiple main functions at {first} and {second} in module {first_module}")]
    DuplicateMain {
        first: Range,
        second: Range,
        first_module: ModuleInfo,
        second_module: ModuleInfo,
    },
}

impl QualifyError {
    pub fn source_module(&self) -> &ModuleInfo {
        match self {
            QualifyError::DuplicateModule(module) => module,
            QualifyError::DuplicateFunction { module, .. } => module,
            QualifyError::ModuleNotFound { source_module, .. } => source_module,
            QualifyError::FunctionQualFailedModuleNotFound { source_module, .. } => source_module,
            QualifyError::FunctionQualFailedFunctionNotFound { module, .. } => module,
            QualifyError::DuplicateMain { first_module, .. } => first_module,
            QualifyError::UniquifyError { module, .. } => module,
        }
    }
}
