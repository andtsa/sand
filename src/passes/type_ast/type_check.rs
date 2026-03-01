//! check that the types of a TypedProgram AST actually make sense

use crate::ir_types::typed_ast::*;
use crate::ir_types::types::Ty;
use crate::passes::type_ast::AstTypeError;

pub(super) fn check_program(prog: &TypedProgram) -> Result<(), AstTypeError> {
    for func in prog.functions.values() {
        check_function(func, prog)?;
    }
    Ok(())
}

pub(super) fn check_function(
    func: &TypedFunction,
    prog: &TypedProgram,
) -> Result<(), AstTypeError> {
    // check that the function's return type matches the type of its body expression
    let body_ty = check_expr(&func.body, prog)?;
    if body_ty != func.ret_type {
        return Err(AstTypeError::TypeError {
            message: format!(
                "Function '{}' has return type {:?} but body has type {:?}",
                func.name, func.ret_type, body_ty
            ),
            expected: func.ret_type.clone(),
            found: body_ty,
            start: func.name_start,
            end: func.name_end,
        });
    }

    Ok(())
}

pub(super) fn check_expr(expr: &Expr, prog: &TypedProgram) -> Result<Ty, AstTypeError> {
    match &expr.expr {
        Expression::BinOp { left, op, right } => {
            let left_ty = check_expr(left, prog)?;
            let right_ty = check_expr(right, prog)?;

            if let Err(expected_ty) = op.accepts_types(left_ty, right_ty) {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "Operator '{:?}' does not accept types {:?} and {:?}",
                        op, left_ty, right_ty
                    ),
                    expected: expected_ty,
                    found: left_ty,
                    start: expr.start,
                    end: expr.end,
                });
            }

            Ok(left_ty)
        }
        Expression::UnOp { op, right } => {
            let right_ty = check_expr(right, prog)?;

            if let Err(expected_ty) = op.accepts_type(right_ty) {
                return Err(AstTypeError::TypeError {
                    message: format!("Operator '{:?}' does not accept type {:?}", op, right_ty),
                    expected: expected_ty,
                    found: right_ty,
                    start: expr.start,
                    end: expr.end,
                });
            }

            Ok(right_ty)
        }
        Expression::If { cond, t, f } => {
            let cond_ty = check_expr(cond, prog)?;
            if cond_ty != Ty::Bool {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "Condition of 'if' expression must be of type Bool, found {:?}",
                        cond_ty
                    ),
                    expected: Ty::Bool,
                    found: cond_ty,
                    start: cond.start,
                    end: cond.end,
                });
            }

            let t_ty = check_expr(t, prog)?;
            let f_ty = check_expr(f, prog)?;
            if t_ty != f_ty {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "Branches of 'if' expression must have the same type, found {:?} and {:?}",
                        t_ty, f_ty
                    ),
                    expected: t_ty,
                    found: f_ty,
                    start: t.start,
                    end: f.end,
                });
            }
            Ok(t_ty)
        }
        Expression::While { cond, body } => {
            let cond_ty = check_expr(cond, prog)?;
            if cond_ty != Ty::Bool {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "Condition of 'while' expression must be of type Bool, found {:?}",
                        cond_ty
                    ),
                    expected: Ty::Bool,
                    found: cond_ty,
                    start: cond.start,
                    end: cond.end,
                });
            }
            check_expr(body, prog)
        }
        Expression::Call { fn_name, args } => {
            let func =
                prog.functions
                    .get(fn_name)
                    .ok_or_else(|| AstTypeError::UndefinedFunction {
                        name: fn_name.clone(),
                        start: expr.start,
                        end: expr.end,
                    })?;

            let arg_tys: Vec<Ty> = args
                .iter()
                .map(|arg| check_expr(arg, prog))
                .collect::<Result<Vec<_>, _>>()?;

            if args.len() != func.parameters.len() {
                return Err(AstTypeError::FunctionCallTypeError {
                    message: format!(
                        "Function '{}' expects {} arguments but found {}",
                        fn_name,
                        func.parameters.len(),
                        args.len()
                    ),
                    expected: func.parameters.iter().map(|p| p.ty.clone()).collect(),
                    found: arg_tys,
                    start: expr.start,
                    end: expr.end,
                });
            }

            for (i, (arg_ty, param)) in arg_tys.iter().zip(&func.parameters).enumerate() {
                if arg_ty != &param.ty {
                    return Err(AstTypeError::FunctionCallTypeError {
                        message: format!(
                            "Argument {} of function '{}' expects type {:?} but found {:?}",
                            i + 1,
                            fn_name,
                            param.ty,
                            arg_ty
                        ),
                        expected: vec![param.ty.clone()],
                        found: vec![arg_ty.clone()],
                        start: args[i].start,
                        end: args[i].end,
                    });
                }
            }

            Ok(func.ret_type.clone())
        }
        Expression::Block { statements, expr } => {
            let mut block_scope = prog.clone();
            for stmt in statements {
                match stmt {
                    Statement::Declaration {
                        name,
                        ty,
                        val,
                        name_start,
                        name_end,
                    } => {
                        let val_ty = check_expr(val, &block_scope)?;
                        if val_ty != *ty {
                            return Err(AstTypeError::TypeError {
                                message: format!(
                                    "Declared variable '{}' has type {:?} but initializer has type {:?}",
                                    name, ty, val_ty
                                ),
                                expected: ty.clone(),
                                found: val_ty,
                                start: *name_start,
                                end: *name_end,
                            });
                        }
                        block_scope.avail_vars.insert(name.clone(), ty.clone());
                    }
                    Statement::Assignment {
                        name,
                        val,
                        name_start,
                        name_end,
                    } => {
                        let var_ty = block_scope.avail_vars.get(name).ok_or_else(|| {
                            AstTypeError::UnboundVariable {
                                name: name.clone(),
                                start: *name_start,
                                end: *name_end,
                            }
                        })?;
                        let val_ty = check_expr(val, &block_scope)?;
                        if val_ty != *var_ty {
                            return Err(AstTypeError::TypeError {
                                message: format!(
                                    "Variable '{}' has type {:?} but assigned value has type {:?}",
                                    name, var_ty, val_ty
                                ),
                                expected: var_ty.clone(),
                                found: val_ty,
                                start: *name_start,
                                end: *name_end,
                            });
                        }
                    }
                    Statement::Expr(e) => {
                        check_expr(e, &block_scope)?;
                    }
                }
            }
            if let Some(e) = expr {
                check_expr(e, &block_scope)
            } else {
                Ok(Ty::Unit)
            }
        }
        Expression::Int(_) => Ok(Ty::Int),
        Expression::Bool(_) => Ok(Ty::Bool),
        Expression::Unit => Ok(Ty::Unit),
        Expression::Var(name) => match prog.avail_vars.get(name) {
            Some(ty) => Ok(*ty),
            None => Err(AstTypeError::UnboundVariable {
                name: name.clone(),
                start: expr.start,
                end: expr.end,
            }),
        },
    }
}
