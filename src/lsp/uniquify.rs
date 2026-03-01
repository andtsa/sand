//! convert uniquify errors to LSP diagnostics

use tower_lsp::lsp_types::*;

use crate::lsp::util::position_from_line_col;
use crate::passes::uniquify::reserved::UniquifyError;

pub(super) fn uniquify_error_to_diagnostic(
    uri: &Url,
    text: &str,
    err: UniquifyError,
) -> Vec<Diagnostic> {
    use UniquifyError::*;
    match err {
        UnboundVariable { name, at } => {
            let ((sl, sc), (el, ec)) = at;
            let start_pos = position_from_line_col(text, sl, sc);
            let end_pos = position_from_line_col(text, el, ec);
            let range = Range::new(start_pos, end_pos);
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
                source: Some("kap".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        UndefinedFunction { name, at } => {
            let ((sl, sc), (el, ec)) = at;
            let start_pos = position_from_line_col(text, sl, sc);
            let end_pos = position_from_line_col(text, el, ec);
            let range = Range::new(start_pos, end_pos);
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
                source: Some("kap".into()),
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
            let ((fsl, fsc), (fel, fec)) = first_instance;
            let ((ssl, ssc), (sel, sec)) = second_instance;

            let first_start = position_from_line_col(text, fsl, fsc);
            let first_end = position_from_line_col(text, fel, fec);
            let first_range = Range::new(first_start, first_end);

            let second_start = position_from_line_col(text, ssl, ssc);
            let second_end = position_from_line_col(text, sel, sec);
            let second_range = Range::new(second_start, second_end);

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
                source: Some("kap".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }

        IllegalFunctionName { name, at } => {
            let ((sl, sc), (el, ec)) = at;
            let start_pos = position_from_line_col(text, sl, sc);
            let end_pos = position_from_line_col(text, el, ec);
            let range = Range::new(start_pos, end_pos);
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
                source: Some("kap".into()),
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
            let ((fsl, fsc), (fel, fec)) = first_instance;
            let ((ssl, ssc), (sel, sec)) = second_instance;

            let first_start = position_from_line_col(text, fsl, fsc);
            let first_end = position_from_line_col(text, fel, fec);
            let first_range = Range::new(first_start, first_end);

            let second_start = position_from_line_col(text, ssl, ssc);
            let second_end = position_from_line_col(text, sel, sec);
            let second_range = Range::new(second_start, second_end);

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
                source: Some("kap".into()),
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
            let ((fsl, fsc), (fel, fec)) = first_instance;
            let ((ssl, ssc), (sel, sec)) = second_instance;

            let first_start = position_from_line_col(text, fsl, fsc);
            let first_end = position_from_line_col(text, fel, fec);
            let first_range = Range::new(first_start, first_end);

            let second_start = position_from_line_col(text, ssl, ssc);
            let second_end = position_from_line_col(text, sel, sec);
            let second_range = Range::new(second_start, second_end);

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
                source: Some("kap".into()),
                message,
                related_information: Some(vec![related]),
                ..Default::default()
            }]
        }
    }
}
