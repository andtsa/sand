//! errors for the ast typing pass

use thiserror::Error;

use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::lang::types::Ty;

#[derive(Debug)]
pub struct TypeError {
    pub error: AstTypeError,
    pub module: ModuleRef,
}

#[derive(Debug, Error)]
pub enum AstTypeError {
    #[error("unbound variable '{name}' at {range}")]
    UnboundVariable { name: String, range: Range },
    #[error("cannot assign to immutable variable '{name}' at {range}")]
    ImmutableAssignment { name: String, range: Range },
    #[error("undefined function '{name}' at {range}")]
    UndefinedFunction { name: String, range: Range },
    #[error("type error at {range}: {message} (expected {expected:?}, found {found:?})")]
    TypeError {
        message: String,
        expected: Ty,
        found: Ty,
        range: Range,
    },
    #[error(
        "function call type error at {range}: {message} (expected argument types {expected:?}, found argument types {found:?})"
    )]
    FunctionCallTypeError {
        message: String,
        expected: Vec<Ty>,
        found: Vec<Ty>,
        range: Range,
    },
    #[error("bare tag '#{variant}' used without an expected type context at {range}")]
    TagWithoutContext { variant: String, range: Range },
    #[error("bare tag '#{variant}' used where a non-enum type was expected at {range}")]
    TagInNonEnumContext { variant: String, range: Range },
    #[error("unknown variant '{variant}' on enum '{enum_name}' at {range}")]
    UnknownTagVariant {
        variant: String,
        enum_name: String,
        range: Range,
    },
}
