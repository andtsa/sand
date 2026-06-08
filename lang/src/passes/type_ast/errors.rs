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
    #[error("type error at {range}: {message} (expected {expected}, found {found})")]
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
    #[error("match scrutinee has type {ty} but match requires an enum type at {range}")]
    MatchNonEnumScrutinee { ty: Ty, range: Range },
    #[error(
        "match on enum '{enum_name}' is not exhaustive at {range}; uncovered variants: {uncovered:?}"
    )]
    NonExhaustiveMatch {
        enum_name: String,
        uncovered: Vec<String>,
        range: Range,
    },
    #[error("duplicate match pattern '{pattern}' at {range}")]
    DuplicateMatchPattern { pattern: String, range: Range },
    #[error("unreachable match arm at {range} (appears after a wildcard or exhaustive pattern)")]
    UnreachableMatchArm { range: Range },
    #[error(
        "match arm pattern is for enum '{found_enum}' but scrutinee has type '{expected_enum}' at {range}"
    )]
    MatchWrongEnumType {
        expected_enum: String,
        found_enum: String,
        range: Range,
    },
}
