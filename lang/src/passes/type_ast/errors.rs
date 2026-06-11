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
    #[error("variant '#{variant}' takes no payload, but one was provided at {range}")]
    TagPayloadOnNullaryVariant { variant: String, range: Range },
    #[error("variant '#{variant}' expects a payload, but none was provided at {range}")]
    TagMissingPayload { variant: String, range: Range },
    #[error(
        "match scrutinee has type {ty} but match requires an enum, tuple, Int, or Bool type at {range}"
    )]
    MatchNonAggregateScrutinee { ty: Ty, range: Range },
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
    #[error(
        "constructor '{enum_name}#{variant}' at {range}: payload mismatch — variant {} a payload, but the call {}",
        if *expected_payload { "expects" } else { "does not expect" },
        if *expected_payload { "has none" } else { "supplies one" }
    )]
    ConstructorPayloadMismatch {
        enum_name: String,
        variant: String,
        /// `true` if the variant is declared with a payload (so the call is
        /// missing one); `false` if the variant is nullary (so the call's
        /// payload is unexpected).
        expected_payload: bool,
        range: Range,
    },
    #[error(
        "pattern '{enum_name}#{variant}' at {range}: payload mismatch — variant {} a payload, but the pattern {}",
        if *expected_payload { "carries" } else { "does not carry" },
        if *expected_payload { "doesn't destructure it" } else { "tries to destructure one" }
    )]
    PatternPayloadMismatch {
        enum_name: String,
        variant: String,
        /// `true` if the variant is declared with a payload (so the pattern
        /// should — but doesn't — destructure it); `false` if the variant is
        /// nullary (so the pattern's sub-pattern is unexpected).
        expected_payload: bool,
        range: Range,
    },
    #[error(
        "tuple pattern at {range} has {found} element(s) but the scrutinee type has {expected}"
    )]
    PatternArityMismatch {
        expected: usize,
        found: usize,
        range: Range,
    },
    #[error("pattern type error at {range}: {message}")]
    PatternTypeMismatch { message: String, range: Range },
    #[error(
        "literal pattern '{enum_name}::{variant}' at {range} cannot appear in a nested (payload/tuple) position — enum variant patterns, bindings, wildcards, and tuple-destructuring are all supported in nested position, but integer and boolean literals are not"
    )]
    RefutableNestedPattern {
        enum_name: String,
        variant: String,
        range: Range,
    },

    // ── let-pattern errors ──────────────────────────────────────
    #[error(
        "`let E#V(…) = …` at {range} requires an `else` branch because the pattern is refutable"
    )]
    LetPatternElseMissing { range: Range },

    #[error(
        "`let E#V(…) = … else fallback` at {range}: the sub-pattern inside the constructor must be irrefutable (only bindings, wildcards, and tuple-of-bindings are allowed); use `match` for nested refutable patterns"
    )]
    NestedVariantInLetPattern { range: Range },

    #[error(
        "`let E#V(…) = … else fallback` at {range}: the else expression must be a constructor of the same variant as the LHS pattern so that destructuring the fallback always succeeds"
    )]
    LetPatternElseNotIrrefutable { range: Range },
}
