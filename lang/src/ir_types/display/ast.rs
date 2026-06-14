//! inspect the typed AST

use std::fmt::Write as _;

use crate::compiler::context::CompileCtx;
use crate::ir_types::display::INDENT;
use crate::ir_types::typed_hir::*;

impl<'tcx> TypedProgram<'tcx> {
    pub fn dump(&self, ctx: &CompileCtx<'tcx>) -> String {
        let mut out = String::new();
        for func in self.functions.values() {
            if ctx.is_core_module(ctx.file_of_module(func.src_module)) {
                continue;
            }
            out.push_str(&func.dump(ctx));
            out.push('\n');
        }

        out
    }
}

impl<'tcx> TypedFunction<'tcx> {
    pub fn dump(&self, ctx: &CompileCtx<'tcx>) -> String {
        let mut out = String::new();

        let params: Vec<String> = self
            .parameters
            .iter()
            .map(|p| {
                format!(
                    "{}: {}",
                    ctx.uniq_variable_name(&p.name),
                    ctx.display_ty(p.ty)
                )
            })
            .collect();

        let _ = writeln!(
            out,
            "fn {}({}) -> {}",
            ctx.original_fun_name(self.name),
            params.join(", "),
            ctx.display_ty(self.ret_type),
        );

        dump_expr(&mut out, &self.body, ctx, 1);

        out
    }
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str(INDENT);
    }
}

fn dump_expr<'tcx>(out: &mut String, expr: &Expr<'tcx>, ctx: &CompileCtx<'tcx>, level: usize) {
    indent(out, level);
    let _ = write!(out, "[{}] ", ctx.display_ty(expr.ty));

    match &expr.expr {
        Expression::Borrow(inner, mutable) => {
            let _ = writeln!(out, "{}", if *mutable { "borrow_mut" } else { "borrow" });
            dump_expr(out, inner, ctx, level + 1);
        }
        Expression::Deref(inner) => {
            let _ = writeln!(out, "deref");
            dump_expr(out, inner, ctx, level + 1);
        }
        Expression::If { cond, t, f } => {
            let _ = writeln!(out, "if");
            dump_expr(out, cond, ctx, level + 1);
            indent(out, level);
            let _ = writeln!(out, "then");
            dump_expr(out, t, ctx, level + 1);
            indent(out, level);
            let _ = writeln!(out, "else");
            dump_expr(out, f, ctx, level + 1);
        }
        Expression::While { cond, body } => {
            let _ = writeln!(out, "while");
            dump_expr(out, cond, ctx, level + 1);
            indent(out, level);
            let _ = writeln!(out, "do");
            dump_expr(out, body, ctx, level + 1);
        }
        Expression::BinOp { left, op, right } => {
            let _ = writeln!(out, "binop {}", op);
            dump_expr(out, left, ctx, level + 1);
            dump_expr(out, right, ctx, level + 1);
        }
        Expression::UnOp { op, right } => {
            let _ = writeln!(out, "unop {}", op);
            dump_expr(out, right, ctx, level + 1);
        }
        Expression::Call { fn_name, args } => {
            let _ = writeln!(out, "call {}", ctx.original_fun_name(*fn_name));
            for arg in args {
                dump_expr(out, arg, ctx, level + 1);
            }
        }
        Expression::IntrinsicCall { fn_name, args, .. } => {
            let _ = writeln!(out, "intrinsic {}", fn_name);
            for arg in args {
                dump_expr(out, arg, ctx, level + 1);
            }
        }
        Expression::MethodCall { method, args, .. } => {
            let _ = writeln!(out, "method {method}");
            for arg in args {
                dump_expr(out, arg, ctx, level + 1);
            }
        }
        Expression::Var(v) => {
            let _ = writeln!(out, "var {}", ctx.uniq_variable_name(v));
        }
        Expression::Int(i) => {
            let _ = writeln!(out, "int {}", i);
        }
        Expression::Bool(b) => {
            let _ = writeln!(out, "bool {}", b);
        }
        Expression::Unit => {
            let _ = writeln!(out, "unit");
        }
        Expression::Block {
            statements, expr, ..
        } => {
            let _ = writeln!(out, "block");
            for stmt in statements {
                dump_statement(out, stmt, ctx, level + 1);
            }
            if let Some(tail) = expr {
                dump_expr(out, tail, ctx, level + 1);
            }
        }
        Expression::Constructor {
            enum_ref,
            variant_idx,
            payload,
        } => {
            let _ = writeln!(out, "ctor {}", ctx.enum_display(*enum_ref, *variant_idx));
            if let Some(p) = payload {
                dump_expr(out, p, ctx, level + 1);
            }
        }
        Expression::Tuple(elems) => {
            let _ = writeln!(out, "tuple");
            for e in elems {
                dump_expr(out, e, ctx, level + 1);
            }
        }
        Expression::Match { scrutinee, arms } => {
            let _ = writeln!(out, "match");
            dump_expr(out, scrutinee, ctx, level + 1);
            for arm in arms {
                let pattern_str = dump_match_pattern(&arm.pattern, ctx);
                indent(out, level + 1);
                let _ = writeln!(out, "arm {} =>", pattern_str);
                dump_expr(out, &arm.body, ctx, level + 2);
            }
        }
    }
}

