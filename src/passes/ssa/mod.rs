//! convert a typed program to SSA form

use crate::ir_types::ssa::SsaFunction;
use crate::ir_types::ssa::SsaProgram;
use crate::ir_types::ssa::{self};
use crate::ir_types::typed_hir::TypedFunction;
use crate::ir_types::typed_hir::TypedProgram;
use crate::ir_types::typed_hir::{self};
use crate::lang::structure::Map;
use crate::lang::structure::VarName;

impl SsaProgram {
    pub fn ssa_form(prog: &TypedProgram) -> Self {
        let functions = prog
            .functions
            .iter()
            .map(|(name, body)| {
                let mut env = Map::new();
                (name.clone(), ssa_function(body, &mut env))
            })
            .collect();

        Self { functions }
    }
}

fn ssa_function(func: &TypedFunction, env: &mut Map<VarName, Vec<VarName>>) -> SsaFunction {
    let parameters = func
        .parameters
        .iter()
        .map(|p| ssa::Parameter {
            name: p.name.clone(),
            ty: p.ty,
            range: p.range,
        })
        .collect();

    let body = ssa_expr(&func.body, env);

    SsaFunction {
        name: func.name.clone(),
        range: func.range,
        parameters,
        ret_type: func.ret_type,
        body,
    }
}

fn ssa_expr(expr: &typed_hir::Expr, env: &mut Map<VarName, Vec<VarName>>) -> ssa::Expr {
    let body = match &expr.expr {
        typed_hir::Expression::Int(x) => ssa::Expression::Int(*x),
        typed_hir::Expression::Bool(x) => ssa::Expression::Bool(*x),
        typed_hir::Expression::Unit => ssa::Expression::Unit,
        typed_hir::Expression::BinOp { left, op, right } => ssa::Expression::BinOp {
            left: Box::new(ssa_expr(&left, env)),
            op: *op,
            right: Box::new(ssa_expr(&right, env)),
        },
        typed_hir::Expression::Call { fn_name, args } => ssa::Expression::Call {
            fn_name: fn_name.clone(),
            args: args.iter().map(|p| ssa_expr(p, env)).collect(),
        },
        typed_hir::Expression::IntrinsicCall { fn_name, args } => ssa::Expression::IntrinsicCall {
            fn_name: *fn_name,
            args: args.iter().map(ssa_expr, env).collect(),
        },
        typed_hir::Expression::UnOp { op, right } => ssa::Expression::UnOp {
            op: *op,
            right: Box::new(ssa_expr(&right, env)),
        },
        typed_hir::Expression::If { cond, t, f } => ssa::Expression::If {
            cond: Box::new(ssa_expr(&cond, env)),
            t: Box::new(ssa_expr(&t, env)),
            f: Box::new(ssa_expr(&f, env)),
        },
        typed_hir::Expression::While { cond, body } => ssa::Expression::While {
            cond: Box::new(ssa_expr(&cond, env)),
            body: Box::new(ssa_expr(&body, env)),
        },
        typed_hir::Expression::RVar(x) => {
            todo!()
        }
        typed_hir::Expression::Block { statements, expr } => {
            todo!()
        }
    };

    ssa::Expr {
        expr: body,
        ty: expr.ty,
        range: expr.range,
    }
}
