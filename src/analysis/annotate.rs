use std::collections::HashSet;

use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::Expression;
use crate::ir_types::typed_hir::Statement;

pub fn get_dependencies(expr: &Expr) -> HashSet<UniqVar> {
    let mut dependencies = HashSet::new();
    collect_dependencies(&expr.expr, &mut dependencies);
    dependencies
}

pub fn collect_dependencies(expr: &Expression, dependencies: &mut HashSet<UniqVar>) {
    match expr {
        Expression::Var(name) => {
            dependencies.insert(*name);
        }
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
        Expression::Call { args, .. } | Expression::IntrinsicCall { args, .. } => {
            for arg in args {
                collect_dependencies(&arg.expr, dependencies);
            }
        }
        Expression::Block { statements, expr } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { val, .. } => {
                        collect_dependencies(&val.expr, dependencies);
                    }
                    Statement::Assignment { val, .. } => {
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
    }
}

pub fn get_mutations_stmt(stmt: &Statement) -> HashSet<UniqVar> {
    match stmt {
        Statement::Declaration { name, .. } => HashSet::from([*name]),
        Statement::Assignment { name, .. } => HashSet::from([*name]),
        Statement::Expr(_) => HashSet::new(),
    }
}

pub fn get_mutations_expr(expr: &Expr) -> HashSet<UniqVar> {
    let mut mutations = HashSet::new();
    collect_mutations(&expr.expr, &mut mutations);
    mutations
}

fn collect_mutations(expr: &Expression, mutations: &mut HashSet<UniqVar>) {
    match expr {
        Expression::Block { statements, expr } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { name, .. } => {
                        mutations.insert(*name);
                    }
                    Statement::Assignment { name, .. } => {
                        mutations.insert(*name);
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
        _ => {}
    }
}
