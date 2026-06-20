//! convert uniquify errors to SandDiagnostics

use crate::compiler::context::CompileCtx;
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
            let related = SdRelatedInfo {
                file,
                range: *at,
                message: "no binding found for this variable".into(),
            };
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(file, *at, format!("unbound variable: {name}"), related),
            );
        }

        UndefinedFunction { name, at } => {
            let related = SdRelatedInfo {
                file,
                range: *at,
                message: "no function with this name was found".into(),
            };
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *at,
                    format!("undefined function: {name}"),
                    related,
                ),
            );
        }

        DuplicateFunction {
            name,
            first_instance,
            second_instance,
        } => {
            let related = SdRelatedInfo {
                file,
                range: *first_instance,
                message: "first declaration is here".into(),
            };
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *second_instance,
                    format!("duplicate function: {name}"),
                    related,
                ),
            );
        }

        IllegalFunctionName { name, at } => {
            let related = SdRelatedInfo {
                file,
                range: *at,
                message: "function name is reserved".into(),
            };
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *at,
                    format!("illegal function name: {name}"),
                    related,
                ),
            );
        }

        DuplicateParameterName {
            name,
            first_instance,
            second_instance,
        } => {
            let related = SdRelatedInfo {
                file,
                range: *first_instance,
                message: "first parameter with this name is here".into(),
            };
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *second_instance,
                    format!("duplicate parameter: {name}"),
                    related,
                ),
            );
        }

        DuplicateVariableName {
            name,
            first_instance,
            second_instance,
        } => {
            let related = SdRelatedInfo {
                file,
                range: *first_instance,
                message: "first declaration is here".into(),
            };
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *second_instance,
                    format!("duplicate variable: {name}"),
                    related,
                ),
            );
        }
        DuplicateBindingInPattern {
            name,
            first_instance,
            second_instance,
        } => {
            let related = SdRelatedInfo {
                file,
                range: *first_instance,
                message: "first bound here".into(),
            };
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *second_instance,
                    format!("identifier '{name}' bound more than once in the same pattern"),
                    related,
                ),
            );
        }
    }

    diagnostics
}
