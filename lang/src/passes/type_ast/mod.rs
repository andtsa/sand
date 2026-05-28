//! take a parsed and uniquified AST,
//! annotate expressions with their types,
//! check them for correctness,
//! and output a TypedProgram AST

mod errors;
mod type_check;
mod var_types;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedFunction;
use crate::lang::intrinsics::INTRINSICS;
use crate::lang::types::Ty;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::errors::TypeError;

impl typed_hir::TypedProgram {
    pub fn from_ast_program<'tyc, 'run>(
        ctx: &'tyc mut CompileCtx<'run>,
        ast: qhir::Program,
    ) -> Result<Self, TypeError> {
        let fn_list = ast
            .functions
            .values()
            .map(|f| annotate_function(ctx, f))
            .collect::<Result<Vec<(FunRef, TypedFunction)>, _>>()?;

        let functions = fn_list.into_iter().collect::<Map<_, _>>();

        let prog = typed_hir::TypedProgram { functions };

        type_check::check_program(ctx, &prog)?;

        Ok(prog)
    }
}

pub fn annotate_function<'tyc, 'run>(
    ctx: &'tyc mut CompileCtx<'run>,
    func: &qhir::Function,
) -> Result<(FunRef, TypedFunction), TypeError> {
    var_types::collect_variables(ctx, func).map_err(|e| TypeError {
        error: e,
        module: func.src_module,
    })?;

    let body = annotate_expression(ctx, &func.body).map_err(|e| TypeError {
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

fn annotate_expression<'tyc, 'run>(
    ctx: &'tyc mut CompileCtx<'run>,
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
        qhir::Expression::Var(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Var(*x),
            range: expr.range,
            ty: ctx.get_var_type(x).expect("untyped variable"),
        }),
        qhir::Expression::BinOp { left, op, right } => {
            let left_expr = annotate_expression(ctx, left)?;
            let right_expr = annotate_expression(ctx, right)?;

            let expected_ty =
                op.accepts_types(left_expr.ty, right_expr.ty)
                    .map_err(|expected_ty| AstTypeError::TypeError {
                        message: format!(
                            "Operator '{:?}' does not accept types {:?} and {:?}",
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
                ty: expected_ty,
            })
        }
        qhir::Expression::UnOp { op, right } => {
            let right_expr = annotate_expression(ctx, right)?;

            let expected_ty =
                op.accepts_type(right_expr.ty)
                    .map_err(|expected_ty| AstTypeError::TypeError {
                        message: format!(
                            "Operator '{:?}' does not accept type {:?}",
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
                ty: expected_ty,
            })
        }
        qhir::Expression::If { cond, t, f } => {
            let cond_expr = annotate_expression(ctx, cond)?;
            let t_expr = annotate_expression(ctx, t)?;
            let f_expr = annotate_expression(ctx, f)?;

            let expected_ty = if t_expr.ty != f_expr.ty {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "Branches of 'if' expression must have the same type, found {:?} and {:?}",
                        t_expr.ty, f_expr.ty
                    ),
                    expected: t_expr.ty,
                    found: f_expr.ty,
                    range: expr.range,
                });
            } else {
                t_expr.ty
            };

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::If {
                    cond: Box::new(cond_expr),
                    t: Box::new(t_expr),
                    f: Box::new(f_expr),
                },
                range: expr.range,
                ty: expected_ty,
            })
        }
        qhir::Expression::While { cond, body } => {
            let cond_expr = annotate_expression(ctx, cond)?;
            let body_expr = annotate_expression(ctx, body)?;
            let ret_ty = body_expr.ty;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::While {
                    cond: Box::new(cond_expr),
                    body: Box::new(body_expr),
                },
                range: expr.range,
                ty: ret_ty,
            })
        }
        qhir::Expression::Call { fn_name, args } => {
            let arg_exprs_and_tys = args
                .iter()
                .map(|arg| annotate_expression(ctx, arg))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Call {
                    fn_name: *fn_name,
                    args: arg_exprs_and_tys,
                },
                range: expr.range,
                ty: ctx.get_fun_sig(fn_name).expect("untyped function").ret_ty,
            })
        }
        qhir::Expression::IntrinsicCall { fn_name, args } => {
            let arg_exprs_and_tys = args
                .iter()
                .map(|arg| annotate_expression(ctx, arg))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::IntrinsicCall {
                    fn_name: *fn_name,
                    args: arg_exprs_and_tys,
                },
                range: expr.range,
                ty: INTRINSICS[fn_name].1.ret_ty,
            })
        }
        qhir::Expression::Block {
            statements,
            expr: ret_expr,
        } => {
            let typed_statements = statements
                .iter()
                .map(|stmt| annotate_statement(ctx, stmt))
                .collect::<Result<Vec<_>, _>>()?;

            let (typed_expr, ret_ty) = if let Some(e) = ret_expr {
                let t_expr = annotate_expression(ctx, e)?;
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

fn annotate_statement<'tyc, 'run>(
    ctx: &'tyc mut CompileCtx<'run>,
    stmt: &qhir::Statement,
) -> Result<typed_hir::Statement, AstTypeError> {
    let typed_stmt = match stmt {
        qhir::Statement::Declaration {
            name,
            ty,
            val,
            range,
        } => {
            let val_expr = annotate_expression(ctx, val)?;
            typed_hir::Statement::Declaration {
                name: *name,
                range: *range,
                ty: *ty,
                val: val_expr,
            }
        }
        qhir::Statement::Assignment { name, val, range } => {
            let val_expr = annotate_expression(ctx, val)?;
            typed_hir::Statement::Assignment {
                name: *name,
                range: *range,
                val: val_expr,
            }
        }
        qhir::Statement::Expr(e) => {
            let e_expr = annotate_expression(ctx, e)?;
            typed_hir::Statement::Expr(e_expr)
        }
    };
    Ok(typed_stmt)
}
