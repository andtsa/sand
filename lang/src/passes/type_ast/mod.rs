//! take a parsed and uniquified AST,
//! annotate expressions with their types,
//! check them for correctness,
//! and output a TypedProgram AST

mod errors;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::UniqVar;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedFunction;
use crate::lang::intrinsics::INTRINSICS;
use crate::lang::types::Ty;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::errors::TypeError;

type TypeEnv = Map<UniqVar, (Ty, bool)>; // (type, is_mutable)

impl typed_hir::TypedProgram {
    pub fn from_ast_program(ctx: &CompileCtx, ast: qhir::Program) -> Result<Self, TypeError> {
        let fn_list = ast
            .functions
            .values()
            .map(|f| infer_function(ctx, f))
            .collect::<Result<Vec<(FunRef, TypedFunction)>, _>>()?;

        let functions = fn_list.into_iter().collect::<Map<_, _>>();

        Ok(typed_hir::TypedProgram { functions })
    }
}

fn infer_function(
    ctx: &CompileCtx,
    func: &qhir::Function,
) -> Result<(FunRef, TypedFunction), TypeError> {
    let env: TypeEnv = func
        .parameters
        .iter()
        .map(|p| (p.name, (p.ty, p.is_mutable)))
        .collect();

    // Use check() so that bare tags in return position are resolved against the
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

/// Bidirectional type checking: verify that `expr` has type `expected`,
/// propagating the expected type into sub-expressions where useful (primarily
/// bare `#Tag` expressions and the trailing expression of if-else / blocks).
fn check(
    ctx: &CompileCtx,
    env: &TypeEnv,
    expr: &qhir::Expr,
    expected: Ty,
) -> Result<typed_hir::Expr, AstTypeError> {
    match &expr.expr {
        // Resolve a bare #Tag against the expected enum type.
        qhir::Expression::Tag { variant } => {
            let er = match expected {
                Ty::Enum(er) => er,
                _ => {
                    return Err(AstTypeError::TagInNonEnumContext {
                        variant: variant.clone(),
                        range: expr.range,
                    });
                }
            };
            let variant_idx =
                ctx.lookup_variant(er, variant)
                    .ok_or_else(|| AstTypeError::UnknownTagVariant {
                        variant: variant.clone(),
                        enum_name: ctx.get_enum(er).name.clone(),
                        range: expr.range,
                    })?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Constructor {
                    enum_ref: er,
                    variant_idx,
                },
                ty: expected,
                range: expr.range,
            })
        }

        // Propagate check mode into both branches of an if-else.
        qhir::Expression::If {
            cond,
            t,
            f: Some(f),
        } => {
            let cond_expr = infer(ctx, env, cond)?;
            if cond_expr.ty != Ty::Bool {
                return Err(AstTypeError::TypeError {
                    message: format!("condition of 'if' must be Bool, found {:?}", cond_expr.ty),
                    expected: Ty::Bool,
                    found: cond_expr.ty,
                    range: cond.range,
                });
            }
            let t_expr = check(ctx, env, t, expected)?;
            let f_expr = check(ctx, env, f, expected)?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::If {
                    cond: Box::new(cond_expr),
                    t: Box::new(t_expr),
                    f: Box::new(f_expr),
                },
                ty: expected,
                range: expr.range,
            })
        }

        // Propagate check mode into the trailing expression of a block.
        qhir::Expression::Block {
            statements,
            expr: Some(ret),
        } => {
            let (typed_statements, final_env) = statements.iter().try_fold(
                (Vec::with_capacity(statements.len()), env.clone()),
                |(mut stmts, mut env), stmt| {
                    stmts.push(infer_statement(ctx, &mut env, stmt)?);
                    Ok((stmts, env))
                },
            )?;
            let typed_ret = check(ctx, &final_env, ret, expected)?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Block {
                    statements: typed_statements,
                    expr: Some(Box::new(typed_ret)),
                },
                ty: expected,
                range: expr.range,
            })
        }

        // Everything else: infer, then verify type matches expected.
        _ => {
            let e = infer(ctx, env, expr)?;
            if e.ty.type_neq(&expected) {
                return Err(AstTypeError::TypeError {
                    message: format!("expected type {:?} but found {:?}", expected, e.ty),
                    expected,
                    found: e.ty,
                    range: expr.range,
                });
            }
            Ok(e)
        }
    }
}

