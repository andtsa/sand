//! LSP hover action implementation

use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::compiler::structure::FileRef;
use lang::compiler::structure::Pos;
use lang::compiler::structure::Range as LangRange;
use lang::ir_types::typed_hir::Expr;
use lang::ir_types::typed_hir::Expression;
use lang::ir_types::typed_hir::Statement;
use lang::ir_types::typed_hir::TypedProgram;
use lang::lang::intrinsics::INTRINSICS;
use lang::lang::types::Ty;
use tower_lsp::lsp_types::Hover;
use tower_lsp::lsp_types::HoverContents;
use tower_lsp::lsp_types::MarkupContent;
use tower_lsp::lsp_types::MarkupKind;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::Url;

use crate::util::pos_from_lsp_position;

pub fn hover_at_position(
    lsp_pos: Position,
    uri: &Url,
    ctx: &CompileCtx,
    ast: &TypedProgram,
    project: &Project,
) -> Option<Hover> {
    let file_ref: FileRef = project.is_tracked(uri)?;
    let text = project.text_for_file(file_ref)?;
    let pos = pos_from_lsp_position(text, lsp_pos);

    for fun in ast.functions.values() {
        if ctx.file_of_module(fun.src_module) != file_ref {
            continue;
        }
        // Check parameters first (more specific than the body range)
        for param in &fun.parameters {
            if range_contains(param.range, pos) {
                let name = ctx.uniq_variable_name(&param.name);
                return Some(make_hover(format!(
                    "**{}: {}**\nParameter",
                    name,
                    fmt_ty(param.ty)
                )));
            }
        }
        if let Some(expr) = find_in_expr(&fun.body, pos) {
            return Some(format_hover(expr, ctx));
        }
    }
    None
}

fn range_contains(range: LangRange, pos: Pos) -> bool {
    let p = (pos.line, pos.col);
    p >= (range.start.line, range.start.col) && p <= (range.end.line, range.end.col)
}

fn find_in_expr<'a>(expr: &'a Expr, pos: Pos) -> Option<&'a Expr> {
    if !range_contains(expr.range, pos) {
        return None;
    }
    let child = match &expr.expr {
        Expression::BinOp { left, right, .. } => {
            find_in_expr(left, pos).or_else(|| find_in_expr(right, pos))
        }
        Expression::UnOp { right, .. } => find_in_expr(right, pos),
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
    };
    child.or(Some(expr))
}

fn find_in_stmt<'a>(stmt: &'a Statement, pos: Pos) -> Option<&'a Expr> {
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
        Statement::Expr(e) => find_in_expr(e, pos),
    }
}

fn format_hover(expr: &Expr, ctx: &CompileCtx) -> Hover {
    let content = match &expr.expr {
        Expression::Var(uv) => {
            let name = ctx.uniq_variable_name(uv);
            let decl = ctx.uniq_var_declaration(uv);
            format!(
                "**{}: {}**\nDeclared at line {}, col {}",
                name,
                fmt_ty(expr.ty),
                decl.start.line,
                decl.start.col
            )
        }
        Expression::Call { fn_name, .. } => {
            let name = ctx.original_fun_name(*fn_name);
            let sig = ctx.fun_sig(fn_name);
            let args = fmt_sig_args(&sig.args, ctx);
            let orig = ctx.original_fun(fn_name);
            let module = ctx.module_info(&orig.module);
            format!(
                "**{}({}) → {}**\nDefined in module `{}`",
                name,
                args,
                fmt_ty(sig.ret_ty),
                module.name
            )
        }
        Expression::IntrinsicCall { fn_name, .. } => {
            if let Some((_, sig)) = INTRINSICS.get(fn_name) {
                let args = sig
                    .args
                    .iter()
                    .map(|t| fmt_ty(*t).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "**{}({}) → {}**\nBuilt-in intrinsic",
                    fn_name,
                    args,
                    fmt_ty(sig.ret_ty)
                )
            } else {
                format!("**intrinsic {}**", fn_name)
            }
        }
        _ => format!(": {}", fmt_ty(expr.ty)),
    };
    make_hover(content)
}

fn fmt_sig_args(args: &[(lang::compiler::structure::UniqVar, Ty)], ctx: &CompileCtx) -> String {
    args.iter()
        .map(|(uv, ty)| format!("{}: {}", ctx.uniq_variable_name(uv), fmt_ty(*ty)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_ty(ty: Ty) -> &'static str {
    match ty {
        Ty::Int => "Int",
        Ty::Bool => "Bool",
        Ty::Unit => "Unit",
        Ty::Top => "Top",
        Ty::Bottom => "Bottom",
    }
}

fn make_hover(value: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    }
}
