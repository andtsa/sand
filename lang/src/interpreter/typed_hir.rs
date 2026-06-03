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
    #[error("undefined variable: {0}")]
    UndefinedVariable(String),
    #[error("assignment to undeclared variable: {0}")]
    UndeclaredAssignment(String),
    #[error("if condition must be Bool, got {0}")]
    IfCondNotBool(String),
    #[error("while condition must be Bool, got {0}")]
    WhileCondNotBool(String),
    #[error("division by zero")]
    DivisionByZero,
    #[error("type error in binary operation: {0}")]
    BinOpTypeError(String),
    #[error("type error in unary operation: {0}{1}")]
    UnOpTypeError(Uop, String),
    #[error("runtime error: {0}")]
    Runtime(String),
}

type Env = Map<UniqVar, Expression>;

impl TypedProgram {
    pub fn interpret(&self, ctx: &CompileCtx) -> Result<Expression, InterpError> {
        self.interpret_with_output(ctx, &mut std::io::stdout())
    }

    pub fn interpret_with_output(
        &self,
        ctx: &CompileCtx,
        output: &mut dyn std::io::Write,
    ) -> Result<Expression, InterpError> {
        let (_, main_fn) = self
            .functions
            .iter()
            .find(|(f, _)| ctx.is_main(**f))
            .ok_or(InterpError::NoMainFunction)?;

        self.eval_expr(&main_fn.body.expr, &mut Env::new(), ctx, output)
    }

    fn eval_expr(
        &self,
        expr: &Expression,
        env: &mut Env,
        ctx: &CompileCtx,
        output: &mut dyn std::io::Write,
    ) -> Result<Expression, InterpError> {
        match expr {
            Expression::Int(n) => Ok(Expression::Int(*n)),
            Expression::Bool(b) => Ok(Expression::Bool(*b)),
            Expression::Unit => Ok(Expression::Unit),

            Expression::Var(name) => env
                .get(name)
                .cloned()
                .ok_or(InterpError::UndefinedVariable(ctx.uniq_variable_name(name))),

            Expression::If { cond, t, f } => match self.eval_expr(&cond.expr, env, ctx, output)? {
                Expression::Bool(true) => self.eval_expr(&t.expr, env, ctx, output),
                Expression::Bool(false) => self.eval_expr(&f.expr, env, ctx, output),
                e => Err(InterpError::IfCondNotBool(e.fmt_inline(ctx))),
            },

            Expression::While { cond, body } => {
                match self.eval_expr(&cond.expr, env, ctx, output)? {
                    Expression::Bool(false) => Ok(Expression::Unit),
                    Expression::Bool(true) => {
                        self.eval_expr(&body.expr, env, ctx, output)?;
                        self.eval_expr(expr, env, ctx, output)
                    }
                    e => Err(InterpError::WhileCondNotBool(e.fmt_inline(ctx))),
                }
            }

            Expression::BinOp { left, op, right } => {
                let l = self.eval_expr(&left.expr, env, ctx, output)?;
                let r = self.eval_expr(&right.expr, env, ctx, output)?;
                eval_binop(*op, l, r, ctx)
            }

            Expression::UnOp { op, right } => {
                let v = self.eval_expr(&right.expr, env, ctx, output)?;
                eval_unop(*op, v, ctx)
            }

            Expression::Block { statements, expr } => {
                statements
                    .iter()
                    .try_for_each(|stmt| eval_stmt(self, stmt, env, ctx, output))?;
                expr.as_ref().map_or(Ok(Expression::Unit), |e| {
                    self.eval_expr(&e.expr, env, ctx, output)
                })
            }

            Expression::Call { fn_name, args } => {
                let function = &self.functions[fn_name];
                let mut call_env = function
                    .parameters
                    .iter()
                    .zip(args.iter())
                    .map(|(param, arg)| {
                        self.eval_expr(&arg.expr, env, ctx, output)
                            .map(|v| (param.name, v))
                    })
                    .collect::<Result<Env, _>>()?;
                self.eval_expr(&function.body.expr, &mut call_env, ctx, output)
            }

            Expression::IntrinsicCall { fn_name, args } => {
                let vals = args
                    .iter()
                    .map(|a| self.eval_expr(&a.expr, env, ctx, output))
                    .collect::<Result<Vec<_>, _>>()?;
                eval_intrinsic(*fn_name, vals, ctx, output)
            }

            Expression::Constructor {
                enum_ref,
                variant_idx,
            } => Ok(Expression::Constructor {
                enum_ref: *enum_ref,
                variant_idx: *variant_idx,
            }),

            Expression::Match { scrutinee, arms } => {
                let scrut_val = self.eval_expr(&scrutinee.expr, env, ctx, output)?;
                for arm in arms {
                    let matches = match &arm.pattern {
                        crate::ir_types::typed_hir::MatchPattern::Wildcard => true,
                        crate::ir_types::typed_hir::MatchPattern::Variant {
                            enum_ref,
                            variant_idx,
                        } => {
                            scrut_val
                                == Expression::Constructor {
                                    enum_ref: *enum_ref,
                                    variant_idx: *variant_idx,
                                }
                        }
                    };
                    if matches {
                        return self.eval_expr(&arm.body.expr, env, ctx, output);
                    }
                }
                // exhaustiveness is guaranteed by the type checker
                unreachable!("non-exhaustive match reached at runtime")
            }
        }
    }
}

