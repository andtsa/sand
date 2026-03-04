//! collect variable and function names from the AST for function call
//! resolution and variable binding checks in later passes

use std::collections::BTreeMap;

use crate::ir_types::hhir::*;
use crate::lang::structure::FnName;
use crate::lang::structure::FnSig;
use crate::lang::structure::Map;
use crate::lang::structure::VarName;
use crate::lang::types::Ty;
use crate::passes::type_ast::AstTypeError;
use crate::passes::type_ast::check_intrinsic::is_intrinsic_call;

pub(super) type VarMap = Map<String, (VarName, Ty)>;
pub(super) type FnMap = Map<String, (FnName, FnSig)>;

pub(super) fn collect_variables(func: &Function, fn_args: &FnMap) -> Result<VarMap, AstTypeError> {
    let mut vars = BTreeMap::new();
    for param in &func.parameters {
        vars.insert(param.name.clone(), (VarName::from(param), param.ty));
    }
    vars = collect_variable_names_in_expr(&func.body, vars, fn_args)?;
    Ok(vars)
}

#[track_caller]
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
                    range: expr.range,
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
            if !is_intrinsic_call(fn_name) {
                let _params =
                    fn_args
                        .get(fn_name)
                        .ok_or_else(|| AstTypeError::UndefinedFunction {
                            name: fn_name.clone(),
                            range: expr.range,
                        })?;
            }
            let mut acc = vars;
            for arg in args {
                acc = collect_variable_names_in_expr(arg, acc, fn_args)?;
            }
            Ok(acc)
        }
        Expression::Block { statements, expr } => {
            let mut new_vars = vars.clone();
            for stmt in statements {
                match stmt {
                    d @ Statement::Declaration { name, ty, val, .. } => {
                        new_vars = collect_variable_names_in_expr(val, new_vars, fn_args)?;
                        new_vars.insert(name.clone(), (VarName::try_from(d).unwrap(), *ty));
                    }
                    Statement::Expr(e) => {
                        new_vars = collect_variable_names_in_expr(e, new_vars, fn_args)?;
                    }
                    Statement::Assignment { name, val, range } => {
                        if new_vars.contains_key(name) {
                            new_vars = collect_variable_names_in_expr(val, new_vars, fn_args)?;
                        } else {
                            return Err(AstTypeError::UnboundVariable {
                                name: name.clone(),
                                range: *range,
                            });
                        }
                    }
                }
            }
            if let Some(e) = expr {
                new_vars = collect_variable_names_in_expr(e, new_vars, fn_args)?;
            }
            Ok(new_vars)
        }
    }
}
