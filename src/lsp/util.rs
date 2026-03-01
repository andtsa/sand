//! helper methods

use pest::error::LineColLocation;
use tower_lsp::lsp_types::*;

use crate::passes::parse::Rule;

pub(super) fn position_from_line_col(text: &str, line: usize, col: usize) -> Position {
    // pest reports 1-based line/col; convert to 0-based
    let line_idx = line.saturating_sub(1);
    let col_idx = col.saturating_sub(1);

    // get the text of the line (lines() drops the newline)
    let line_str = text.lines().nth(line_idx).unwrap_or("");

    // take `col_idx` rust chars, then count UTF-16 code units (LSP uses UTF-16)
    let prefix: String = line_str.chars().take(col_idx).collect();
    let utf16_col = prefix.encode_utf16().count();

    Position::new(line_idx as u32, utf16_col as u32)
}

pub(super) fn parse_error_to_diagnostic(text: &str, err: pest::error::Error<Rule>) -> Diagnostic {
    let (start, end) = match err.line_col {
        LineColLocation::Pos((l, c)) => {
            let p = position_from_line_col(text, l, c);
            (p, p)
        }
        LineColLocation::Span((sl, sc), (el, ec)) => {
            let start = position_from_line_col(text, sl, sc);
            let end = position_from_line_col(text, el, ec);
            (start, end)
        }
    };

    Diagnostic {
        range: Range::new(start, end),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("sand".into()),
        message: err.variant.message().into(),
        ..Default::default()
    }
}
