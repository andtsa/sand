//! helper methods

use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::compiler::structure::ModuleRef;
use lang::compiler::structure::Pos;
use lang::compiler::structure::Range as LangRange;
use tower_lsp::lsp_types::*;

pub(super) fn lsp_position_from_pest(text: &str, pos: Pos) -> Position {
    // pest reports 1-based line/col; convert to 0-based
    let line_idx = pos.line.saturating_sub(1);
    let col_idx = pos.col.saturating_sub(1);

    // get the text of the line (lines() drops the newline)
    let line_str = text.lines().nth(line_idx).unwrap_or("");

    // take `col_idx` rust chars, then count UTF-16 code units (LSP uses UTF-16)
    let prefix: String = line_str.chars().take(col_idx).collect();
    let utf16_col = prefix.encode_utf16().count();

    Position::new(line_idx as u32, utf16_col as u32)
}

pub(super) fn lsp_positions_from_range(text: &str, range: LangRange) -> (Position, Position) {
    let start = lsp_position_from_pest(text, range.start);
    let end = lsp_position_from_pest(text, range.end);
    (start, end)
}

pub(super) fn lsp_range_from_pest(text: &str, range: LangRange) -> Range {
    let (start, end) = lsp_positions_from_range(text, range);
    Range::new(start, end)
}

pub fn url_of_module(module: ModuleRef, ctx: &CompileCtx, project: &Project) -> Option<Url> {
    let file_ref = ctx.file_of_module(module);
    project
        .file_contents
        .contains_key(&file_ref)
        .then(|| project.uri_of_file(file_ref))
}
