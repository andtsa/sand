//! collect variable and function names from the AST for function call
//! resolution and variable binding checks in later passes

use crate::compiler::context::CompileCtx;
use crate::ir_types::qhir::*;
use crate::passes::type_ast::AstTypeError;

pub(super) fn collect_variables<'col, 'run>(
    ctx: &'col mut CompileCtx<'run>,
    func: &Function,
) -> Result<(), AstTypeError> {
    for param in &func.parameters {
        ctx.set_var_type(param.name, param.ty);
    }
    collect_variable_names_in_expr(ctx, &func.body)?;
    Ok(())
}

pub(super) fn collect_variable_names_in_expr<'col, 'run>(
    ctx: &'col mut CompileCtx<'run>,
    expr: &Expr,
) -> Result<(), AstTypeError> {
    match &expr.expr {
        Expression::Int(_) | Expression::Bool(_) | Expression::Unit | Expression::Var(_) => Ok(()),
        Expression::BinOp { left, right, .. } => {
            collect_variable_names_in_expr(ctx, left)?;
            collect_variable_names_in_expr(ctx, right)
        }
        Expression::UnOp { right, .. } => collect_variable_names_in_expr(ctx, right),
        Expression::If { cond, t, f } => {
            collect_variable_names_in_expr(ctx, cond)?;
            collect_variable_names_in_expr(ctx, t)?;
            collect_variable_names_in_expr(ctx, f)
        }
        Expression::While { cond, body } => {
            collect_variable_names_in_expr(ctx, cond)?;
            collect_variable_names_in_expr(ctx, body)
        }
        Expression::Call { fn_name: _, args } => {
            for arg in args {
                collect_variable_names_in_expr(ctx, arg)?;
            }
            Ok(())
        }
        Expression::IntrinsicCall { fn_name: _, args } => {
            for arg in args {
                collect_variable_names_in_expr(ctx, arg)?;
            }
            Ok(())
        }
        Expression::Block { statements, expr } => {
            for stmt in statements {
                match stmt {
                    Statement::Declaration { name, ty, val, .. } => {
                        collect_variable_names_in_expr(ctx, val)?;
                        ctx.set_var_type(*name, *ty);
                    }
                    Statement::Expr(e) => {
                        collect_variable_names_in_expr(ctx, e)?;
                    }
                    Statement::Assignment { name, val, range } => {
                        if ctx.get_var_type(name).is_some() {
                            collect_variable_names_in_expr(ctx, val)?;
                        } else {
                            return Err(AstTypeError::UnboundVariable {
                                name: ctx.uniq_variable_name(*name),
                                range: *range,
                            });
                        }
                    }
                }
            }
            if let Some(e) = expr {
                collect_variable_names_in_expr(ctx, e)?;
            }
            Ok(())
        }
    }
}
