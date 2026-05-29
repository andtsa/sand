//! turn AstTypeError to SandDiagnostics

use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::DiagnosticSeverity;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::diagnostics::SdRelatedInfo;
use crate::compiler::structure::FileRef;
use crate::passes::type_ast::AstTypeError;

pub fn type_error_to_diagnostic(
    _ctx: &CompileCtx,
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
            let diagnostic_message =
                format!("{} (expected {:?}, found {:?})", message, expected, found);

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: format!("expected type: {:?}, found type: {:?}", expected, found),
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
            let diagnostic_message =
                format!("{} (expected {:?}, found {:?})", message, expected, found);

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: format!(
                    "expected argument types: {:?}, found argument types: {:?}",
                    expected, found
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
        MatchNonEnumScrutinee { ty, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "match scrutinee has type {ty:?} — match requires an enum type"
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
    }
    diagnostics
}