fn eval_stmt(
    prog: &TypedProgram,
    stmt: &Statement,
    env: &mut Env,
    ctx: &CompileCtx,
    output: &mut dyn std::io::Write,
) -> Result<(), InterpError> {
    match stmt {
        Statement::Declaration { name, val, .. } => {
            let v = prog.eval_expr(&val.expr, env, ctx, output)?;
            env.insert(*name, v);
        }
        Statement::Assignment { name, val, .. } => {
            if !env.contains_key(name) {
                return Err(InterpError::UndeclaredAssignment(
                    ctx.uniq_variable_name(name),
                ));
            }
            let v = prog.eval_expr(&val.expr, env, ctx, output)?;
            env.insert(*name, v);
        }
        Statement::Expr(e) => {
            prog.eval_expr(&e.expr, env, ctx, output)?;
        }
    }
    Ok(())
}

fn eval_binop(
    op: Bop,
    l: Expression,
    r: Expression,
    ctx: &CompileCtx,
) -> Result<Expression, InterpError> {
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
        (Expression::Int(l), Expression::Int(r), Bop::And) => Ok(Expression::Int(l & r)),
        (Expression::Int(l), Expression::Int(r), Bop::Or) => Ok(Expression::Int(l | r)),
        (Expression::Int(l), Expression::Int(r), Bop::Xor) => Ok(Expression::Int(l ^ r)),
        (Expression::Bool(l), Expression::Bool(r), Bop::And) => Ok(Expression::Bool(l && r)),
        (Expression::Bool(l), Expression::Bool(r), Bop::Or) => Ok(Expression::Bool(l || r)),
        (Expression::Bool(l), Expression::Bool(r), Bop::Xor) => Ok(Expression::Bool(l ^ r)),
        (Expression::Bool(l), Expression::Bool(r), Bop::Comp(CompOp::Eq)) => {
            Ok(Expression::Bool(l == r))
        }
        (Expression::Bool(l), Expression::Bool(r), Bop::Comp(CompOp::Ne)) => {
            Ok(Expression::Bool(l != r))
        }
        (
            Expression::Constructor {
                enum_ref: er_l,
                variant_idx: vi_l,
            },
            Expression::Constructor {
                enum_ref: er_r,
                variant_idx: vi_r,
            },
            Bop::Comp(cop),
        ) => match cop {
            _ if er_l != er_r => Err(InterpError::BinOpTypeError(format!(
                "{} cannot be compared with {}",
                ctx.enum_display(er_l, vi_l),
                ctx.enum_display(er_r, vi_r)
            ))),
            CompOp::Eq => Ok(Expression::Bool(vi_l == vi_r)),
            CompOp::Ne => Ok(Expression::Bool(vi_l != vi_r)),
            CompOp::Ge => Ok(Expression::Bool(vi_l >= vi_r)),
            CompOp::Le => Ok(Expression::Bool(vi_l <= vi_r)),
            CompOp::Gt => Ok(Expression::Bool(vi_l > vi_r)),
            CompOp::Lt => Ok(Expression::Bool(vi_l < vi_r)),
        },
        (el, er, o) => Err(InterpError::BinOpTypeError(format!(
            "{} {o} {}",
            el.fmt_inline(ctx),
            er.fmt_inline(ctx)
        ))),
    }
}

