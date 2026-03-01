//! collect variable and function names from the AST for function call
//! resolution and variable binding checks in later passes

use std::collections::BTreeMap;

use crate::ir_types::ast::*;
use crate::ir_types::typed_ast::FnName;
use crate::ir_types::typed_ast::VarName;
use crate::ir_types::types::Ty;
use crate::passes::type_ast::AstTypeError;

pub(super) type VarMap = BTreeMap<VarName, Ty>;
pub(super) type FnSig = (Vec<(VarName, Ty)>, Ty);
pub(super) type FnMap = BTreeMap<FnName, FnSig>;

pub(super) fn collect_variables(func: &Function, fn_args: &FnMap) -> Result<VarMap, AstTypeError> {
    let mut vars = BTreeMap::new();
    for param in &func.parameters {
        vars.insert(param.name.clone(), param.ty);
    }
    vars = collect_variable_names_in_expr(&func.body, vars, fn_args)?;
    Ok(vars)
}

pub(super) fn collect_variable_names_in_expr(
    expr: &Expr,
    vars: VarMap,
    fn_args: &FnMap,
) -> Result<VarMap, AstTypeError> {
    match &expr.expr {
        Expression::Int(_) | Expression::Bool(_) | Expression::Unit => Ok(vars),
        Expression::Var(x) => {
            if !vars.contains_key(x) {
                return Err(AstTypeError::UnboundVariable {
                    name: x.clone(),
                    start: expr.start,
                    end: expr.end,
                });
            }
            Ok(vars)
        }
        Expression::BinOp { left, right, .. } => {
            let left = collect_variable_names_in_expr(left, vars.clone(), fn_args)?;
            let right = collect_variable_names_in_expr(right, vars, fn_args)?;
            Ok(left.into_iter().chain(right).collect())
        }
        Expression::UnOp { right, .. } => collect_variable_names_in_expr(right, vars, fn_args),
        Expression::If { cond, t, f } => {
            let cond_vars = collect_variable_names_in_expr(cond, vars.clone(), fn_args)?;
            let t_vars = collect_variable_names_in_expr(t, vars.clone(), fn_args)?;
            let f_vars = collect_variable_names_in_expr(f, vars.clone(), fn_args)?;
            Ok(vars
                .into_iter()
                .chain(
                    cond_vars
                        .into_iter()
                        .chain(t_vars.into_iter().chain(f_vars)),
                )
                .collect())
        }
        Expression::While { cond, body } => {
            let cond_vars = collect_variable_names_in_expr(cond, vars.clone(), fn_args)?;
            let body_vars = collect_variable_names_in_expr(body, vars, fn_args)?;
            Ok(cond_vars.into_iter().chain(body_vars).collect())
        }
        Expression::Call { fn_name, args } => {
            // functions only get their arguments in the variable scope
            let params = fn_args
                .get(fn_name)
                .ok_or_else(|| AstTypeError::UndefinedFunction {
                    name: fn_name.clone(),
                    start: expr.start,
                    end: expr.end,
                })?;
            let mut new_vars = vars.clone();
            for (param_name, param_ty) in &params.0 {
                new_vars.insert(param_name.clone(), *param_ty);
            }
            new_vars = args
                .iter()
                .map(|arg| collect_variable_names_in_expr(arg, new_vars.clone(), fn_args))
                .collect::<Result<Vec<VarMap>, AstTypeError>>()?
                .into_iter()
                .flatten()
                .collect();
            Ok(new_vars)
        }
        Expression::Block { statements, expr } => {
            let mut new_vars = vars.clone();
            for stmt in statements {
                match stmt {
                    Statement::Declaration { name, ty, .. } => {
                        new_vars.insert(name.clone(), *ty);
                    }
                    Statement::Expr(e) => {
                        new_vars = collect_variable_names_in_expr(e, new_vars, fn_args)?;
                    }
                    Statement::Assignment {
                        name,
                        val,
                        name_start,
                        name_end,
                    } => {
                        if vars.get(name).is_some() {
                            new_vars = collect_variable_names_in_expr(val, vars.clone(), fn_args)?;
                        } else {
                            return Err(AstTypeError::UnboundVariable {
                                name: name.clone(),
                                start: *name_start,
                                end: *name_end,
                            });
                        }
                    }
                }
            }
            if let Some(e) = expr {
                new_vars = collect_variable_names_in_expr(e, vars, fn_args)?;
            }
            Ok(new_vars)
        }
    }
}
