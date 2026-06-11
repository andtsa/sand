//! infer type of subexpression

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedFunction;
use crate::lang::intrinsics::INTRINSICS;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::passes::type_ast::TypeEnv;
use crate::passes::type_ast::check::check;
use crate::passes::type_ast::check::check_let_pattern;
use crate::passes::type_ast::check::type_check_match_arms;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::errors::TypeError;

pub(super) fn infer_function(
    ctx: &mut CompileCtx,
    func: &qhir::Function,
) -> Result<(FunRef, TypedFunction), TypeError> {
    let env: TypeEnv = func
        .parameters
        .iter()
        .map(|p| (p.name, (p.ty, p.is_mutable)))
        .collect();

    // use check() so that bare tags in return position are resolved against the
    // declared return type.
    let body = check(ctx, &env, &func.body, func.ret_type).map_err(|e| TypeError {
        error: e,
        module: func.src_module,
    })?;

    Ok((
        func.name,
        TypedFunction {
            name: func.name,
            range: func.range,
            parameters: func.parameters.to_vec(),
            ret_type: func.ret_type,
            body,
            src_module: func.src_module,
        },
    ))
}

pub(super) fn infer_statement(
    ctx: &mut CompileCtx,
    env: &mut TypeEnv,
    stmt: &qhir::Statement,
) -> Result<typed_hir::Statement, AstTypeError> {
    match stmt {
        qhir::Statement::Declaration {
            name,
            ty: annotation,
            is_mutable,
            val,
            range,
        } => {
            // When an annotation is present use check() so bare tags are resolved.
            let (val_expr, ty) = match annotation {
                Some(declared_ty) => {
                    let checked = check(ctx, env, val, *declared_ty)?;
                    (checked, *declared_ty)
                }
                None => {
                    let inferred = infer(ctx, env, val)?;
                    let ty = inferred.ty;
                    (inferred, ty)
                }
            };
            env.insert(*name, (ty, *is_mutable));
            Ok(typed_hir::Statement::Declaration {
                name: *name,
                range: *range,
                ty,
                val: val_expr,
            })
        }
        qhir::Statement::Assignment { name, val, range } => {
            let (var_ty, is_mutable) =
                env.get(name)
                    .copied()
                    .ok_or_else(|| AstTypeError::UnboundVariable {
                        name: ctx.uniq_variable_name(name),
                        range: *range,
                    })?;
            if !is_mutable {
                return Err(AstTypeError::ImmutableAssignment {
                    name: ctx.uniq_variable_name(name),
                    range: *range,
                });
            }
            // Use check() so that bare tags are resolved against the variable's
            // known type (e.g. `result = #gt` when result: #gt | #lt | #eq).
            let val_expr = check(ctx, env, val, var_ty)?;
            Ok(typed_hir::Statement::Assignment {
                name: *name,
                range: *range,
                val: val_expr,
            })
        }
        qhir::Statement::LetTuple {
            elems,
            ty: annotation,
            val,
            range,
        } => {
            // Infer or check the RHS; it must be a tuple type of matching arity.
            let val_expr = match annotation {
                Some(declared_ty) => check(ctx, env, val, *declared_ty)?,
                None => infer(ctx, env, val)?,
            };
            let elem_tys = match ctx.ty_kind(val_expr.ty) {
                TyKind::Tuple(tys) => tys.clone(),
                _ => {
                    return Err(AstTypeError::PatternTypeMismatch {
                        message: format!(
                            "let-tuple pattern used against non-tuple type {}",
                            ctx.display_ty(val_expr.ty)
                        ),
                        range: *range,
                    });
                }
            };
            if elem_tys.len() != elems.len() {
                return Err(AstTypeError::PatternArityMismatch {
                    expected: elem_tys.len(),
                    found: elems.len(),
                    range: *range,
                });
            }
            // Register each element in the environment and collect typed elems.
            let typed_elems: Vec<(
                crate::compiler::structure::UniqVar,
                Ty,
                bool,
                crate::compiler::structure::Range,
            )> = elems
                .iter()
                .zip(elem_tys.iter())
                .map(|((name, is_mutable, elem_range), &ty)| {
                    env.insert(*name, (ty, *is_mutable));
                    (*name, ty, *is_mutable, *elem_range)
                })
                .collect();
            Ok(typed_hir::Statement::LetTuple {
                elems: typed_elems,
                range: *range,
                val: val_expr,
            })
        }

        qhir::Statement::LetPattern {
            pattern,
            ty: annotation,
            val,
            else_branch,
            range,
        } => {
            // 1. Type-check (or infer) the main value expression.
            let val_expr = match annotation {
                Some(declared_ty) => check(ctx, env, val, *declared_ty)?,
                None => infer(ctx, env, val)?,
            };
            let scrutinee_ty = val_expr.ty;

            // 2. Resolve the pattern against scrutinee_ty; collect bindings.
            let (typed_pattern, bindings) = check_let_pattern(ctx, pattern, scrutinee_ty, *range)?;

            // 3. Extract the expected variant_idx from the typed pattern for the else
            //    check.
            let expected_variant_idx = match &typed_pattern {
                typed_hir::MatchPattern::Variant { variant_idx, .. } => *variant_idx,
                _ => unreachable!("check_let_pattern always returns a Variant pattern"),
            };

            // 4. Type-check the else branch against the same type as `val`.
            let else_expr = check(ctx, env, else_branch, scrutinee_ty)?;

            // 5. Verify the else expression is a constructor of the same variant so that
            //    destructuring the fallback is always guaranteed to succeed.
            match &else_expr.expr {
                typed_hir::Expression::Constructor {
                    variant_idx: else_vi,
                    ..
                } if *else_vi == expected_variant_idx => {}
                _ => {
                    return Err(AstTypeError::LetPatternElseNotIrrefutable { range: *range });
                }
            }

            // 6. Register all bindings in the environment.
            for (var, ty, _range) in &bindings {
                env.insert(*var, (*ty, false)); // all let-pattern bindings are immutable
            }

            Ok(typed_hir::Statement::LetPattern {
                pattern: typed_pattern,
                val: val_expr,
                else_branch: else_expr,
                range: *range,
            })
        }

        qhir::Statement::Expr(e) => {
            let e_expr = infer(ctx, env, e)?;
            Ok(typed_hir::Statement::Expr(e_expr))
        }
    }
}

