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
                payload,
            } => {
                let payload_val = match payload {
                    Some(p) => Some(Box::new(Expr {
                        expr: self.eval_expr(&p.expr, env, ctx, output)?,
                        ty: p.ty,
                        range: p.range,
                    })),
                    None => None,
                };
                Ok(Expression::Constructor {
                    enum_ref: *enum_ref,
                    variant_idx: *variant_idx,
                    payload: payload_val,
                })
            }

            Expression::Tuple(elems) => {
                let vals = elems
                    .iter()
                    .map(|e| {
                        Ok(Expr {
                            expr: self.eval_expr(&e.expr, env, ctx, output)?,
                            ty: e.ty,
                            range: e.range,
                        })
                    })
                    .collect::<Result<Vec<_>, InterpError>>()?;
                Ok(Expression::Tuple(vals))
            }

            Expression::Match { scrutinee, arms } => {
                let scrut_val = self.eval_expr(&scrutinee.expr, env, ctx, output)?;
                for arm in arms {
                    // try-and-fall-back: clone `env`, attempt to match *and*
                    // bind in one recursive pass; only `Variant` sub-checks
                    // can fail (every other pattern form is irrefutable, see
                    // decision D1 in DESTRUCTURING_PATTERNS.todo.md), so a
                    // failed attempt simply discards its (partially-bound)
                    // env clone and moves on to the next arm.
                    let mut arm_env = env.clone();
                    if bind_pattern(&arm.pattern, &scrut_val, &mut arm_env) {
                        return self.eval_expr(&arm.body.expr, &mut arm_env, ctx, output);
                    }
                }
                // exhaustiveness is guaranteed by the type checker
                unreachable!("non-exhaustive match reached at runtime")
            }
        }
    }
}

/// recursively match `pattern` against an already-evaluated scrutinee `value`,
/// extending `env` with any bindings the pattern introduces along the way.
/// returns whether the pattern matched.
///
/// only `Variant` patterns can fail to match (by tag mismatch) — every other
/// pattern form (`Wildcard`, `Binding`, `Tuple`) is irrefutable by
/// construction (decision D1 in `DESTRUCTURING_PATTERNS.todo.md`: only
/// bindings, wildcards, and recursive tuple-destructuring are allowed in
/// sub-pattern position, so once the top-level tag matches every sub-pattern
/// is guaranteed to match too).
fn bind_pattern(pattern: &MatchPattern, value: &Expression, env: &mut Env) -> bool {
    match pattern {
        MatchPattern::Wildcard => true,
        MatchPattern::Binding { var, .. } => {
            env.insert(*var, value.clone());
            true
        }
        MatchPattern::Variant {
            enum_ref,
            variant_idx,
            payload,
        } => match value {
            Expression::Constructor {
                enum_ref: er,
                variant_idx: vi,
                payload: val_payload,
            } if er == enum_ref && vi == variant_idx => match (payload, val_payload) {
                (None, _) => true,
                (Some((_, sub_pat)), Some(sub_val)) => bind_pattern(sub_pat, &sub_val.expr, env),
                (Some(_), None) => unreachable!(
                    "ill-typed match: pattern destructures a payload but the value carries none \
                     (the type checker should have rejected this)"
                ),
            },
            _ => false,
        },
        MatchPattern::Tuple {
            elems: sub_patterns,
            ..
        } => match value {
            Expression::Tuple(elems) => sub_patterns
                .iter()
                .zip(elems.iter())
                .all(|(p, e)| bind_pattern(p, &e.expr, env)),
            _ => unreachable!(
                "ill-typed match: tuple pattern against a non-tuple value \
                 (the type checker should have rejected this)"
            ),
        },
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
                payload: ref pl_l,
            },
            Expression::Constructor {
                enum_ref: er_r,
                variant_idx: vi_r,
                payload: ref pl_r,
            },
            Bop::Comp(cop @ (CompOp::Eq | CompOp::Ne)),
        ) => {
            if er_l != er_r {
                return Err(InterpError::BinOpTypeError(format!(
                    "{} cannot be compared with {}",
                    ctx.enum_display(er_l, vi_l),
                    ctx.enum_display(er_r, vi_r)
                )));
            }
            let eq = vi_l == vi_r && pl_l == pl_r;
            match cop {
                CompOp::Eq => Ok(Expression::Bool(eq)),
                CompOp::Ne => Ok(Expression::Bool(!eq)),
                _ => unreachable!(),
            }
        }
        (
            Expression::Constructor {
                enum_ref: er_l,
                variant_idx: vi_l,
                ..
            },
            Expression::Constructor {
                enum_ref: er_r,
                variant_idx: vi_r,
                ..
            },
            Bop::Comp(cop),
        ) => {
            if er_l != er_r {
                return Err(InterpError::BinOpTypeError(format!(
                    "{} cannot be compared with {}",
                    ctx.enum_display(er_l, vi_l),
                    ctx.enum_display(er_r, vi_r)
                )));
            }
            match cop {
                CompOp::Ge => Ok(Expression::Bool(vi_l >= vi_r)),
                CompOp::Le => Ok(Expression::Bool(vi_l <= vi_r)),
                CompOp::Gt => Ok(Expression::Bool(vi_l > vi_r)),
                CompOp::Lt => Ok(Expression::Bool(vi_l < vi_r)),
                _ => unreachable!(),
            }
        }
        (Expression::Tuple(l), Expression::Tuple(r), Bop::Comp(CompOp::Eq)) => {
            Ok(Expression::Bool(l == r))
        }
        (Expression::Tuple(l), Expression::Tuple(r), Bop::Comp(CompOp::Ne)) => {
            Ok(Expression::Bool(l != r))
        }
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
