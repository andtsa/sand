//! turn AstTypeError to diagnostics

use tower_lsp::lsp_types::*;

use crate::lsp::util::lsp_range_from_pest;
use crate::passes::type_ast::AstTypeError;

pub fn type_error_to_diagnostic(uri: &Url, text: &str, err: AstTypeError) -> Vec<Diagnostic> {
    use crate::passes::type_ast::AstTypeError::*;
    match err {
        UnboundVariable { name, range } => {
            let range = lsp_range_from_pest(text, range);
            let message = format!("unbound variable '{}'", name);

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
        UndefinedFunction { name, range } => {
            let range = lsp_range_from_pest(text, range);
            let message = format!("undefined function '{}'", name);

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
        UniquifyError(e) => vec![Diagnostic {
            range: Range::default(),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("sand".into()),
            message: format!("uniquify error: {}", e),
            ..Default::default()
        }],
        TypeError {
            message,
            expected,
            found,
            range,
        } => {
            let range = lsp_range_from_pest(text, range);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: format!("expected type: {:?}, found type: {:?}", expected, found),
            };

            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sand".into()),
                message: format!("{} (expected {:?}, found {:?})", message, expected, found),
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }
        FunctionCallTypeError {
            message,
            expected,
            found,
            range,
        } => {
            let range = lsp_range_from_pest(text, range);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: format!(
                    "expected argument types: {:?}, found argument types: {:?}",
                    expected, found
                ),
            };

            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sand".into()),
                message: format!("{} (expected {:?}, found {:?})", message, expected, found),
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }
    }
}
