use std::collections::HashSet;
use crate::lang::{Expr, Expression, Statement};

pub fn get_dependencies(expr: &Expr) -> Vec<String> {
    let mut dependencies = HashSet::new();
    collect_dependencies(&expr.expr, &mut dependencies);
    dependencies.into_iter().collect()
}

pub fn collect_dependencies(expr: &Expression, dependencies: &mut HashSet<String>) {
    match expr {
        Expression::Var(name) => {
            dependencies.insert(name.clone());
        }
        Expression::BinOp { left, right, .. } => {
            collect_dependencies(&left.expr, dependencies);
            collect_dependencies(&right.expr, dependencies);
        }
        Expression::UnOp { right, ..} => {
            collect_dependencies(&right.expr, dependencies);
        }
        Expression::If { cond, f, t} => {
            collect_dependencies(&cond.expr, dependencies);
            collect_dependencies(&f.expr, dependencies);
            collect_dependencies(&t.expr, dependencies);
        }
        Expression::While { cond, body} => {
            collect_dependencies(&cond.expr, dependencies);
            collect_dependencies(&body.expr, dependencies);
        }
        Expression::Call { args, .. } => {
            for arg in args {
                collect_dependencies(&arg.expr, dependencies);
            }
        }
        Expression::Block { statements, expr} => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration {val, ..} => {
                        collect_dependencies(&val.expr, dependencies);
                    }
                    Statement::Assignment {val, ..} => {
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