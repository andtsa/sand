//! a simple interpreter for the typed_hir IR

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Map;
use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;

#[derive(Debug, thiserror::Error)]
pub enum InterpError {
    #[error("no main function found")]
    NoMainFunction,
    #[error("undefined variable: {0:?}")]
    UndefinedVariable(UniqVar),
    #[error("assignment to undeclared variable: {0:?}")]
    UndeclaredAssignment(UniqVar),
    #[error("if condition must be Bool, got {0:?}")]
    IfCondNotBool(Expression),
    #[error("while condition must be Bool, got {0:?}")]
    WhileCondNotBool(Expression),
    #[error("division by zero")]
    DivisionByZero,
    #[error("type error in binary operation: {0:?}")]
    BinOpTypeError(String),
    #[error("type error in unary operation: {0:?}")]
    UnOpTypeError(String),
}

type Env = Map<UniqVar, Expression>;

impl TypedProgram {
    pub fn interpret(&self, ctx: &CompileCtx) -> Result<Expression, InterpError> {
        let (_, main_fn) = self
            .functions
            .iter()
            .find(|(f, _)| ctx.is_main(**f))
            .ok_or(InterpError::NoMainFunction)?;

        self.eval_expr(&main_fn.body.expr, &mut Env::new())
    }

    fn eval_expr(&self, expr: &Expression, env: &mut Env) -> Result<Expression, InterpError> {
        match expr {
            Expression::Int(n) => Ok(Expression::Int(*n)),
            Expression::Bool(b) => Ok(Expression::Bool(*b)),
            Expression::Unit => Ok(Expression::Unit),

            Expression::Var(name) => env
                .get(name)
                .cloned()
                .ok_or(InterpError::UndefinedVariable(*name)),

            Expression::If { cond, t, f } => match self.eval_expr(&cond.expr, env)? {
                Expression::Bool(true) => self.eval_expr(&t.expr, env),
                Expression::Bool(false) => self.eval_expr(&f.expr, env),
                e => Err(InterpError::IfCondNotBool(e)),
            },

            Expression::While { cond, body } => match self.eval_expr(&cond.expr, env)? {
                Expression::Bool(false) => Ok(Expression::Unit),
                Expression::Bool(true) => {
                    self.eval_expr(&body.expr, env)?;
                    self.eval_expr(expr, env)
                }
                e => Err(InterpError::WhileCondNotBool(e)),
            },

            Expression::BinOp { left, op, right } => {
                let l = self.eval_expr(&left.expr, env)?;
                let r = self.eval_expr(&right.expr, env)?;
                eval_binop(*op, l, r)
            }

            Expression::UnOp { op, right } => {
                let v = self.eval_expr(&right.expr, env)?;
                eval_unop(*op, v)
            }

            Expression::Block { statements, expr } => {
                statements
                    .iter()
                    .try_for_each(|stmt| eval_stmt(self, stmt, env))?;
                expr.as_ref()
                    .map_or(Ok(Expression::Unit), |e| self.eval_expr(&e.expr, env))
            }

            Expression::Call { fn_name, args } => {
                let function = &self.functions[fn_name];
                let mut call_env = function
                    .parameters
                    .iter()
                    .zip(args.iter())
                    .map(|(param, arg)| self.eval_expr(&arg.expr, env).map(|v| (param.name, v)))
                    .collect::<Result<Env, _>>()?;
                self.eval_expr(&function.body.expr, &mut call_env)
            }

            Expression::IntrinsicCall { fn_name, args } => {
                let vals = args
                    .iter()
                    .map(|a| self.eval_expr(&a.expr, env))
                    .collect::<Result<Vec<_>, _>>()?;
                eval_intrinsic(*fn_name, vals)
            }
        }
    }
}

fn eval_stmt(prog: &TypedProgram, stmt: &Statement, env: &mut Env) -> Result<(), InterpError> {
    match stmt {
        Statement::Declaration { name, val, .. } => {
            let v = prog.eval_expr(&val.expr, env)?;
            env.insert(*name, v);
        }
        Statement::Assignment { name, val, .. } => {
            if !env.contains_key(name) {
                return Err(InterpError::UndeclaredAssignment(*name));
            }
            let v = prog.eval_expr(&val.expr, env)?;
            env.insert(*name, v);
        }
        Statement::Expr(e) => {
            prog.eval_expr(&e.expr, env)?;
        }
    }
    Ok(())
}

fn eval_binop(op: Bop, l: Expression, r: Expression) -> Result<Expression, InterpError> {
    match (l, r, op) {
        (Expression::Int(l), Expression::Int(r), Bop::Plus) => {
            Ok(Expression::Int(l.overflowing_add(r).0))
        }
        (Expression::Int(l), Expression::Int(r), Bop::Minus) => {
            Ok(Expression::Int(l.overflowing_sub(r).0))
        }
        (Expression::Int(l), Expression::Int(r), Bop::Mult) => {
            Ok(Expression::Int(l.overflowing_mul(r).0))
        }
        (Expression::Int(l), Expression::Int(r), Bop::Div) => {
            if r == 0 {
                return Err(InterpError::DivisionByZero);
            }
            Ok(Expression::Int(l / r))
        }
        (Expression::Int(l), Expression::Int(r), Bop::Pow) => Ok(Expression::Int(l.pow(r as u32))),
        (Expression::Int(l), Expression::Int(r), Bop::Comp(cop)) => match cop {
            CompOp::Eq => Ok(Expression::Bool(l == r)),
            CompOp::Ne => Ok(Expression::Bool(l != r)),
            CompOp::Lt => Ok(Expression::Bool(l < r)),
            CompOp::Le => Ok(Expression::Bool(l <= r)),
            CompOp::Gt => Ok(Expression::Bool(l > r)),
            CompOp::Ge => Ok(Expression::Bool(l >= r)),
        },
        (Expression::Bool(l), Expression::Bool(r), Bop::And) => Ok(Expression::Bool(l && r)),
        (Expression::Bool(l), Expression::Bool(r), Bop::Or) => Ok(Expression::Bool(l || r)),
        (Expression::Bool(l), Expression::Bool(r), Bop::Xor) => Ok(Expression::Bool(l ^ r)),
        (Expression::Bool(l), Expression::Bool(r), Bop::Comp(CompOp::Eq)) => {
            Ok(Expression::Bool(l == r))
        }
        (Expression::Bool(l), Expression::Bool(r), Bop::Comp(CompOp::Ne)) => {
            Ok(Expression::Bool(l != r))
        }
        (x, y, z) => Err(InterpError::BinOpTypeError(format!("{x:?} {z:?} {y:?}"))),
    }
}

fn eval_unop(op: Uop, v: Expression) -> Result<Expression, InterpError> {
    match (v, op) {
        (Expression::Bool(b), Uop::Not) => Ok(Expression::Bool(!b)),
        (Expression::Int(n), Uop::Neg) => Ok(Expression::Int(-n)),
        (x, y) => Err(InterpError::UnOpTypeError(format!("{y:?} {x:?}"))),
    }
}

fn eval_intrinsic(fn_name: Intrinsic, vals: Vec<Expression>) -> Result<Expression, InterpError> {
    match fn_name {
        Intrinsic::Println => {
            for val in vals {
                print!("{:?} ", val);
            }
            println!();
            Ok(Expression::Unit)
        }
        Intrinsic::Print => {
            for val in vals {
                print!("{:?} ", val);
            }
            Ok(Expression::Unit)
        }
    }
}
