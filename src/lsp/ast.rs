//! convert AstErrors into LSP diagnostics

use tower_lsp::lsp_types::*;

use crate::lsp::util::lsp_range_from_pest;
use crate::lsp::util::parse_error_to_diagnostic;
use crate::passes::build_ast::AstError;

/// convert an AstError into one or more lsp diagnostics
pub(super) fn ast_error_to_diagnostics(uri: &Url, text: &str, err: AstError) -> Vec<Diagnostic> {
    match err {
        AstError::Pest(parse_err) => vec![parse_error_to_diagnostic(text, parse_err)],

        AstError::UnexpectedRule {
            expected,
            got,
            range,
        } => {
            let range = lsp_range_from_pest(text, range);
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
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        AstError::Missing { expected, range } => {
            let range = lsp_range_from_pest(text, range);
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
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        AstError::InvalidInteger { got, range } => {
            let range = lsp_range_from_pest(text, range);
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
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        AstError::InvalidName { got, range } => {
            let range = lsp_range_from_pest(text, range);
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
                source: Some("sand".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }
    }
}
