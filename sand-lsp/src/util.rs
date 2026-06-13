//! helper methods

use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::compiler::structure::ModuleRef;
use lang::compiler::structure::Pos;
use lang::compiler::structure::Range as LangRange;
use lang::ir_types::typed_hir::Expr;
use lang::ir_types::typed_hir::Expression;
use lang::ir_types::typed_hir::Statement;
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

/// Convert an LSP `Position` (0-based line, UTF-16 character offset) back to
/// a compiler `Pos` (1-based line, 1-based byte column).
pub(super) fn pos_from_lsp_position(text: &str, pos: Position) -> Pos {
    let line_str = text.lines().nth(pos.line as usize).unwrap_or("");
    // Walk chars, accumulating UTF-16 length until we hit the target offset.
    let mut utf16_count = 0u32;
    let mut col = 0usize;
    for ch in line_str.chars() {
        if utf16_count >= pos.character {
            break;
        }
        utf16_count += ch.len_utf16() as u32;
        col += 1;
    }
    Pos {
        line: pos.line as usize + 1,
        col: col + 1,
    }
}

pub(crate) fn range_contains(range: LangRange, pos: Pos) -> bool {
    let p = (pos.line, pos.col);
    p >= (range.start.line, range.start.col) && p <= (range.end.line, range.end.col)
}

pub(crate) fn find_in_expr<'a, 'tcx>(expr: &'a Expr<'tcx>, pos: Pos) -> Option<&'a Expr<'tcx>> {
    if !range_contains(expr.range, pos) {
        return None;
    }
    let child = match &expr.expr {
        Expression::BinOp { left, right, .. } => {
            find_in_expr(left, pos).or_else(|| find_in_expr(right, pos))
        }
        Expression::UnOp { right, .. } => find_in_expr(right, pos),
        Expression::Borrow(inner, _) => find_in_expr(inner, pos),
        Expression::Deref(inner) => find_in_expr(inner, pos),
        Expression::If { cond, t, f } => find_in_expr(cond, pos)
            .or_else(|| find_in_expr(t, pos))
            .or_else(|| find_in_expr(f, pos)),
        Expression::While { cond, body } => {
            find_in_expr(cond, pos).or_else(|| find_in_expr(body, pos))
        }
        Expression::Call { args, .. } | Expression::IntrinsicCall { args, .. } => {
            args.iter().find_map(|a| find_in_expr(a, pos))
        }
        Expression::Block { statements, expr } => statements
            .iter()
            .find_map(|s| find_in_stmt(s, pos))
            .or_else(|| expr.as_deref().and_then(|e| find_in_expr(e, pos))),
        Expression::Var(_) | Expression::Int(_) | Expression::Bool(_) | Expression::Unit => None,
        Expression::Constructor { payload, .. } => {
            payload.as_deref().and_then(|p| find_in_expr(p, pos))
        }
        Expression::Tuple(elems) => elems.iter().find_map(|e| find_in_expr(e, pos)),
        Expression::Match { scrutinee, arms } => find_in_expr(scrutinee, pos)
            .or_else(|| arms.iter().find_map(|arm| find_in_expr(&arm.body, pos))),
    };
    child.or(Some(expr))
}

pub(crate) fn find_in_stmt<'a, 'tcx>(
    stmt: &'a Statement<'tcx>,
    pos: Pos,
) -> Option<&'a Expr<'tcx>> {
    match stmt {
        Statement::Declaration { range, val, .. } => {
            if range_contains(*range, pos) {
                find_in_expr(val, pos).or(Some(val))
            } else {
                None
            }
        }
        Statement::Assignment { range, val, .. } => {
            if range_contains(*range, pos) {
                find_in_expr(val, pos).or(Some(val))
            } else {
                None
            }
        }
        Statement::LetTuple { range, val, .. } | Statement::LetPattern { range, val, .. } => {
            if range_contains(*range, pos) {
                find_in_expr(val, pos).or(Some(val))
            } else {
                None
            }
        }
        Statement::Expr(e) => find_in_expr(e, pos),
    }
}

pub fn url_of_module<'tcx>(
    module: ModuleRef<'tcx>,
    ctx: &CompileCtx<'tcx>,
    project: &Project,
) -> Option<Url> {
    let file_ref = ctx.file_of_module(module);
    project
        .file_contents
        .contains_key(&file_ref)
        .then(|| project.uri_of_file(file_ref))
}
