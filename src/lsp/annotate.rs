//! generate diagnostics for expression annotation

use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DiagnosticSeverity;
use tower_lsp::lsp_types::Range;

use crate::ProgramAnnotations;
use crate::analyse;
use crate::lang::Program;
use crate::lsp::util::position_from_line_col;

/// if an expression without side effects appears multiple times in the code,
/// we can compute its value just once,
/// and reuse everywhere else.
///
/// we want to show this to the user by highlighting the repeated expressions:
///
/// ```ignore
/// def main(): Int := {
///     let x: Int = 5;
///     let y: Int = x ^ x;
///     let i: Int = 4;
///     while i ≥ 0 do {
///         y = y + (x ^ x);
///         i = i - 1;
///     }
///     y
/// }
/// ```
/// in this example, both instances of `x ^ x` should be highlighted,
/// indicating a reused value
pub fn annotate_reused_expressions(text: &str, ast: Program) -> Vec<Diagnostic> {
    let annotations: ProgramAnnotations = match analyse(&ast) {
        Ok(a) => a,
        Err(e) => {
            return vec![Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("kap".into()),
                message: format!("failed to analyse code: {e}"),
                ..Default::default()
            }];
        }
    };

    // produce diagnostics for keys with more than one occurrence
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for (_, occs) in annotations.available_at.into_iter() {
        if occs.len() <= 1 {
            continue;
        }

        for e in occs {
            let start = position_from_line_col(text, e.start.0, e.start.1);
            let end = position_from_line_col(text, e.end.0, e.end.1);

            let range = Range::new(start, end);

            let message = format!("reused expression: {}", e.expr);

            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::HINT),
                source: Some("kap".into()),
                message,
                ..Default::default()
            });
        }
    }

    diagnostics
}