pub(super) fn infer(
    ctx: &mut CompileCtx,
    env: &TypeEnv,
    expr: &qhir::Expr,
) -> Result<typed_hir::Expr, AstTypeError> {
    match &expr.expr {
        qhir::Expression::Int(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Int(*x),
            range: expr.range,
            ty: Ty::INT,
        }),
        qhir::Expression::Bool(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Bool(*x),
            range: expr.range,
            ty: Ty::BOOL,
        }),
        qhir::Expression::Unit => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Unit,
            range: expr.range,
            ty: Ty::UNIT,
        }),
        qhir::Expression::Var(x) => {
            let (ty, _) = env
                .get(x)
                .copied()
                .ok_or_else(|| AstTypeError::UnboundVariable {
                    name: ctx.uniq_variable_name(x),
                    range: expr.range,
                })?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Var(*x),
                range: expr.range,
                ty,
            })
        }
        qhir::Expression::Constructor {
            enum_ref,
            variant_idx,
            payload,
        } => {
            let declared_payload = ctx.get_enum(*enum_ref).variants[*variant_idx].payload;
            let typed_payload = match (declared_payload, payload) {
                (None, None) => None,
                (Some(declared_ty), Some(p)) => Some(Box::new(check(ctx, env, p, declared_ty)?)),
                (None, Some(p)) => {
                    return Err(AstTypeError::ConstructorPayloadMismatch {
                        enum_name: ctx.get_enum(*enum_ref).name.clone(),
                        variant: ctx.get_enum(*enum_ref).variants[*variant_idx].name.clone(),
                        expected_payload: false,
                        range: p.range,
                    });
                }
                (Some(_), None) => {
                    return Err(AstTypeError::ConstructorPayloadMismatch {
                        enum_name: ctx.get_enum(*enum_ref).name.clone(),
                        variant: ctx.get_enum(*enum_ref).variants[*variant_idx].name.clone(),
                        expected_payload: true,
                        range: expr.range,
                    });
                }
            };
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Constructor {
                    enum_ref: *enum_ref,
                    variant_idx: *variant_idx,
                    payload: typed_payload,
                },
                range: expr.range,
                ty: ctx.enum_ty(*enum_ref),
            })
        }

        qhir::Expression::Tag { variant, .. } => Err(AstTypeError::TagWithoutContext {
            variant: variant.clone(),
            range: expr.range,
        }),

        qhir::Expression::Tuple(elems) => {
            let typed_elems = elems
                .iter()
                .map(|e| infer(ctx, env, e))
                .collect::<Result<Vec<_>, _>>()?;
            let elem_tys: Vec<Ty> = typed_elems.iter().map(|e| e.ty).collect();
            let ty = ctx.intern_tuple(elem_tys);
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Tuple(typed_elems),
                range: expr.range,
                ty,
            })
        }

        qhir::Expression::BinOp { left, op, right } => {
            // When one side is a bare Tag, infer the other side first, then
            // use check() to resolve the tag against the inferred type.
            let (left_expr, right_expr) = match (&left.expr, &right.expr) {
                (_, qhir::Expression::Tag { .. }) => {
                    let l = infer(ctx, env, left)?;
                    let r = check(ctx, env, right, l.ty)?;
                    (l, r)
                }
                (qhir::Expression::Tag { .. }, _) => {
                    let r = infer(ctx, env, right)?;
                    let l = check(ctx, env, left, r.ty)?;
                    (l, r)
                }
                _ => (infer(ctx, env, left)?, infer(ctx, env, right)?),
            };

            let ty = op
                .accepts_types(left_expr.ty, right_expr.ty)
                .map_err(|expected_ty| AstTypeError::TypeError {
                    message: format!(
                        "operator '{:?}' does not accept types {} and {}",
                        op, left_expr.ty, right_expr.ty
                    ),
                    expected: expected_ty,
                    found: left_expr.ty,
                    range: expr.range,
                })?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::BinOp {
                    left: Box::new(left_expr),
                    op: *op,
                    right: Box::new(right_expr),
                },
                range: expr.range,
                ty,
            })
        }
        qhir::Expression::UnOp { op, right } => {
            let right_expr = infer(ctx, env, right)?;

            let ty =
                op.accepts_type(right_expr.ty)
                    .map_err(|expected_ty| AstTypeError::TypeError {
                        message: format!(
                            "operator '{:?}' does not accept type {}",
                            op, right_expr.ty
                        ),
                        expected: expected_ty,
                        found: right_expr.ty,
                        range: expr.range,
                    })?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::UnOp {
                    op: *op,
                    right: Box::new(right_expr),
                },
                range: expr.range,
                ty,
            })
        }
        qhir::Expression::If { cond, t, f } => {
            let cond_expr = infer(ctx, env, cond)?;
            if cond_expr.ty != Ty::BOOL {
                return Err(AstTypeError::TypeError {
                    message: format!("condition of 'if' must be Bool, found {}", cond_expr.ty),
                    expected: Ty::BOOL,
                    found: cond_expr.ty,
                    range: cond.range,
                });
            }

            let t_expr = infer(ctx, env, t)?;

            let (f_expr, ty) = match f {
                Some(f) => {
                    let f_expr = infer(ctx, env, f)?;
                    if t_expr.ty != f_expr.ty {
                        return Err(AstTypeError::TypeError {
                            message: format!(
                                "branches of 'if' expression must have the same type, found {} and {}",
                                t_expr.ty, f_expr.ty
                            ),
                            expected: t_expr.ty,
                            found: f_expr.ty,
                            range: expr.range,
                        });
                    }
                    let ty = t_expr.ty;
                    (f_expr, ty)
                }
                None => {
                    if t_expr.ty != Ty::UNIT {
                        return Err(AstTypeError::TypeError {
                            message: format!(
                                "'if' without 'else' must have type Unit, but then-branch has type {}",
                                t_expr.ty
                            ),
                            expected: Ty::UNIT,
                            found: t_expr.ty,
                            range: t.range,
                        });
                    }
                    let f_expr = typed_hir::Expr {
                        expr: typed_hir::Expression::Unit,
                        range: expr.range,
                        ty: Ty::UNIT,
                    };
                    (f_expr, Ty::UNIT)
                }
            };

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::If {
                    cond: Box::new(cond_expr),
                    t: Box::new(t_expr),
                    f: Box::new(f_expr),
                },
                range: expr.range,
                ty,
            })
        }
        qhir::Expression::While { cond, body } => {
            let cond_expr = infer(ctx, env, cond)?;
            if cond_expr.ty != Ty::BOOL {
                return Err(AstTypeError::TypeError {
                    message: format!("condition of 'while' must be Bool, found {}", cond_expr.ty),
                    expected: Ty::BOOL,
                    found: cond_expr.ty,
                    range: cond.range,
                });
            }

            let body_expr = infer(ctx, env, body)?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::While {
                    cond: Box::new(cond_expr),
                    body: Box::new(body_expr),
                },
                range: expr.range,
                ty: Ty::UNIT,
            })
        }
        qhir::Expression::Call { fn_name, args } => {
            let fun_sig = ctx.fun_sig(fn_name);
            let expected_tys: Vec<Ty> = fun_sig.args.iter().map(|p| p.1).collect();

            if args.len() != expected_tys.len() {
                // Arity mismatch: infer args just for the error message.
                let arg_exprs = args
                    .iter()
                    .map(|arg| infer(ctx, env, arg))
                    .collect::<Result<Vec<_>, _>>()?;
                let arg_tys: Vec<Ty> = arg_exprs.iter().map(|e| e.ty).collect();
                return Err(AstTypeError::FunctionCallTypeError {
                    message: format!(
                        "function '{}' expects {} arguments but found {}",
                        ctx.original_fun_name(*fn_name),
                        expected_tys.len(),
                        arg_tys.len()
                    ),
                    expected: expected_tys,
                    found: arg_tys,
                    range: expr.range,
                });
            }

            // check() each argument against the declared parameter type so that bare
            // tags in argument position can be resolved from the expected type context.
            let arg_exprs = args
                .iter()
                .zip(&expected_tys)
                .map(|(arg, &expected_ty)| check(ctx, env, arg, expected_ty))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Call {
                    fn_name: *fn_name,
                    args: arg_exprs,
                },
                range: expr.range,
                ty: fun_sig.ret_ty,
            })
        }
        qhir::Expression::IntrinsicCall { fn_name, args } => {
            let (_, fn_sig) = &INTRINSICS[fn_name];
            let expected_tys = fn_sig.args.clone();

            if args.len() != expected_tys.len() {
                // Arity mismatch: infer args just for the error message.
                let arg_exprs = args
                    .iter()
                    .map(|arg| infer(ctx, env, arg))
                    .collect::<Result<Vec<_>, _>>()?;
                let arg_tys: Vec<Ty> = arg_exprs.iter().map(|e| e.ty).collect();
                return Err(AstTypeError::FunctionCallTypeError {
                    message: format!(
                        "intrinsic '{}' expects {} arguments but found {}",
                        fn_name,
                        expected_tys.len(),
                        arg_tys.len()
                    ),
                    expected: expected_tys,
                    found: arg_tys,
                    range: expr.range,
                });
            }

            // check() each argument against the declared parameter type.
            let arg_exprs = args
                .iter()
                .zip(&expected_tys)
                .map(|(arg, &expected_ty)| check(ctx, env, arg, expected_ty))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::IntrinsicCall {
                    fn_name: *fn_name,
                    args: arg_exprs,
                },
                range: expr.range,
                ty: fn_sig.ret_ty,
            })
        }
        qhir::Expression::Block {
            statements,
            expr: ret_expr,
        } => {
            let (typed_statements, final_env) = statements.iter().try_fold(
                (Vec::with_capacity(statements.len()), env.clone()),
                |(mut stmts, mut env), stmt| {
                    stmts.push(infer_statement(ctx, &mut env, stmt)?);
                    Ok((stmts, env))
                },
            )?;

            let (typed_expr, ret_ty) = if let Some(e) = ret_expr {
                let t_expr = infer(ctx, &final_env, e)?;
                let ret_ty = t_expr.ty;
                (Some(Box::new(t_expr)), ret_ty)
            } else {
                (None, Ty::UNIT)
            };

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Block {
                    statements: typed_statements,
                    expr: typed_expr,
                },
                range: expr.range,
                ty: ret_ty,
            })
        }
        qhir::Expression::Match { scrutinee, arms } => {
            let scrut_expr = infer(ctx, env, scrutinee)?;
            let typed_arms =
                type_check_match_arms(ctx, env, arms, scrut_expr.ty, None, expr.range)?;
            let result_ty = typed_arms.first().map(|a| a.body.ty).unwrap_or(Ty::UNIT);
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Match {
                    scrutinee: Box::new(scrut_expr),
                    arms: typed_arms,
                },
                ty: result_ty,
                range: expr.range,
            })
        }
    }
}
