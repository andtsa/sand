//! LSP hover action implementation

use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::compiler::structure::FileRef;
use lang::ir_types::typed_hir::Expr;
use lang::ir_types::typed_hir::Expression;
use lang::ir_types::typed_hir::TypedFunction;
use lang::ir_types::typed_hir::TypedProgram;
use lang::lang::intrinsics::INTRINSICS;
use lang::lang::types::Ty;
use tower_lsp::lsp_types::Hover;
use tower_lsp::lsp_types::HoverContents;
use tower_lsp::lsp_types::MarkupContent;
use tower_lsp::lsp_types::MarkupKind;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::Url;

use crate::util::find_in_expr;
use crate::util::pos_from_lsp_position;
use crate::util::range_contains;

pub fn hover_at_position<'tcx>(
    lsp_pos: Position,
    uri: &Url,
    ctx: &CompileCtx<'tcx>,
    ast: &TypedProgram<'tcx>,
    project: &Project,
) -> Option<Hover> {
    let file_ref: FileRef = project.is_tracked(uri)?;
    let text = project.text_for_file(file_ref)?;
    let pos = pos_from_lsp_position(text, lsp_pos);

    for fun in ast.functions.values() {
        if ctx.file_of_module(fun.src_module) != file_ref {
            continue;
        }
        // if cursor is on the function name itself,
        // show signature, and run if main
        if range_contains(fun.range, pos) {
            return Some(format_function_hover(fun, ctx, ast));
        }
        // cursor on a parameter
        for param in &fun.parameters {
            if range_contains(param.range, pos) {
                let name = ctx.uniq_variable_name(&param.name);
                return Some(make_hover(format!(
                    "**{}: {}**\nParameter",
                    name,
                    fmt_ty(ctx, param.ty)
                )));
            }
        }
        if let Some(expr) = find_in_expr(&fun.body, pos) {
            return Some(format_hover(expr, ctx));
        }
    }
    None
}

fn format_function_hover<'tcx>(
    fun: &TypedFunction<'tcx>,
    ctx: &CompileCtx<'tcx>,
    ast: &TypedProgram<'tcx>,
) -> Hover {
    let name = ctx.original_fun_name(fun.name);
    let sig = ctx.fun_sig(&fun.name);
    let args = fmt_sig_args(&sig.args, ctx);
    let sig_line = format!("**{}({}) -> {}**", name, args, fmt_ty(ctx, sig.ret_ty));

    if ctx.is_main(fun.name) {
        let mut output_buf: Vec<u8> = Vec::new();
        let run_result = ast.interpret_with_output(ctx, &mut output_buf);
        let printed = String::from_utf8_lossy(&output_buf);

        let content = match run_result {
            Ok(val) => {
                let mut s = sig_line;
                if !printed.is_empty() {
                    s.push_str("\n\n## Output:\n```\n");
                    s.push_str(printed.trim_end());
                    s.push_str("\n```");
                }
                s.push_str(&format!("\n\n## Returned:\n`{}`", fmt_expr_val(&val, ctx)));
                s
            }
            Err(e) => format!("{sig_line}\n\n⚠ Runtime error: {e}"),
        };
        make_hover(content)
    } else {
        let module = ctx.module_info(&ctx.original_fun(&fun.name).module);
        make_hover(format!("{sig_line}\nDefined in module `{}`", module.name))
    }
}

fn fmt_expr_val<'tcx>(val: &Expression<'tcx>, ctx: &CompileCtx<'tcx>) -> String {
    match val {
        Expression::Int(n) => n.to_string(),
        Expression::Bool(b) => b.to_string(),
        Expression::Unit => "()".to_string(),
        Expression::Constructor {
            enum_ref,
            variant_idx,
            payload,
        } => {
            let tag = ctx.enum_display(*enum_ref, *variant_idx);
            match payload {
                Some(p) => format!("{tag}({})", fmt_expr_val(&p.expr, ctx)),
                None => tag,
            }
        }
        Expression::Tuple(elems) => format!(
            "({})",
            elems
                .iter()
                .map(|e| fmt_expr_val(&e.expr, ctx))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => "<value>".to_string(),
    }
}

fn format_hover<'tcx>(expr: &Expr<'tcx>, ctx: &CompileCtx<'tcx>) -> Hover {
    let content = match &expr.expr {
        Expression::Var(uv) => {
            let name = ctx.uniq_variable_name(uv);
            let decl = ctx.uniq_var_declaration(uv);
            format!(
                "**{}: {}**\nDeclared at line {}, col {}",
                name,
                fmt_ty(ctx, expr.ty),
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
                fmt_ty(ctx, sig.ret_ty),
                module.name
            )
        }
        Expression::IntrinsicCall { fn_name, .. } => {
            if let Some((_, sig)) = INTRINSICS.get(fn_name) {
                let (resolved_args, resolved_ret) = sig.resolve(&ctx.types);
                let args = resolved_args
                    .iter()
                    .map(|&t| fmt_ty(ctx, t).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "**{}({}) → {}**\nBuilt-in intrinsic",
                    fn_name,
                    args,
                    fmt_ty(ctx, resolved_ret)
                )
            } else {
                format!("**intrinsic {}**", fn_name)
            }
        }
        _ => format!(": {}", fmt_ty(ctx, expr.ty)),
    };
    make_hover(content)
}

fn fmt_sig_args<'tcx>(
    args: &[(lang::compiler::structure::UniqVar<'tcx>, Ty<'tcx>)],
    ctx: &CompileCtx<'tcx>,
) -> String {
    args.iter()
        .map(|(uv, ty)| format!("{}: {}", ctx.uniq_variable_name(uv), fmt_ty(ctx, *ty)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_ty<'tcx>(ctx: &CompileCtx<'tcx>, ty: Ty<'tcx>) -> String {
    ctx.display_ty(ty).to_string()
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
