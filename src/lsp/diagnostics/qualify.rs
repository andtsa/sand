//! convert qualify errors to LSP diagnostics

use tower_lsp::lsp_types::*;

use crate::compiler::context::CompileCtx;
use crate::lsp::diagnostics::Diagnostics;
use crate::lsp::diagnostics::uniquify::uniquify_error_to_diagnostic;
use crate::lsp::util::lsp_range_from_pest;
use crate::passes::qualify::error::QualifyError;

pub fn qualify_error_to_diagnostics(
    ctx: &CompileCtx,
    uri: Url,
    text: &str,
    err: QualifyError,
) -> Diagnostics {
    let mut diagnostics = Diagnostics::default();
    match err {
        QualifyError::DuplicateFunction {
            name,
            module,
            first_instance,
            second_instance,
        } => {
            // these two functions are in the same module, so the
            // DiagnosticRelatedInformation can use the same file URL
            diagnostics.add_one(
                uri.clone(),
                Diagnostic {
                    range: lsp_range_from_pest(text, first_instance),
                    message: format!("function '{name}' is already defined in this module"),
                    source: Some(format!("error in module {module}")),
                    ..Default::default()
                },
            );
            diagnostics.add_one(
                uri,
                Diagnostic {
                    range: lsp_range_from_pest(text, second_instance),
                    message: format!("function '{name}' is already defined in this module",),
                    source: Some(format!("error in module {module}")),
                    ..Default::default()
                },
            );
        }
        QualifyError::DuplicateMain {
            first,
            second,
            first_module,
            second_module,
        } => {
            let links = vec![
                DiagnosticRelatedInformation {
                    location: Location {
                        uri: ctx.url_of_module(first_module.index),
                        range: lsp_range_from_pest(text, first),
                    },
                    message: "first main function is here".to_string(),
                },
                DiagnosticRelatedInformation {
                    location: Location {
                        uri: ctx.url_of_module(second_module.index),
                        range: lsp_range_from_pest(text, second),
                    },
                    message: "second main function is here".to_string(),
                },
            ];
            diagnostics.add_one(uri.clone(), Diagnostic {
                range: lsp_range_from_pest(text, first),
                message: "main function is already defined! you can only have one main function per project.".to_string(),
                related_information: Some(links.clone()),
                ..Default::default()
            });
            diagnostics.add_one(uri, Diagnostic {
                range: lsp_range_from_pest(text, second),
                message: "main function is already defined! you can only have one main function per project.".to_string(),
                related_information: Some(links),
                ..Default::default()
            });
        }

        // todo: keep track in which files each module was declared
        QualifyError::DuplicateModule(dm) => {
            diagnostics.add_one(
                uri,
                Diagnostic {
                    message: format!("module '{}' is already defined", dm.name),
                    source: Some(format!("error in module {}", dm.name)),
                    ..Default::default()
                },
            );
        }

        QualifyError::FunctionQualFailedFunctionNotFound {
            func,
            module,
            range,
        } => {
            diagnostics.add_one(
                uri,
                Diagnostic {
                    range: lsp_range_from_pest(text, range),
                    message: format!(
                        "function '{}' is not defined in module '{}'",
                        func, module.name
                    ),
                    source: Some(format!("error in module {}", module.name)),
                    ..Default::default()
                },
            );
        }

        QualifyError::FunctionQualFailedModuleNotFound {
            func,
            module,
            source_module,
            range,
        } => {
            diagnostics.add_one(
                uri,
                Diagnostic {
                    range: lsp_range_from_pest(text, range),
                    message: format!("module '{}' is not found for function '{}'", module, func),
                    source: Some(format!("error in module {}", source_module.name)),
                    ..Default::default()
                },
            );
        }

        QualifyError::UniquifyError { module: _, source } => {
            return uniquify_error_to_diagnostic(ctx, uri, text, source);
        }

        QualifyError::ModuleNotFound {
            module,
            source_module,
        } => {
            diagnostics.add_one(
                uri,
                Diagnostic {
                    message: format!("module '{}' is not found", module),
                    source: Some(format!("error in module {}", source_module.name)),
                    ..Default::default()
                },
            );
        }
    }
    diagnostics
}
