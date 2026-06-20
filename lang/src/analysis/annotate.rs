use std::collections::HashSet;

use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::Expression;
use crate::ir_types::typed_hir::MatchPattern;
use crate::ir_types::typed_hir::Statement;
use crate::lang::types::Ty;

/// Visit every variable bound by `pattern`, calling `f` with the bound variable
/// and its type. The shared traversal behind the per-pass binding handlers
/// (collecting into a set here, declaring into an ownership env, ..) which only
/// differ in what they do at each `Binding` leaf.
pub fn for_each_binding<'tcx>(
    pattern: &MatchPattern<'tcx>,
    f: &mut impl FnMut(UniqVar<'tcx>, Ty<'tcx>),
) {
    match pattern {
        MatchPattern::Binding { var, ty, .. } => f(*var, *ty),
        MatchPattern::Tuple { elems, .. } => {
            for e in elems {
                for_each_binding(e, f);
            }
        }
        MatchPattern::Variant { payload, .. } => {
            if let Some((_, sub)) = payload {
                for_each_binding(sub, f);
            }
        }
        MatchPattern::Wildcard | MatchPattern::IntLit(_) | MatchPattern::BoolLit(_) => {}
    }
}

/// Collect all variable names bound by a `LetPattern`'s match pattern.
pub fn collect_let_pattern_bindings<'tcx>(pattern: &MatchPattern<'tcx>) -> HashSet<UniqVar<'tcx>> {
    let mut set = HashSet::new();
    for_each_binding(pattern, &mut |var, _| {
        set.insert(var);
    });
    set
}

pub fn get_dependencies<'tcx>(expr: &Expr<'tcx>) -> HashSet<UniqVar<'tcx>> {
    let mut dependencies = HashSet::new();
    collect_dependencies(&expr.expr, &mut dependencies);
    dependencies
}

pub fn collect_dependencies<'tcx>(
    expr: &Expression<'tcx>,
    dependencies: &mut HashSet<UniqVar<'tcx>>,
) {
    match expr {
        Expression::Var(name) => {
            dependencies.insert(*name);
        }
        Expression::Borrow(inner, _) => collect_dependencies(&inner.expr, dependencies),
        Expression::Deref(inner) => collect_dependencies(&inner.expr, dependencies),
        Expression::BinOp { left, right, .. } => {
            collect_dependencies(&left.expr, dependencies);
            collect_dependencies(&right.expr, dependencies);
        }
        Expression::UnOp { right, .. } => {
            collect_dependencies(&right.expr, dependencies);
        }
        Expression::If { cond, f, t } => {
            collect_dependencies(&cond.expr, dependencies);
            collect_dependencies(&f.expr, dependencies);
            collect_dependencies(&t.expr, dependencies);
        }
        Expression::While { cond, body } => {
            collect_dependencies(&cond.expr, dependencies);
            collect_dependencies(&body.expr, dependencies);
        }
        Expression::Call { args, .. }
        | Expression::IntrinsicCall { args, .. }
        | Expression::MethodCall { args, .. } => {
            for arg in args {
                collect_dependencies(&arg.expr, dependencies);
            }
        }
        Expression::Block {
            statements, expr, ..
        } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { val, .. } => {
                        collect_dependencies(&val.expr, dependencies);
                    }
                    Statement::Assignment { val, .. } => {
                        collect_dependencies(&val.expr, dependencies);
                    }
                    Statement::DerefAssign {
                        reference, value, ..
                    } => {
                        collect_dependencies(&reference.expr, dependencies);
                        collect_dependencies(&value.expr, dependencies);
                    }
                    Statement::LetTuple { val, .. } | Statement::LetPattern { val, .. } => {
                        collect_dependencies(&val.expr, dependencies);
                    }
                    Statement::Expr(e) => {
                        collect_dependencies(&e.expr, dependencies);
                    }
                }
            }
            if let Some(e) = expr {
                collect_dependencies(&e.expr, dependencies);
            }
        }
        Expression::Int(_) | Expression::Bool(_) | Expression::Unit => {}
        Expression::Constructor { payload, .. } => {
            if let Some(p) = payload {
                collect_dependencies(&p.expr, dependencies);
            }
        }
        Expression::Tuple(elems) => {
            for e in elems {
                collect_dependencies(&e.expr, dependencies);
            }
        }
        Expression::Lambda { body, .. } => collect_dependencies(&body.expr, dependencies),
        Expression::Apply { func, arg } => {
            collect_dependencies(&func.expr, dependencies);
            collect_dependencies(&arg.expr, dependencies);
        }
        Expression::Closure { captures, .. } => {
            for (c, _) in captures {
                dependencies.insert(*c);
            }
        }
        Expression::Match { scrutinee, arms } => {
            collect_dependencies(&scrutinee.expr, dependencies);
            for arm in arms {
                collect_dependencies(&arm.body.expr, dependencies);
            }
        }
    }
}

pub fn get_mutations_stmt<'tcx>(stmt: &Statement<'tcx>) -> HashSet<UniqVar<'tcx>> {
    match stmt {
        Statement::Declaration { name, .. } => HashSet::from([*name]),
        Statement::Assignment { name, .. } => HashSet::from([*name]),
        // write-through mutates through a reference, not a named local.
        Statement::DerefAssign { .. } => HashSet::new(),
        Statement::LetTuple { elems, .. } => elems.iter().map(|(n, ..)| *n).collect(),
        Statement::LetPattern { pattern, .. } => collect_let_pattern_bindings(pattern),
        Statement::Expr(_) => HashSet::new(),
    }
}

pub fn get_mutations_expr<'tcx>(expr: &Expr<'tcx>) -> HashSet<UniqVar<'tcx>> {
    let mut mutations = HashSet::new();
    collect_mutations(&expr.expr, &mut mutations);
    mutations
}

fn collect_mutations<'tcx>(expr: &Expression<'tcx>, mutations: &mut HashSet<UniqVar<'tcx>>) {
    match expr {
        Expression::Block {
            statements, expr, ..
        } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { name, .. } => {
                        mutations.insert(*name);
                    }
                    Statement::Assignment { name, .. } => {
                        mutations.insert(*name);
                    }
                    Statement::DerefAssign { .. } => {}
                    Statement::LetTuple { elems, .. } => {
                        for (name, ..) in elems {
                            mutations.insert(*name);
                        }
                    }
                    Statement::LetPattern { pattern, .. } => {
                        mutations.extend(collect_let_pattern_bindings(pattern));
                    }
                    Statement::Expr(e) => {
                        collect_mutations(&e.expr, mutations);
                    }
                }
            }
            if let Some(e) = expr {
                collect_dependencies(&e.expr, mutations);
            }
        }
        Expression::If { t, f, .. } => {
            collect_mutations(&t.expr, mutations);
            collect_mutations(&f.expr, mutations);
        }
        Expression::While { body, .. } => {
            collect_mutations(&body.expr, mutations);
        }
        Expression::Match { arms, .. } => {
            for arm in arms {
                collect_mutations(&arm.body.expr, mutations);
            }
        }
        _ => {}
    }
}
