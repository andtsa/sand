//! turn AstTypeError to SandDiagnostics

use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::DiagnosticSeverity;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::diagnostics::SdRelatedInfo;
use crate::compiler::structure::FileRef;
use crate::passes::type_ast::AstTypeError;

pub fn type_error_to_diagnostic(
    ctx: &CompileCtx,
    file: FileRef,
    err: &AstTypeError,
) -> SandDiagnostics {
    use crate::passes::type_ast::AstTypeError::*;
    let mut diagnostics = SandDiagnostics::default();
    match err {
        UnboundVariable { name, range } => {
            let message = format!("unbound variable '{}'", name);

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: "no binding found for this variable".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *range,
                    related: vec![related],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        ImmutableAssignment { name, range } => {
            let message = format!("cannot assign to immutable variable '{}'", name);

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: "variable is not declared with 'mut'".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *range,
                    related: vec![related],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        UndefinedFunction { name, range } => {
            let message = format!("undefined function '{}'", name);

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: "no function with this name was found".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *range,
                    related: vec![related],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        TypeError {
            message,
            expected,
            found,
            range,
        } => {
            let diagnostic_message = format!(
                "{} (expected {}, found {})",
                message,
                ctx.display_ty(*expected),
                ctx.display_ty(*found)
            );

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: format!(
                    "expected type: {}, found type: {}",
                    ctx.display_ty(*expected),
                    ctx.display_ty(*found)
                ),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: diagnostic_message,
                    range: *range,
                    related: vec![related],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        FunctionCallTypeError {
            message,
            expected,
            found,
            range,
        } => {
            let fmt_tys = |tys: &[_]| {
                tys.iter()
                    .map(|t| ctx.display_ty(*t).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let diagnostic_message = format!(
                "{} (expected [{}], found [{}])",
                message,
                fmt_tys(expected),
                fmt_tys(found)
            );

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: format!(
                    "expected argument types: [{}], found argument types: [{}]",
                    fmt_tys(expected),
                    fmt_tys(found),
                ),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: diagnostic_message,
                    range: *range,
                    related: vec![related],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        TagWithoutContext { variant, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "bare tag '#{variant}' cannot be used here: no expected type to resolve it against"
                    ),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        TagInNonEnumContext { variant, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "bare tag '#{variant}' used where a non-enum type was expected"
                    ),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        UnknownTagVariant {
            variant,
            enum_name,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!("unknown variant '{variant}' on enum type '{enum_name}'"),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        TagPayloadOnNullaryVariant { variant, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "variant '#{variant}' takes no payload, but a payload was provided"
                    ),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        TagMissingPayload { variant, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "variant '#{variant}' expects a payload, but none was provided"
                    ),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        MatchNonAggregateScrutinee { ty, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "match scrutinee has type {}; match requires an enum type",
                        ctx.display_ty(*ty)
                    ),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        NonExhaustiveMatch {
            enum_name,
            uncovered,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "match on '{enum_name}' is not exhaustive; missing variants: {}",
                        uncovered.join(", ")
                    ),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        DuplicateMatchPattern { pattern, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!("duplicate match pattern '{pattern}'"),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        UnreachableMatchArm { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message:
                        "unreachable match arm (appears after a wildcard or exhaustive pattern)"
                            .into(),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        MatchWrongEnumType {
            expected_enum,
            found_enum,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "match arm pattern is for enum '{found_enum}' but scrutinee has type '{expected_enum}'"
                    ),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        ConstructorPayloadMismatch {
            enum_name,
            variant,
            expected_payload,
            range,
        } => {
            let message = if *expected_payload {
                format!(
                    "constructor '{enum_name}#{variant}' expects a payload, but none was supplied"
                )
            } else {
                format!(
                    "constructor '{enum_name}#{variant}' does not take a payload, but one was supplied"
                )
            };
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        PatternPayloadMismatch {
            enum_name,
            variant,
            expected_payload,
            range,
        } => {
            let message = if *expected_payload {
                format!(
                    "pattern '{enum_name}#{variant}' should destructure its payload but doesn't (e.g. write '{enum_name}#{variant}(x)' or '{enum_name}#{variant}(_)')"
                )
            } else {
                format!(
                    "variant '{enum_name}#{variant}' does not carry a payload, but the pattern tries to destructure one"
                )
            };
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        PatternArityMismatch {
            expected,
            found,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "tuple pattern has {found} element(s) but the matched type has {expected}"
                    ),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        PatternTypeMismatch { message, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: message.clone(),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
        RefutableNestedPattern {
            enum_name,
            variant,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "literal pattern '{variant}' (of type '{enum_name}') cannot appear in a nested pattern position; enum variant patterns ('E#Variant(...)'), bindings ('x'), wildcards ('_'), and tuple-destructuring ('(a, b)') are permitted inside a payload or tuple element, but integer and boolean literals are not"
                    ),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }

        LetPatternElseMissing { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: "`let E#V(…) = …` requires an `else` branch because the pattern is refutable".to_string(),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }

        NestedVariantInLetPattern { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: "the sub-pattern inside a `let E#V(…)` constructor must be irrefutable (bindings, wildcards, tuple-of-bindings); use `match` for nested refutable patterns".to_string(),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }

        LetPatternElseNotIrrefutable { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: "the `else` expression must be a constructor of the same variant as the LHS pattern so that destructuring the fallback always succeeds".to_string(),
                    range: *range,
                    related: vec![],
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
    }
    diagnostics
}