fn infer(
    ctx: &CompileCtx,
    env: &TypeEnv,
    expr: &qhir::Expr,
) -> Result<typed_hir::Expr, AstTypeError> {
    match &expr.expr {
        qhir::Expression::Int(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Int(*x),
            range: expr.range,
            ty: Ty::Int,
        }),
        qhir::Expression::Bool(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Bool(*x),
            range: expr.range,
            ty: Ty::Bool,
        }),
        qhir::Expression::Unit => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Unit,
            range: expr.range,
            ty: Ty::Unit,
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
        } => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Constructor {
                enum_ref: *enum_ref,
                variant_idx: *variant_idx,
            },
            range: expr.range,
            ty: Ty::Enum(*enum_ref),
        }),

        qhir::Expression::Tag { variant } => Err(AstTypeError::TagWithoutContext {
            variant: variant.clone(),
            range: expr.range,
        }),

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
                        "operator '{:?}' does not accept types {:?} and {:?}",
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
                            "operator '{:?}' does not accept type {:?}",
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
            if cond_expr.ty != Ty::Bool {
                return Err(AstTypeError::TypeError {
                    message: format!("condition of 'if' must be Bool, found {:?}", cond_expr.ty),
                    expected: Ty::Bool,
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
                                "branches of 'if' expression must have the same type, found {:?} and {:?}",
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
                    if t_expr.ty != Ty::Unit {
                        return Err(AstTypeError::TypeError {
                            message: format!(
                                "'if' without 'else' must have type Unit, but then-branch has type {:?}",
                                t_expr.ty
                            ),
                            expected: Ty::Unit,
                            found: t_expr.ty,
                            range: t.range,
                        });
                    }
                    let f_expr = typed_hir::Expr {
                        expr: typed_hir::Expression::Unit,
                        range: expr.range,
                        ty: Ty::Unit,
                    };
                    (f_expr, Ty::Unit)
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
            if cond_expr.ty != Ty::Bool {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "condition of 'while' must be Bool, found {:?}",
                        cond_expr.ty
                    ),
                    expected: Ty::Bool,
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
                ty: Ty::Unit,
            })
        }
        qhir::Expression::Call { fn_name, args } => {
            let fun_sig = ctx.fun_sig(fn_name);

            let arg_exprs = args
                .iter()
                .map(|arg| infer(ctx, env, arg))
                .collect::<Result<Vec<_>, _>>()?;

            let arg_tys: Vec<Ty> = arg_exprs.iter().map(|e| e.ty).collect();
            let expected_tys: Vec<Ty> = fun_sig.args.iter().map(|p| p.1).collect();

            if arg_tys.len() != expected_tys.len() {
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

            let _arg_typecheck: Option<()> = arg_tys
                .iter()
                .zip(&expected_tys)
                .enumerate()
                .find(|(_, (found, expected))| found.type_neq(expected))
                .map(|(i, (found, expected))| {
                    Err(AstTypeError::FunctionCallTypeError {
                        message: format!(
                            "argument {} of function '{}' expects type {:?} but found {:?}",
                            i + 1,
                            ctx.original_fun_name(*fn_name),
                            expected,
                            found
                        ),
                        expected: vec![*expected],
                        found: vec![*found],
                        range: args[i].range,
                    })
                })
                .transpose()?;

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

            let arg_exprs = args
                .iter()
                .map(|arg| infer(ctx, env, arg))
                .collect::<Result<Vec<_>, _>>()?;

            let arg_tys: Vec<Ty> = arg_exprs.iter().map(|e| e.ty).collect();

            if arg_tys.len() != fn_sig.args.len() {
                return Err(AstTypeError::FunctionCallTypeError {
                    message: format!(
                        "intrinsic '{}' expects {} arguments but found {}",
                        fn_name,
                        fn_sig.args.len(),
                        arg_tys.len()
                    ),
                    expected: fn_sig.args.to_vec(),
                    found: arg_tys,
                    range: expr.range,
                });
            }

            let _arg_typecheck: Option<()> = arg_tys
                .iter()
                .zip(&fn_sig.args)
                .enumerate()
                .find(|(_, (found, expected))| found.type_neq(expected))
                .map(|(i, (found, expected))| {
                    Err(AstTypeError::FunctionCallTypeError {
                        message: format!(
                            "argument {} of intrinsic '{}' expects type {:?} but found {:?}",
                            i + 1,
                            fn_name,
                            expected,
                            found
                        ),
                        expected: vec![*expected],
                        found: vec![*found],
                        range: args[i].range,
                    })
                })
                .transpose()?;

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
                (None, Ty::Unit)
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
    }
}

fn infer_statement(
    ctx: &CompileCtx,
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
            let val_expr = infer(ctx, env, val)?;
            if val_expr.ty.type_neq(&var_ty) {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "variable '{}' has type {:?} but assigned value has type {:?}",
                        ctx.uniq_variable_name(name),
                        var_ty,
                        val_expr.ty
                    ),
                    expected: var_ty,
                    found: val_expr.ty,
                    range: *range,
                });
            }
            Ok(typed_hir::Statement::Assignment {
                name: *name,
                range: *range,
                val: val_expr,
            })
        }
        qhir::Statement::Expr(e) => {
            let e_expr = infer(ctx, env, e)?;
            Ok(typed_hir::Statement::Expr(e_expr))
        }
    }
}
