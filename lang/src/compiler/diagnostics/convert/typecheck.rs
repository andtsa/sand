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
    }
    diagnostics
}
