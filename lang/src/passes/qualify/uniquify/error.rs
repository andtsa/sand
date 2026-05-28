//! error types for uniquify pass

use crate::compiler::structure::Range;

/// errors produced by the uniquify / reserved-name checking passes
#[derive(Debug)]
pub enum UniquifyError {
    UnboundVariable {
        name: String,
        at: Range,
    },
    UndefinedFunction {
        name: String,
        at: Range,
    },
    DuplicateFunction {
        name: String,
        first_instance: Range,
        second_instance: Range,
    },
    IllegalFunctionName {
        name: String,
        at: Range,
    },
    DuplicateParameterName {
        name: String,
        first_instance: Range,
        second_instance: Range,
    },
    DuplicateVariableName {
        name: String,
        first_instance: Range,
        second_instance: Range,
    },
}

impl std::fmt::Display for UniquifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use UniquifyError::*;
        match self {
            UnboundVariable { name, at } => {
                write!(f, "unbound variable '{name}' at {at}")
            }
            UndefinedFunction { name, at } => {
                write!(f, "undefined function '{name}' at {at}")
            }
            DuplicateFunction {
                name,
                first_instance,
                second_instance,
            } => write!(
                f,
                "duplicate function '{name}' at {first_instance} and {second_instance}"
            ),
            IllegalFunctionName { name, at } => {
                write!(f, "illegal function name '{name}' at {at}")
            }
            DuplicateParameterName {
                name,
                first_instance,
                second_instance,
            } => write!(
                f,
                "duplicate parameter '{name}' at {first_instance} and {second_instance}"
            ),
            DuplicateVariableName {
                name,
                first_instance,
                second_instance,
            } => write!(
                f,
                "duplicate variable '{name}' at {first_instance} and {second_instance}",
            ),
        }
    }
}

impl std::error::Error for UniquifyError {}
