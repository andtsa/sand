//! take a parsed and uniquified AST,
//! resolve variable and function names,
//! convert to SSA form,
//! annotate expressions with their types,
//! and output a TypedProgram AST

mod check_intrinsic;
mod errors;
mod type_check;
mod var_types;

use crate::ir_types::hhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedFunction;
use crate::lang::structure::FnName;
use crate::lang::structure::FnSig;
use crate::lang::structure::Map;
use crate::lang::structure::Range;
use crate::lang::structure::VarName;
use crate::lang::types::Ty;
use crate::passes::type_ast::check_intrinsic::get_intrinsic_call;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::var_types::FnMap;
use crate::passes::type_ast::var_types::VarMap;
use crate::passes::uniquify::reserved::assert_unique;

impl typed_hir::TypedProgram {
    pub fn from_ast_program(ast: &hhir::ProgramModule) -> Result<Self, AstTypeError> {
        assert_unique(ast).map_err(AstTypeError::UniquifyError)?;

        let avail_fns = ast
            .functions
            .iter()
            .map(|f| {
                (
                    f.name.clone(),
                    (FnName::from(f), FnSig::with(&f.parameters, f.ret_type)),
                )
            })
            .collect();

        let mut avail_vars = Map::new(); // variables are only available within function bodies, so we can start with an empty map here and fill it in as we go through the functions
        let fn_list = ast
            .functions
            .iter()
            .map(|f| annotate_function(f, &avail_fns, &mut avail_vars))
            .collect::<Result<Vec<(FnName, TypedFunction)>, _>>()?;

        let functions = fn_list.into_iter().collect::<Map<_, _>>();

        let prog = typed_hir::TypedProgram {
            avail_vars: avail_vars.into_iter().map(|(_, (v, t))| (v, t)).collect(),
            functions,
        };

        type_check::check_program(&prog)?;

        Ok(prog)
    }
}

fn retrieve_variable(x: &str, range: Range, map: &VarMap) -> Result<(VarName, Ty), AstTypeError> {
    map.get(x)
        .ok_or_else(|| AstTypeError::UnboundVariable {
            name: x.to_string(),
            range,
        })
        .map(|(v, t)| (v.clone(), *t))
}

pub fn annotate_function(
    func: &hhir::Function,
    avail_fns: &FnMap,
    avail_vars: &mut VarMap,
) -> Result<(FnName, typed_hir::TypedFunction), AstTypeError> {
    let mut var_types = var_types::collect_variables(func, avail_fns)?;

    let body = annotate_expression(&func.body, avail_fns, &var_types)?;

    avail_vars.append(&mut var_types);

    Ok((
        FnName::from(func),
        typed_hir::TypedFunction {
            name: FnName::from(func),
            range: func.range,
            parameters: func
                .parameters
                .iter()
                .map(|p| typed_hir::Parameter {
                    name: VarName::from(p),
                    ty: p.ty,
                    range: p.range,
                })
                .collect(),
            ret_type: func.ret_type,
            body,
        },
    ))
}

fn annotate_expression(
    expr: &hhir::Expr,
    avail_fns: &FnMap,
    var_types: &VarMap,
) -> Result<typed_hir::Expr, AstTypeError> {
    match &expr.expr {
        hhir::Expression::Int(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Int(*x),
            range: expr.range,
            ty: Ty::Int,
        }),
        hhir::Expression::Bool(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Bool(*x),
            range: expr.range,
            ty: Ty::Bool,
        }),
        hhir::Expression::Unit => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Unit,
            range: expr.range,
            ty: Ty::Unit,
        }),
        hhir::Expression::Var(x) => {
            let (name, ty) = retrieve_variable(x, expr.range, var_types)?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::RVar(name),
                range: expr.range,
                ty,
            })
        }
        hhir::Expression::BinOp { left, op, right } => {
            let left_expr = annotate_expression(left, avail_fns, var_types)?;
            let right_expr = annotate_expression(right, avail_fns, var_types)?;

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
        hhir::Expression::UnOp { op, right } => {
            let right_expr = annotate_expression(right, avail_fns, var_types)?;

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
        hhir::Expression::If { cond, t, f } => {
            let cond_expr = annotate_expression(cond, avail_fns, var_types)?;
            let t_expr = annotate_expression(t, avail_fns, var_types)?;
            let f_expr = annotate_expression(f, avail_fns, var_types)?;

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
        hhir::Expression::While { cond, body } => {
            let cond_expr = annotate_expression(cond, avail_fns, var_types)?;
            let body_expr = annotate_expression(body, avail_fns, var_types)?;
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
        hhir::Expression::Call { fn_name, args } => {
            let arg_exprs_and_tys = args
                .iter()
                .map(|arg| annotate_expression(arg, avail_fns, var_types))
                .collect::<Result<Vec<_>, _>>()?;

            if let Some((intrinsic_call, ret_ty)) = get_intrinsic_call(fn_name, &arg_exprs_and_tys)
            {
                Ok(typed_hir::Expr {
                    expr: intrinsic_call,
                    ty: ret_ty,
                    range: expr.range,
                })
            } else {
                let (_, (fn_ref, fn_sig)) = avail_fns.get_key_value(fn_name).ok_or_else(|| {
                    AstTypeError::UndefinedFunction {
                        name: fn_name.clone(),
                        range: expr.range,
                    }
                })?;

                Ok(typed_hir::Expr {
                    expr: typed_hir::Expression::Call {
                        fn_name: fn_ref.clone(),
                        args: arg_exprs_and_tys,
                    },
                    range: expr.range,
                    ty: fn_sig.ret_ty,
                })
            }
        }
        hhir::Expression::Block {
            statements,
            expr: ret_expr,
        } => {
            let typed_statements = statements
                .iter()
                .map(|stmt| annotate_statement(stmt, avail_fns, var_types))
                .collect::<Result<Vec<_>, _>>()?;

            let (typed_expr, ret_ty) = if let Some(e) = ret_expr {
                let t_expr = annotate_expression(e, avail_fns, var_types)?;
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

fn annotate_statement(
    stmt: &hhir::Statement,
    avail_fns: &FnMap,
    var_types: &VarMap,
) -> Result<typed_hir::Statement, AstTypeError> {
    let typed_stmt = match stmt {
        d @ hhir::Statement::Declaration {
            name: _,
            ty,
            val,
            range,
        } => {
            let val_expr = annotate_expression(val, avail_fns, var_types)?;
            typed_hir::Statement::Declaration {
                name: VarName::try_from(d).unwrap(),
                range: *range,
                ty: *ty,
                val: val_expr,
            }
        }
        hhir::Statement::Assignment { name, val, range } => {
            let val_expr = annotate_expression(val, avail_fns, var_types)?;
            let (name, _) = retrieve_variable(name, *range, var_types)?;
            typed_hir::Statement::Assignment {
                name,
                range: *range,
                val: val_expr,
            }
        }
        hhir::Statement::Expr(e) => {
            let e_expr = annotate_expression(e, avail_fns, var_types)?;
            typed_hir::Statement::Expr(e_expr)
        }
    };
    Ok(typed_stmt)
}
