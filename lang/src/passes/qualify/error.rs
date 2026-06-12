//! errors raised during qualify pass

use thiserror::Error;

use crate::compiler::structure::ModuleInfo;
use crate::compiler::structure::Range;
use crate::passes::qualify::uniquify::error::UniquifyError;

#[derive(Debug, Error)]
pub enum QualifyError<'tcx> {
    #[error("found two modules with the same name: {0}")]
    DuplicateModule(ModuleInfo<'tcx>),

    #[error(
        "found two functions with the same name: {name} at {first_instance} and {second_instance} in module {module}"
    )]
    DuplicateFunction {
        name: String,
        module: ModuleInfo<'tcx>,
        first_instance: Range,
        second_instance: Range,
    },

    #[error("error uniquifying module {module}: {source}")]
    UniquifyError {
        module: ModuleInfo<'tcx>,
        source: UniquifyError,
    },

    #[error("module {module} was not found")]
    ModuleNotFound {
        module: String,
        source_module: ModuleInfo<'tcx>,
        range: Range,
    },

    #[error("tried to call function {func} from module {module} that doesn't exist")]
    FunctionQualFailedModuleNotFound {
        func: String,
        module: String,
        source_module: ModuleInfo<'tcx>,
        range: Range,
    },

    // todo: add range for locating the offending function call
    #[error("could not find function {func} in module {module}")]
    FunctionQualFailedFunctionNotFound {
        func: String,
        module: ModuleInfo<'tcx>,
        source_module: ModuleInfo<'tcx>,
        range: Range,
    },

    #[error("encountered multiple main functions at {first} and {second} in module {first_module}")]
    DuplicateMain {
        first: Range,
        second: Range,
        first_module: ModuleInfo<'tcx>,
        second_module: ModuleInfo<'tcx>,
    },

    #[error("unknown enum type '{name}' used in constructor expression at {range}")]
    UnknownConstructorType {
        name: String,
        range: Range,
        source_module: ModuleInfo<'tcx>,
    },

    #[error("unknown variant '{variant}' on enum type '{type_name}' at {range}")]
    UnknownVariant {
        type_name: String,
        variant: String,
        range: Range,
        source_module: ModuleInfo<'tcx>,
    },

    #[error("unknown enum type '{name}' used in match pattern at {range}")]
    UnknownPatternType {
        name: String,
        range: Range,
        source_module: ModuleInfo<'tcx>,
    },
}

impl<'tcx> QualifyError<'tcx> {
    pub fn source_module(&self) -> &ModuleInfo<'tcx> {
        match self {
            QualifyError::DuplicateModule(module) => module,
            QualifyError::DuplicateFunction { module, .. } => module,
            QualifyError::ModuleNotFound { source_module, .. } => source_module,
            QualifyError::FunctionQualFailedModuleNotFound { source_module, .. } => source_module,
            QualifyError::FunctionQualFailedFunctionNotFound { module, .. } => module,
            QualifyError::DuplicateMain { first_module, .. } => first_module,
            QualifyError::UniquifyError { module, .. } => module,
            QualifyError::UnknownConstructorType { source_module, .. } => source_module,
            QualifyError::UnknownVariant { source_module, .. } => source_module,
            QualifyError::UnknownPatternType { source_module, .. } => source_module,
        }
    }
}
