//! turn AstTypeError to diagnostics

use tower_lsp::lsp_types::*;

use crate::{lsp::util::position_from_line_col, passes::type_ast::AstTypeError};

pub fn type_error_to_diagnostic(uri: &Url, text: &str, err: AstTypeError) -> Vec<Diagnostic> {
    use crate::passes::type_ast::AstTypeError::*;
    match err {
        UnboundVariable { name, start, end } => {
            let start_pos = position_from_line_col(text, start.0, start.1);
            let end_pos = position_from_line_col(text, end.0, end.1);
            let range = Range::new(start_pos, end_pos);
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
        UndefinedFunction { name, start, end } => {
            let start_pos = position_from_line_col(text, start.0, start.1);
            let end_pos = position_from_line_col(text, end.0, end.1);
            let range = Range::new(start_pos, end_pos);
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
            start,
            end,
        } => {
            let start_pos = position_from_line_col(text, start.0, start.1);
            let end_pos = position_from_line_col(text, end.0, end.1);
            let range = Range::new(start_pos, end_pos);
            
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
                message: format!(
                    "{} (expected {:?}, found {:?})",
                    message, expected, found
                ),
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }
        FunctionCallTypeError {
            message,
            expected,
            found,
            start,
            end,
        } => {
            let start_pos = position_from_line_col(text, start.0, start.1);
            let end_pos = position_from_line_col(text, end.0, end.1);
            let range = Range::new(start_pos, end_pos);
            
            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: format!("expected argument types: {:?}, found argument types: {:?}", expected, found),
            };
            
            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sand".into()),
                message: format!(
                    "{} (expected {:?}, found {:?})",
                    message, expected, found
                ),
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }
    }
}