/// recursively render a `MatchPattern` as source-like syntax, e.g.
/// `Shape#Circle(r)`, `(a, b)`, `Wrap((x, y))`, `_`.
fn dump_match_pattern<'tcx>(pattern: &MatchPattern<'tcx>, ctx: &CompileCtx<'tcx>) -> String {
    match pattern {
        MatchPattern::Variant {
            enum_ref,
            variant_idx,
            payload,
            ..
        } => {
            let tag = ctx.enum_display(*enum_ref, *variant_idx);
            match payload {
                Some((_, p)) => format!("{tag}({})", dump_match_pattern(p, ctx)),
                None => tag,
            }
        }
        MatchPattern::Tuple { elems, .. } => format!(
            "({})",
            elems
                .iter()
                .map(|p| dump_match_pattern(p, ctx))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        MatchPattern::IntLit(n) => n.to_string(),
        MatchPattern::BoolLit(b) => b.to_string(),
        MatchPattern::Binding { var, .. } => ctx.uniq_variable_name(var),
        MatchPattern::Wildcard => "_".to_string(),
    }
}

fn dump_statement<'tcx>(
    out: &mut String,
    stmt: &Statement<'tcx>,
    ctx: &CompileCtx<'tcx>,
    level: usize,
) {
    match stmt {
        Statement::Declaration { name, ty, val, .. } => {
            indent(out, level);
            let _ = writeln!(out, "let {}: {} =", ctx.uniq_variable_name(name), ty);
            dump_expr(out, val, ctx, level + 1);
        }
        Statement::LetTuple { elems, val, .. } => {
            indent(out, level);
            let names: Vec<String> = elems
                .iter()
                .map(|(name, _, is_mutable, _)| {
                    let n = ctx.uniq_variable_name(name);
                    if *is_mutable { format!("mut {n}") } else { n }
                })
                .collect();
            let _ = writeln!(out, "let ({}) =", names.join(", "));
            dump_expr(out, val, ctx, level + 1);
        }
        Statement::LetPattern {
            pattern,
            val,
            else_branch,
            ..
        } => {
            indent(out, level);
            let _ = writeln!(out, "let {} =", dump_match_pattern(pattern, ctx));
            dump_expr(out, val, ctx, level + 1);
            indent(out, level);
            let _ = writeln!(out, "else");
            dump_expr(out, else_branch, ctx, level + 1);
        }
        Statement::Assignment { name, val, .. } => {
            indent(out, level);
            let _ = writeln!(out, "{} =", ctx.uniq_variable_name(name));
            dump_expr(out, val, ctx, level + 1);
        }
        Statement::DerefAssign {
            reference, value, ..
        } => {
            indent(out, level);
            let _ = writeln!(out, "* (deref-assign) =");
            dump_expr(out, reference, ctx, level + 1);
            dump_expr(out, value, ctx, level + 1);
        }
        Statement::Expr(e) => {
            dump_expr(out, e, ctx, level);
        }
    }
}
