//! explicate control of our functional language to construct the MIR from
//! an AST

pub mod context;

use crate::compiler::context::CompileCtx;
use crate::ir_types::mir::*;
use crate::ir_types::typed_hir as th;
use crate::passes::explicate_control::context::FnCx;

impl<'tcx> MirProgram<'tcx> {
    pub fn from_typed_program(prog: &th::TypedProgram<'tcx>, ctx: &CompileCtx<'tcx>) -> Self {
        let functions = prog
            .functions
            .iter()
            .map(|(name, func)| (*name, lower_function(func, ctx)))
            .collect();

        Self { functions }
    }
}

fn lower_function<'tcx>(
    func: &th::TypedFunction<'tcx>,
    ctx: &CompileCtx<'tcx>,
) -> MirFunction<'tcx> {
    let mut cx = FnCx::new(func.name, func.range, func.ret_type, ctx.types);

    let params = func
        .parameters
        .iter()
        .map(|p| {
            let local = cx.get_or_create_local(p.name, p.ty, p.range);
            MirParam {
                local,
                name: p.name,
                ty: p.ty,
                range: p.range,
            }
        })
        .collect::<Vec<_>>();

    collect_locals(&mut cx, &func.body);

    let mut entry = cx.lower_tail(&func.body);

    cx.blocks.reverse();
    // fix up all BlockId references since indices just changed
    let n = cx.blocks.len();
    for block in &mut cx.blocks {
        block.id = BlockId(n - 1 - block.id.0);
        fix_terminator_ids(&mut block.terminator, n);
    }
    entry.0 = n - 1 - entry.0;

    MirFunction {
        name: func.name,
        range: func.range,
        params,
        ret_type: func.ret_type,
        locals: cx.locals,
        blocks: cx.blocks,
        entry,
    }
}

fn fix_terminator_ids(term: &mut Terminator, n: usize) {
    match term {
        Terminator::Branch {
            cond: _,
            then_bb,
            else_bb,
        } => {
            then_bb.0 = n - 1 - then_bb.0;
            else_bb.0 = n - 1 - else_bb.0;
        }
        Terminator::Goto { target } => {
            target.0 = n - 1 - target.0;
        }
        _ => {}
    }
}

fn collect_locals<'tcx>(cx: &mut FnCx<'tcx>, expr: &th::Expr<'tcx>) {
    match &expr.expr {
        th::Expression::Block { statements, expr } => {
            for stmt in statements {
                match stmt {
                    th::Statement::Declaration {
                        name,
                        ty,
                        range,
                        val,
                    } => {
                        cx.get_or_create_local(*name, *ty, *range);
                        collect_locals(cx, val);
                    }
                    th::Statement::Assignment { val, .. } => collect_locals(cx, val),
                    th::Statement::LetTuple { elems, val, .. } => {
                        for (name, ty, _, range) in elems {
                            cx.get_or_create_local(*name, *ty, *range);
                        }
                        collect_locals(cx, val);
                    }
                    th::Statement::LetPattern {
                        pattern,
                        val,
                        else_branch,
                        ..
                    } => {
                        // Register locals for all bindings introduced by the pattern.
                        declare_pattern_locals(cx, pattern);
                        collect_locals(cx, val);
                        collect_locals(cx, else_branch);
                    }
                    th::Statement::Expr(e) => collect_locals(cx, e),
                }
            }
            if let Some(e) = expr {
                collect_locals(cx, e);
            }
        }
        th::Expression::If { cond, t, f } => {
            collect_locals(cx, cond);
            collect_locals(cx, t);
            collect_locals(cx, f);
        }
        th::Expression::While { cond, body } => {
            collect_locals(cx, cond);
            collect_locals(cx, body);
        }
        th::Expression::BinOp { left, right, .. } => {
            collect_locals(cx, left);
            collect_locals(cx, right);
        }
        th::Expression::UnOp { right, .. } => collect_locals(cx, right),
        th::Expression::Borrow(inner, _) => collect_locals(cx, inner),
        th::Expression::Deref(inner) => collect_locals(cx, inner),
        th::Expression::Call { args, .. } | th::Expression::IntrinsicCall { args, .. } => {
            for a in args {
                collect_locals(cx, a);
            }
        }
        th::Expression::Var(_)
        | th::Expression::Int(_)
        | th::Expression::Bool(_)
        | th::Expression::Unit => {}
        th::Expression::Constructor { payload, .. } => {
            if let Some(p) = payload {
                collect_locals(cx, p);
            }
        }
        th::Expression::Tuple(elems) => {
            for e in elems {
                collect_locals(cx, e);
            }
        }
        th::Expression::Match { scrutinee, arms } => {
            collect_locals(cx, scrutinee);
            for arm in arms {
                collect_locals(cx, &arm.body);
            }
        }
    }
}

/// Recursively declare MIR locals for every variable bound in a `LetPattern`.
fn declare_pattern_locals<'tcx>(cx: &mut FnCx<'tcx>, pattern: &th::MatchPattern<'tcx>) {
    match pattern {
        th::MatchPattern::Binding {
            var,
            ty,
            range: brange,
        } => {
            cx.get_or_create_local(*var, *ty, *brange);
        }
        th::MatchPattern::Tuple { elems, .. } => {
            for e in elems {
                declare_pattern_locals(cx, e);
            }
        }
        th::MatchPattern::Variant { payload, .. } => {
            if let Some((_, sub)) = payload {
                declare_pattern_locals(cx, sub);
            }
        }
        th::MatchPattern::Wildcard | th::MatchPattern::IntLit(_) | th::MatchPattern::BoolLit(_) => {
        }
    }
}
