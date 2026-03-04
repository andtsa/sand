//! errors for the ast typing pass

use std::error::Error;

use crate::lang::structure::Range;
use crate::lang::types::Ty;

#[derive(Debug)]
pub enum AstTypeError {
    UnboundVariable {
        name: String,
        range: Range,
    },
    UndefinedFunction {
        name: String,
        range: Range,
    },
    UniquifyError(crate::passes::uniquify::reserved::UniquifyError),
    TypeError {
        message: String,
        expected: Ty,
        found: Ty,
        range: Range,
    },
    FunctionCallTypeError {
        message: String,
        expected: Vec<Ty>,
        found: Vec<Ty>,
        range: Range,
    },
}

impl std::fmt::Display for AstTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use AstTypeError::*;
        match self {
            UnboundVariable { name, range } => {
                write!(f, "unbound variable '{name}' at {range}",)
            }
            UndefinedFunction { name, range } => {
                write!(f, "undefined function '{name}' at {range}",)
            }
            UniquifyError(e) => write!(f, "uniquify error: {}", e),
            TypeError {
                message,
                expected,
                found,
                range,
            } => {
                write!(
                    f,
                    "type error at {range}: {} (expected {:?}, found {:?})",
                    message, expected, found
                )
            }
            FunctionCallTypeError {
                message,
                expected,
                found,
                range,
            } => {
                write!(
                    f,
                    "function call type error at {range}: {} (expected argument types {:?}, found argument types {:?})",
                    message, expected, found
                )
            }
        }
    }
}

impl Error for AstTypeError {}
