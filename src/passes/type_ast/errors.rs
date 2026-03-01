//! errors for the ast typing pass

use std::error::Error;

use crate::lang::types::Ty;

#[derive(Debug)]
pub enum AstTypeError {
    UnboundVariable {
        name: String,
        start: (usize, usize),
        end: (usize, usize),
    },
    UndefinedFunction {
        name: String,
        start: (usize, usize),
        end: (usize, usize),
    },
    UniquifyError(crate::passes::uniquify::reserved::UniquifyError),
    TypeError {
        message: String,
        expected: Ty,
        found: Ty,
        start: (usize, usize),
        end: (usize, usize),
    },
    FunctionCallTypeError {
        message: String,
        expected: Vec<Ty>,
        found: Vec<Ty>,
        start: (usize, usize),
        end: (usize, usize),
    },
}

impl std::fmt::Display for AstTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use AstTypeError::*;
        match self {
            UnboundVariable { name, start, end } => {
                write!(
                    f,
                    "unbound variable '{}' at span {:?}-{:?}",
                    name, start, end
                )
            }
            UndefinedFunction { name, start, end } => {
                write!(
                    f,
                    "undefined function '{}' at span {:?}-{:?}",
                    name, start, end
                )
            }
            UniquifyError(e) => write!(f, "uniquify error: {}", e),
            TypeError {
                message,
                expected,
                found,
                start,
                end,
            } => {
                write!(
                    f,
                    "type error at {:?}-{:?}: {} (expected {:?}, found {:?})",
                    start, end, message, expected, found
                )
            }
            FunctionCallTypeError {
                message,
                expected,
                found,
                start,
                end,
            } => {
                write!(
                    f,
                    "function call type error at {:?}-{:?}: {} (expected argument types {:?}, found argument types {:?})",
                    start, end, message, expected, found
                )
            }
        }
    }
}

impl Error for AstTypeError {}
