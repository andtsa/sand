//! convert uniquify errors to LSP diagnostics

use tower_lsp::lsp_types::*;

use crate::lsp::util::lsp_range_from_pest;
use crate::passes::uniquify::reserved::UniquifyError;

pub(super) fn uniquify_error_to_diagnostic(
    uri: &Url,
    text: &str,
    err: UniquifyError,
) -> Vec<Diagnostic> {
    use UniquifyError::*;
    match err {
        UnboundVariable { name, at } => {
            let range = lsp_range_from_pest(text, at);
            let message = format!("unbound variable: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "no binding found for this variable".into(),
            };

            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        UndefinedFunction { name, at } => {
            let range = lsp_range_from_pest(text, at);
            let message = format!("undefined function: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "no function with this name was found".into(),
            };

            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        DuplicateFunction {
            name,
            first_instance,
            second_instance,
        } => {
            let first_range = lsp_range_from_pest(text, first_instance);
            let second_range = lsp_range_from_pest(text, second_instance);

            let message = format!("duplicate function: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: first_range,
                },
                message: "first declaration is here".into(),
            };

            vec![Diagnostic {
                range: second_range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        IllegalFunctionName { name, at } => {
            let range = lsp_range_from_pest(text, at);
            let message = format!("illegal function name: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "function name is reserved".into(),
            };

            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        DuplicateParameterName {
            name,
            first_instance,
            second_instance,
        } => {
            let first_range = lsp_range_from_pest(text, first_instance);
            let second_range = lsp_range_from_pest(text, second_instance);

            let message = format!("duplicate parameter: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: first_range,
                },
                message: "first parameter with this name is here".into(),
            };

            vec![Diagnostic {
                range: second_range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        DuplicateVariableName {
            name,
            first_instance,
            second_instance,
        } => {
            let first_range = lsp_range_from_pest(text, first_instance);
            let second_range = lsp_range_from_pest(text, second_instance);

            let message = format!("duplicate variable: {}", name);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: first_range,
                },
                message: "first declaration is here".into(),
            };

            vec![Diagnostic {
                range: second_range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }
    }
}
