//! check that the types of a TypedProgram AST actually make sense

use crate::ir_types::typed_hir::*;
use crate::lang::intrinsics::INTRINSICS;
use crate::lang::structure::Range;
use crate::lang::types::Ty;
use crate::passes::type_ast::AstTypeError;

fn expect_type(
    found: Ty,
    expected: Ty,
    message: impl FnOnce() -> String,
    range: Range,
) -> Result<(), AstTypeError> {
    if found.type_neq(&expected) {
        Err(AstTypeError::TypeError {
            message: message(),
            expected,
            found,
            range,
        })
    } else {
        Ok(())
    }
}

fn expect_same_type(
    left: Ty,
    right: Ty,
    message: impl FnOnce() -> String,
    range: Range,
) -> Result<Ty, AstTypeError> {
    if left.type_neq(&right) {
        Err(AstTypeError::TypeError {
            message: message(),
            expected: left,
            found: right,
            range,
        })
    } else {
        Ok(left)
    }
}

fn check_call_args(
    fn_name: String,
    args: &[Expr],
    expected: &[Ty],
    ret_ty: Ty,
    prog: &TypedProgram,
    range: Range,
) -> Result<Ty, AstTypeError> {
    let arg_tys = args
        .iter()
        .map(|arg| check_expr(arg, prog))
        .collect::<Result<Vec<_>, _>>()?;

    if arg_tys.len() != expected.len() {
        return Err(AstTypeError::FunctionCallTypeError {
            message: format!(
                "Function '{}' expects {} arguments but found {}",
                fn_name,
                expected.len(),
                arg_tys.len()
            ),
            expected: expected.to_vec(),
            found: arg_tys,
            range,
        });
    }

    for (i, (found, expected_ty)) in arg_tys.iter().zip(expected).enumerate() {
        if found.type_neq(expected_ty) {
            return Err(AstTypeError::FunctionCallTypeError {
                message: format!(
                    "Argument {} of function '{}' expects type {:?} but found {:?}",
                    i + 1,
                    fn_name,
                    expected_ty,
                    found
                ),
                expected: vec![*expected_ty],
                found: vec![*found],
                range: args[i].range,
            });
        }
    }

    Ok(ret_ty)
}

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
    if body_ty.type_neq(&func.ret_type) {
        return Err(AstTypeError::TypeError {
            message: format!(
                "Function '{}' has return type {:?} but body has type {:?}",
                func.name, func.ret_type, body_ty
            ),
            expected: func.ret_type,
            found: body_ty,
            range: func.range,
        });
    }

    Ok(())
}

pub(super) fn check_expr(expr: &Expr, prog: &TypedProgram) -> Result<Ty, AstTypeError> {
    match &expr.expr {
        Expression::BinOp { left, op, right } => {
            let left_ty = check_expr(left, prog)?;
            let right_ty = check_expr(right, prog)?;

            op.accepts_types(left_ty, right_ty)
                .map_err(|expected_ty| AstTypeError::TypeError {
                    message: format!(
                        "Operator '{:?}' does not accept types {:?} and {:?}",
                        op, left_ty, right_ty
                    ),
                    expected: expected_ty,
                    found: left_ty,
                    range: expr.range,
                })
        }
        Expression::UnOp { op, right } => {
            let right_ty = check_expr(right, prog)?;

            if let Err(expected_ty) = op.accepts_type(right_ty) {
                return Err(AstTypeError::TypeError {
                    message: format!("Operator '{:?}' does not accept type {:?}", op, right_ty),
                    expected: expected_ty,
                    found: right_ty,
                    range: expr.range,
                });
            }

            Ok(right_ty)
        }
        Expression::If { cond, t, f } => {
            let cond_ty = check_expr(cond, prog)?;
            expect_type(
                cond_ty,
                Ty::Bool,
                || format!("Condition of 'if' must be Bool, found {:?}", cond_ty),
                cond.range,
            )?;

            let t_ty = check_expr(t, prog)?;
            let f_ty = check_expr(f, prog)?;
            expect_same_type(
                t_ty,
                f_ty,
                || {
                    format!(
                        "Branches of 'if' must have same type, found {:?} and {:?}",
                        t_ty, f_ty
                    )
                },
                t.range,
            )
        }
        Expression::While { cond, body } => {
            let cond_ty = check_expr(cond, prog)?;
            if cond_ty != Ty::Bool {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "Condition {cond:?} of 'while' expression must be of type Bool, found {:?}",
                        cond_ty
                    ),
                    expected: Ty::Bool,
                    found: cond_ty,
                    range: cond.range,
                });
            }
            check_expr(body, prog)
        }
        Expression::Call { fn_name, args } => {
            let func =
                prog.functions
                    .get(fn_name)
                    .ok_or_else(|| AstTypeError::UndefinedFunction {
                        name: fn_name.to_string(),
                        range: expr.range,
                    })?;

            let expected: Vec<Ty> = func.parameters.iter().map(|p| p.ty).collect();
            check_call_args(
                fn_name.to_string(),
                args,
                &expected,
                func.ret_type,
                prog,
                expr.range,
            )
        }

        Expression::IntrinsicCall { fn_name, args } => {
            let (_fn_ref, fn_sig) = &INTRINSICS[fn_name];
            let expected: Vec<Ty> = fn_sig.args.iter().map(|(_, ty)| *ty).collect();
            check_call_args(
                fn_name.to_string(),
                args,
                &expected,
                fn_sig.ret_ty,
                prog,
                expr.range,
            )
        }
        Expression::Block { statements, expr } => {
            let mut block_scope = prog.clone();
            for stmt in statements {
                match stmt {
                    Statement::Declaration {
                        name,
                        ty,
                        val,
                        range,
                    } => {
                        let val_ty = check_expr(val, &block_scope)?;
                        if val_ty.type_neq(ty) {
                            return Err(AstTypeError::TypeError {
                                message: format!(
                                    "Declared variable '{}' has type {:?} but initializer has type {:?}",
                                    name, ty, val_ty
                                ),
                                expected: *ty,
                                found: val_ty,
                                range: *range,
                            });
                        }
                        block_scope.avail_vars.insert(name.clone(), *ty);
                    }
                    Statement::Assignment { name, val, range } => {
                        let var_ty = block_scope.avail_vars.get(name).ok_or_else(|| {
                            AstTypeError::UnboundVariable {
                                name: name.to_string(),
                                range: *range,
                            }
                        })?;
                        let val_ty = check_expr(val, &block_scope)?;
                        if val_ty.type_neq(var_ty) {
                            return Err(AstTypeError::TypeError {
                                message: format!(
                                    "Variable '{}' has type {:?} but assigned value has type {:?}",
                                    name, var_ty, val_ty
                                ),
                                expected: *var_ty,
                                found: val_ty,
                                range: *range,
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
        Expression::RVar(name) => match prog.avail_vars.get(name) {
            Some(ty) => Ok(*ty),
            None => Err(AstTypeError::UnboundVariable {
                name: name.to_string(),
                range: expr.range,
            }),
        },
    }
}