fn eval_unop(op: Uop, v: Expression, ctx: &CompileCtx) -> Result<Expression, InterpError> {
    match (v, op) {
        (Expression::Bool(b), Uop::Not) => Ok(Expression::Bool(!b)),
        (Expression::Int(n), Uop::Not) => Ok(Expression::Int(!n)),
        (Expression::Int(n), Uop::Neg) => Ok(Expression::Int(-n)),
        (e, o) => Err(InterpError::UnOpTypeError(o, e.fmt_inline(ctx))),
    }
}

fn eval_intrinsic(
    fn_name: Intrinsic,
    vals: Vec<Expression>,
    ctx: &CompileCtx,
    output: &mut dyn std::io::Write,
) -> Result<Expression, InterpError> {
    let fmt = |val: &Expression| val.fmt_inline(ctx);
    match fn_name {
        Intrinsic::Println => {
            let line: Vec<String> = vals.iter().map(fmt).collect();
            let _ = writeln!(output, "{}", line.join(" "));
            Ok(Expression::Unit)
        }
        Intrinsic::Print => {
            let text: Vec<String> = vals.iter().map(fmt).collect();
            let _ = write!(output, "{}", text.join(" "));
            Ok(Expression::Unit)
        }
        Intrinsic::Abs => {
            debug_assert_eq!(vals.len(), 1, "Abs expects 1 arg");
            match &vals[0] {
                Expression::Int(n) => Ok(Expression::Int(n.abs())),
                v => Err(InterpError::Runtime(format!(
                    "__abs: expected Int, got {}",
                    fmt(v)
                ))),
            }
        }
        Intrinsic::Min => {
            debug_assert_eq!(vals.len(), 2, "Min expects 2 args");
            match (&vals[0], &vals[1]) {
                (Expression::Int(a), Expression::Int(b)) => Ok(Expression::Int(*a.min(b))),
                _ => Err(InterpError::Runtime("__min: expected (Int, Int)".into())),
            }
        }
        Intrinsic::Max => {
            debug_assert_eq!(vals.len(), 2, "Max expects 2 args");
            match (&vals[0], &vals[1]) {
                (Expression::Int(a), Expression::Int(b)) => Ok(Expression::Int(*a.max(b))),
                _ => Err(InterpError::Runtime("__max: expected (Int, Int)".into())),
            }
        }
        Intrinsic::ReadInt => {
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| InterpError::Runtime(format!("__read_int: io error: {e}")))?;
            let n = line
                .trim()
                .parse::<i64>()
                .map_err(|e| InterpError::Runtime(format!("__read_int: parse error: {e}")))?;
            Ok(Expression::Int(n))
        }
        Intrinsic::Exit => {
            debug_assert_eq!(vals.len(), 1, "Exit expects 1 arg");
            match &vals[0] {
                Expression::Int(code) => std::process::exit(*code as i32),
                v => Err(InterpError::Runtime(format!(
                    "__exit: expected Int, got {}",
                    fmt(v)
                ))),
            }
        }
    }
}
