//! convert AstErrors into LSP diagnostics

use tower_lsp::lsp_types::*;

use crate::ast::AstError;
use crate::lsp::util::parse_error_to_diagnostic;
use crate::lsp::util::position_from_line_col;

/// convert an AstError into one or more lsp diagnostics
pub(super) fn ast_error_to_diagnostics(uri: &Url, text: &str, err: AstError) -> Vec<Diagnostic> {
    match err {
        AstError::Pest(parse_err) => vec![parse_error_to_diagnostic(text, parse_err)],

        AstError::UnexpectedRule {
            expected,
            got,
            start,
            end,
        } => {
            let start_pos = position_from_line_col(text, start.0, start.1);
            let end_pos = position_from_line_col(text, end.0, end.1);
            let range = Range::new(start_pos, end_pos);
            let message = format!("unexpected rule: expected {:?}, got {:?}", expected, got);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: format!("expected: {:?}, got: {:?}", expected, got),
            };

            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("kap".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        AstError::Missing {
            expected,
            start,
            end,
        } => {
            let start_pos = position_from_line_col(text, start.0, start.1);
            let end_pos = position_from_line_col(text, end.0, end.1);
            let range = Range::new(start_pos, end_pos);
            let message = format!("missing {}", expected);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "syntax may be incomplete here".into(),
            };

            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("kap".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        AstError::InvalidInteger { got, start, end } => {
            let start_pos = position_from_line_col(text, start.0, start.1);
            let end_pos = position_from_line_col(text, end.0, end.1);
            let range = Range::new(start_pos, end_pos);
            let message = format!("invalid integer literal: {}", got);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "integer literal must fit in i64 and contain only digits".into(),
            };

            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("kap".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        AstError::InvalidName { got, start, end } => {
            let start_pos = position_from_line_col(text, start.0, start.1);
            let end_pos = position_from_line_col(text, end.0, end.1);
            let range = Range::new(start_pos, end_pos);
            let message = format!("invalid name: {}", got);

            let related = DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range,
                },
                message: "name is reserved or otherwise invalid".into(),
            };

            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("kap".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }
    }
}
