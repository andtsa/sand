//! convert uniquify errors to SandDiagnostics

use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::DiagnosticSeverity;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::diagnostics::SdRelatedInfo;
use crate::compiler::structure::FileRef;
use crate::passes::qualify::uniquify::error::UniquifyError;

pub fn uniquify_error_to_diagnostics(
    _ctx: &CompileCtx,
    file: FileRef,
    err: &UniquifyError,
) -> SandDiagnostics {
    use UniquifyError::*;
    let mut diagnostics = SandDiagnostics::default();
    match err {
        UnboundVariable { name, at } => {
            let message = format!("unbound variable: {}", name);

            let related = SdRelatedInfo {
                file,
                range: *at,
                message: "no binding found for this variable".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *at,
                    file,
                    related: vec![related],
                    module: None,
                },
            );
        }

        UndefinedFunction { name, at } => {
            let message = format!("undefined function: {}", name);

            let related = SdRelatedInfo {
                file,
                range: *at,
                message: "no function with this name was found".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *at,
                    file,
                    related: vec![related],
                    module: None,
                },
            );
        }

        DuplicateFunction {
            name,
            first_instance,
            second_instance,
        } => {
            let message = format!("duplicate function: {}", name);

            let related = SdRelatedInfo {
                file,
                range: *first_instance,
                message: "first declaration is here".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *second_instance,
                    file,
                    related: vec![related],
                    module: None,
                },
            );
        }

        IllegalFunctionName { name, at } => {
            let message = format!("illegal function name: {}", name);

            let related = SdRelatedInfo {
                file,
                range: *at,
                message: "function name is reserved".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *at,
                    file,
                    related: vec![related],
                    module: None,
                },
            );
        }

        DuplicateParameterName {
            name,
            first_instance,
            second_instance,
        } => {
            let message = format!("duplicate parameter: {}", name);

            let related = SdRelatedInfo {
                file,
                range: *first_instance,
                message: "first parameter with this name is here".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *second_instance,
                    file,
                    related: vec![related],
                    module: None,
                },
            );
        }

        DuplicateVariableName {
            name,
            first_instance,
            second_instance,
        } => {
            let message = format!("duplicate variable: {}", name);

            let related = SdRelatedInfo {
                file,
                range: *first_instance,
                message: "first declaration is here".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *second_instance,
                    file,
                    related: vec![related],
                    module: None,
                },
            );
        }
    }

    diagnostics
}
