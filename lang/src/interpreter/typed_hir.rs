//! a simple interpreter for the typed_hir IR
//!
//! ## Store model (R4)
//!
//! Runtime values live in a graph of mutable **cells** so that references
//! behave like the real pointers the LLVM backend emits, faithful to the
//! Calculus's `BorrowedMut` semantics (§3.2, §6.4): a mutable borrow denotes a
//! *storage location*, and a write through it (`*r = e`) mutates that location
//! observably to every alias — including across function calls.
//!
//! Each variable binding owns a [`Cell`]; a reference value ([`Value::Ref`]) is
//! a *shared handle* to a cell. `&e` / `&mut e` evaluate the operand *as a
//! place* ([`TypedProgram::eval_place`]) to obtain its cell; `*r` reads/writes
//! that cell. The `Rc` is a meta-level detail of the interpreter, not
//! language-level GC: the static region/escape checker guarantees no dangling,
//! so the interpreter never models deallocation.
//!
//! The value domain is the internal [`Value`] type, not `Expression`, so that
//! the store concept stays out of the IR. Public entry points convert the final
//! [`Value`] back to an `Expression` ([`value_to_expr`]) so callers and tests
//! are unaffected.

use std::cell::RefCell;
use std::rc::Rc;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Map;
use crate::compiler::structure::TypeHead;
use crate::compiler::structure::TypeclassRef;
use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::types::EnumRef;
use crate::lang::types::Kind;
use crate::lang::types::TyKind;

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

/// A storage cell: shared, interior-mutable. A variable binding owns one; a
/// [`Value::Ref`] is a shared handle to one.
type Cell<'tcx> = Rc<RefCell<Value<'tcx>>>;

/// The interpreter's runtime value domain. Mirrors the value-shaped subset of
/// `Expression`, plus [`Value::Ref`] — a reference handle into the cell store.
#[derive(Debug, Clone, PartialEq)]
enum Value<'tcx> {
    Int(i64),
    Bool(bool),
    Unit,
    Constructor {
        enum_ref: EnumRef<'tcx>,
        variant_idx: usize,
        payload: Option<Box<Value<'tcx>>>,
    },
    Tuple(Vec<Value<'tcx>>),
    /// A reference: a shared handle to the cell it points at. Produced by a
    /// borrow expression, consumed by `*r` reads and `*r = e` writes.
    Ref(Cell<'tcx>),
}

/// Bindings map a variable to the cell that holds its value.
type Env<'tcx> = Map<UniqVar<'tcx>, Cell<'tcx>>;

fn cell<'tcx>(v: Value<'tcx>) -> Cell<'tcx> {
    Rc::new(RefCell::new(v))
}

/// The typeclass-instance head of a runtime value, for dynamic method dispatch.
fn value_head<'tcx>(v: &Value<'tcx>) -> Option<TypeHead<'tcx>> {
    match v {
        Value::Int(_) => Some(TypeHead::Int),
        Value::Bool(_) => Some(TypeHead::Bool),
        Value::Unit => Some(TypeHead::Unit),
        Value::Constructor { enum_ref, .. } => Some(TypeHead::Enum(*enum_ref)),
        Value::Tuple(_) | Value::Ref(_) => None,
    }
}

impl<'tcx> TypedProgram<'tcx> {
    pub fn interpret(&self, ctx: &CompileCtx<'tcx>) -> Result<Expression<'tcx>, InterpError> {
        self.interpret_with_output(ctx, &mut std::io::stdout())
    }

    pub fn interpret_with_output(
        &self,
        ctx: &CompileCtx<'tcx>,
        output: &mut dyn std::io::Write,
    ) -> Result<Expression<'tcx>, InterpError> {
        let (_, main_fn) = self
            .functions
            .iter()
            .find(|(f, _)| ctx.is_main(**f))
            .ok_or(InterpError::NoMainFunction)?;

        let val = self.eval_expr(&main_fn.body.expr, &mut Env::new(), ctx, output)?;
        Ok(value_to_expr(val, ctx))
    }

    fn eval_expr(
        &self,
        expr: &Expression<'tcx>,
        env: &mut Env<'tcx>,
        ctx: &CompileCtx<'tcx>,
        output: &mut dyn std::io::Write,
    ) -> Result<Value<'tcx>, InterpError> {
        match expr {
            Expression::Int(n) => Ok(Value::Int(*n)),
            Expression::Bool(b) => Ok(Value::Bool(*b)),
            Expression::Unit => Ok(Value::Unit),

            // `&e` / `&mut e`: evaluate the operand as a *place* and take a shared
            // handle to its cell. A borrow of a variable shares that variable's
            // cell, so a later `*r = e` writes back to it.
            Expression::Borrow(inner, _) => Ok(Value::Ref(self.eval_place(
                &inner.expr,
                env,
                ctx,
                output,
            )?)),
            // `*r`: load through the reference.
            Expression::Deref(inner) => match self.eval_expr(&inner.expr, env, ctx, output)? {
                Value::Ref(c) => Ok(c.borrow().clone()),
                v => Ok(v),
            },

            Expression::Var(name) => env
                .get(name)
                .map(|c| c.borrow().clone())
                .ok_or(InterpError::UndefinedVariable(ctx.uniq_variable_name(name))),

            Expression::If { cond, t, f } => match self.eval_expr(&cond.expr, env, ctx, output)? {
                Value::Bool(true) => self.eval_expr(&t.expr, env, ctx, output),
                Value::Bool(false) => self.eval_expr(&f.expr, env, ctx, output),
                e => Err(InterpError::IfCondNotBool(fmt_value(&e, ctx))),
            },

            Expression::While { cond, body } => {
                match self.eval_expr(&cond.expr, env, ctx, output)? {
                    Value::Bool(false) => Ok(Value::Unit),
                    Value::Bool(true) => {
                        self.eval_expr(&body.expr, env, ctx, output)?;
                        self.eval_expr(expr, env, ctx, output)
                    }
                    e => Err(InterpError::WhileCondNotBool(fmt_value(&e, ctx))),
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
                expr.as_ref().map_or(Ok(Value::Unit), |e| {
                    self.eval_expr(&e.expr, env, ctx, output)
                })
            }

            Expression::Call { fn_name, args } => {
                // External (FFI) functions (Memory Step A) have no body; dispatch
                // the known C symbols to simulated-heap built-ins.
                if ctx.is_extern(*fn_name) {
                    let vals = args
                        .iter()
                        .map(|a| self.eval_expr(&a.expr, env, ctx, output))
                        .collect::<Result<Vec<_>, _>>()?;
                    let symbol = ctx
                        .extern_symbol(*fn_name)
                        .expect("extern fn has a registered symbol");
                    return eval_extern(symbol, vals);
                }
                let function = &self.functions[fn_name];
                let mut call_env = function
                    .parameters
                    .iter()
                    .zip(args.iter())
                    .map(|(param, arg)| {
                        // arg values that are `Ref`s share their `Rc` into the
                        // callee's parameter cell, so the callee writes back to
                        // the caller's storage (cross-frame write-through).
                        self.eval_expr(&arg.expr, env, ctx, output)
                            .map(|v| (param.name, cell(v)))
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

            // A deferred (generic) typeclass method call: resolve the instance
            // from the runtime receiver value's head type, then call its method.
            Expression::MethodCall {
                class,
                method,
                args,
                ..
            } => self.eval_method_call(*class, method, args, env, ctx, output),

            Expression::Constructor {
                enum_ref,
                variant_idx,
                payload,
            } => {
                let payload_val = match payload {
                    Some(p) => Some(Box::new(self.eval_expr(&p.expr, env, ctx, output)?)),
                    None => None,
                };
                Ok(Value::Constructor {
                    enum_ref: *enum_ref,
                    variant_idx: *variant_idx,
                    payload: payload_val,
                })
            }

            Expression::Tuple(elems) => {
                let vals = elems
                    .iter()
                    .map(|e| self.eval_expr(&e.expr, env, ctx, output))
                    .collect::<Result<Vec<_>, InterpError>>()?;
                Ok(Value::Tuple(vals))
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

    /// Evaluate an expression *as a place* (lvalue), returning the storage cell
    /// it denotes. `&e`/`*r = e` use this to share / write through a location.
    ///
    /// Evaluate a deferred typeclass method call (a generic dispatch the type
    /// checker left unresolved). The instance is selected from the *runtime*
    /// receiver value's head type (the argument whose method-parameter is the
    /// class parameter), then the impl method is called like an ordinary
    /// function.
    fn eval_method_call(
        &self,
        class: TypeclassRef,
        method: &str,
        args: &[Expr<'tcx>],
        env: &mut Env<'tcx>,
        ctx: &CompileCtx<'tcx>,
        output: &mut dyn std::io::Write,
    ) -> Result<Value<'tcx>, InterpError> {
        let arg_vals: Vec<Value<'tcx>> = args
            .iter()
            .map(|a| self.eval_expr(&a.expr, env, ctx, output))
            .collect::<Result<_, _>>()?;

        // the receiver = the argument at the method-parameter that is the class
        // parameter (default to the first argument).
        let cdef = ctx.get_typeclass(class);
        let class_param = cdef.param;
        let recv_idx = cdef.methods[method]
            .param_tys
            .iter()
            .position(|t| matches!(t.kind(), TyKind::Param(id) if *id == class_param))
            .unwrap_or(0);
        let head = value_head(&arg_vals[recv_idx]).ok_or_else(|| {
            InterpError::Runtime(format!(
                "no typeclass instance head for the receiver of {method}"
            ))
        })?;
        let impl_fn = ctx
            .lookup_instance(class, head)
            .and_then(|d| d.methods.get(method).copied())
            .ok_or_else(|| {
                InterpError::Runtime(format!("no instance providing method {method}"))
            })?;

        let function = &self.functions[&impl_fn];
        let mut call_env = function
            .parameters
            .iter()
            .zip(arg_vals)
            .map(|(param, v)| (param.name, cell(v)))
            .collect::<Env>();
        self.eval_expr(&function.body.expr, &mut call_env, ctx, output)
    }

    /// - `x` resolves to the variable's own cell (so `&mut x` aliases `x`).
    /// - `*r` resolves to the cell `r` points at (reborrow).
    /// - any other expression is a temporary: it is evaluated and a fresh cell
    ///   is materialised to hold it (e.g. `&mut 7`); the temporary outlives the
    ///   reference via the `Rc`.
    fn eval_place(
        &self,
        expr: &Expression<'tcx>,
        env: &mut Env<'tcx>,
        ctx: &CompileCtx<'tcx>,
        output: &mut dyn std::io::Write,
    ) -> Result<Cell<'tcx>, InterpError> {
        match expr {
            Expression::Var(name) => env
                .get(name)
                .cloned()
                .ok_or(InterpError::UndefinedVariable(ctx.uniq_variable_name(name))),
            Expression::Deref(inner) => match self.eval_expr(&inner.expr, env, ctx, output)? {
                Value::Ref(c) => Ok(c),
                v => Ok(cell(v)),
            },
            _ => Ok(cell(self.eval_expr(expr, env, ctx, output)?)),
        }
    }
}

/// recursively match `pattern` against an already-evaluated scrutinee `value`,
/// extending `env` with any bindings the pattern introduces along the way.
/// returns whether the pattern matched.
///
/// only `Variant` patterns can fail to match (by tag mismatch), every other
/// pattern form (`Wildcard`, `Binding`, `Tuple`) is irrefutable by
/// construction (decision D1 in `DESTRUCTURING_PATTERNS.todo.md`: only
/// bindings, wildcards, and recursive tuple-destructuring are allowed in
/// sub-pattern position, so once the top-level tag matches every sub-pattern
/// is guaranteed to match too).
fn bind_pattern<'tcx>(
    pattern: &MatchPattern<'tcx>,
    value: &Value<'tcx>,
    env: &mut Env<'tcx>,
) -> bool {
    match pattern {
        MatchPattern::Wildcard => true,
        MatchPattern::Binding { var, .. } => {
            env.insert(*var, cell(value.clone()));
            true
        }
        MatchPattern::Variant {
            enum_ref,
            variant_idx,
            payload,
            ..
        } => match value {
            Value::Constructor {
                enum_ref: er,
                variant_idx: vi,
                payload: val_payload,
            } if er == enum_ref && vi == variant_idx => match (payload, val_payload) {
                (None, _) => true,
                (Some((_, sub_pat)), Some(sub_val)) => bind_pattern(sub_pat, sub_val, env),
                (Some(_), None) => unreachable!(
                    "ill-typed match: pattern destructures a payload but the value carries none \
                     (the type checker should have rejected this)"
                ),
            },
            _ => false,
        },
        MatchPattern::IntLit(n) => matches!(value, Value::Int(v) if v == n),
        MatchPattern::BoolLit(b) => matches!(value, Value::Bool(v) if v == b),
        MatchPattern::Tuple {
            elems: sub_patterns,
            ..
        } => match value {
            Value::Tuple(elems) => sub_patterns
                .iter()
                .zip(elems.iter())
                .all(|(p, e)| bind_pattern(p, e, env)),
            _ => unreachable!(
                "ill-typed match: tuple pattern against a non-tuple value \
                 (the type checker should have rejected this)"
            ),
        },
    }
}

fn eval_stmt<'tcx>(
    prog: &TypedProgram<'tcx>,
    stmt: &Statement<'tcx>,
    env: &mut Env<'tcx>,
    ctx: &CompileCtx<'tcx>,
    output: &mut dyn std::io::Write,
) -> Result<(), InterpError> {
    match stmt {
        Statement::Declaration { name, val, .. } => {
            let v = prog.eval_expr(&val.expr, env, ctx, output)?;
            env.insert(*name, cell(v));
        }
        // `x = e`: reseat the variable, writing into its existing cell in place so
        // any outstanding reference to `x` observes the new value.
        Statement::Assignment { name, val, .. } => {
            let Some(slot) = env.get(name).cloned() else {
                return Err(InterpError::UndeclaredAssignment(
                    ctx.uniq_variable_name(name),
                ));
            };
            let v = prog.eval_expr(&val.expr, env, ctx, output)?;
            *slot.borrow_mut() = v;
        }
        // Write-through `*r = e` (Calculus §3.2): evaluate the reference to the
        // cell it points at and store the new value into it — observable to every
        // alias, matching the LLVM `store` through the pointer.
        Statement::DerefAssign {
            reference, value, ..
        } => {
            let v = prog.eval_expr(&value.expr, env, ctx, output)?;
            match prog.eval_expr(&reference.expr, env, ctx, output)? {
                Value::Ref(c) => {
                    *c.borrow_mut() = v;
                }
                other => unreachable!(
                    "ill-typed write-through: `*r = e` where r is not a reference ({other:?}) \
                     (the type checker should have rejected this)"
                ),
            }
        }
        Statement::LetTuple { elems, val, .. } => {
            let tuple_val = prog.eval_expr(&val.expr, env, ctx, output)?;
            match tuple_val {
                Value::Tuple(elem_vals) => {
                    for ((name, ..), elem_val) in elems.iter().zip(elem_vals.into_iter()) {
                        env.insert(*name, cell(elem_val));
                    }
                }
                _ => unreachable!(
                    "let-tuple: RHS evaluated to a non-tuple value \
                     (the type checker should have rejected this)"
                ),
            }
        }

        Statement::LetPattern {
            pattern,
            val,
            else_branch,
            ..
        } => {
            // Evaluate the main value; try to match it against the pattern.
            // If the match fails, evaluate the fallback. the type checker
            // guarantees the fallback is a constructor of the same variant,
            // so bind_pattern on the fallback always succeeds.
            let main_val = prog.eval_expr(&val.expr, env, ctx, output)?;
            if bind_pattern(pattern, &main_val, env) {
                // Pattern matched: bindings already inserted by bind_pattern.
                return Ok(());
            }
            let source = prog.eval_expr(&else_branch.expr, env, ctx, output)?;
            let ok = bind_pattern(pattern, &source, env);
            debug_assert!(
                ok,
                "let-pattern else branch did not match the pattern (type checker should prevent this)"
            );
        }

        Statement::Expr(e) => {
            prog.eval_expr(&e.expr, env, ctx, output)?;
        }
    }
    Ok(())
}

fn eval_binop<'tcx>(
    op: Bop,
    l: Value<'tcx>,
    r: Value<'tcx>,
    ctx: &CompileCtx<'tcx>,
) -> Result<Value<'tcx>, InterpError> {
    match (l, r, op) {
        (Value::Int(l), Value::Int(r), Bop::Plus) => Ok(Value::Int(l.overflowing_add(r).0)),
        (Value::Int(l), Value::Int(r), Bop::Minus) => Ok(Value::Int(l.overflowing_sub(r).0)),
        (Value::Int(l), Value::Int(r), Bop::Mult) => Ok(Value::Int(l.overflowing_mul(r).0)),
        (Value::Int(l), Value::Int(r), Bop::Div) => {
            if r == 0 {
                return Err(InterpError::DivisionByZero);
            }
            Ok(Value::Int(l / r))
        }
        (Value::Int(l), Value::Int(r), Bop::Pow) => Ok(Value::Int(l.pow(r as u32))),
        (Value::Int(l), Value::Int(r), Bop::Comp(cop)) => match cop {
            CompOp::Eq => Ok(Value::Bool(l == r)),
            CompOp::Ne => Ok(Value::Bool(l != r)),
            CompOp::Lt => Ok(Value::Bool(l < r)),
            CompOp::Le => Ok(Value::Bool(l <= r)),
            CompOp::Gt => Ok(Value::Bool(l > r)),
            CompOp::Ge => Ok(Value::Bool(l >= r)),
        },
        (Value::Int(l), Value::Int(r), Bop::BitAnd) => Ok(Value::Int(l & r)),
        (Value::Int(l), Value::Int(r), Bop::Or) => Ok(Value::Int(l | r)),
        (Value::Int(l), Value::Int(r), Bop::Xor) => Ok(Value::Int(l ^ r)),
        (Value::Bool(l), Value::Bool(r), Bop::And) => Ok(Value::Bool(l && r)),
        (Value::Bool(l), Value::Bool(r), Bop::Or) => Ok(Value::Bool(l || r)),
        (Value::Bool(l), Value::Bool(r), Bop::Xor) => Ok(Value::Bool(l ^ r)),
        (Value::Bool(l), Value::Bool(r), Bop::Comp(CompOp::Eq)) => Ok(Value::Bool(l == r)),
        (Value::Bool(l), Value::Bool(r), Bop::Comp(CompOp::Ne)) => Ok(Value::Bool(l != r)),
        (
            Value::Constructor {
                enum_ref: er_l,
                variant_idx: vi_l,
                payload: ref pl_l,
            },
            Value::Constructor {
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
                CompOp::Eq => Ok(Value::Bool(eq)),
                CompOp::Ne => Ok(Value::Bool(!eq)),
                _ => unreachable!(),
            }
        }
        (
            Value::Constructor {
                enum_ref: er_l,
                variant_idx: vi_l,
                ..
            },
            Value::Constructor {
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
                CompOp::Ge => Ok(Value::Bool(vi_l >= vi_r)),
                CompOp::Le => Ok(Value::Bool(vi_l <= vi_r)),
                CompOp::Gt => Ok(Value::Bool(vi_l > vi_r)),
                CompOp::Lt => Ok(Value::Bool(vi_l < vi_r)),
                _ => unreachable!(),
            }
        }
        (Value::Tuple(l), Value::Tuple(r), Bop::Comp(CompOp::Eq)) => Ok(Value::Bool(l == r)),
        (Value::Tuple(l), Value::Tuple(r), Bop::Comp(CompOp::Ne)) => Ok(Value::Bool(l != r)),
        (el, er, o) => Err(InterpError::BinOpTypeError(format!(
            "{} {o} {}",
            fmt_value(&el, ctx),
            fmt_value(&er, ctx)
        ))),
    }
}

fn eval_unop<'tcx>(
    op: Uop,
    v: Value<'tcx>,
    ctx: &CompileCtx<'tcx>,
) -> Result<Value<'tcx>, InterpError> {
    match (v, op) {
        (Value::Bool(b), Uop::Not) => Ok(Value::Bool(!b)),
        (Value::Int(n), Uop::Not) => Ok(Value::Int(!n)),
        (Value::Int(n), Uop::Neg) => Ok(Value::Int(-n)),
        (e, o) => Err(InterpError::UnOpTypeError(o, fmt_value(&e, ctx))),
    }
}

fn eval_intrinsic<'tcx>(
    fn_name: Intrinsic,
    vals: Vec<Value<'tcx>>,
    ctx: &CompileCtx<'tcx>,
    output: &mut dyn std::io::Write,
) -> Result<Value<'tcx>, InterpError> {
    let fmt = |val: &Value<'tcx>| fmt_value(val, ctx);
    match fn_name {
        Intrinsic::Println => {
            let line: Vec<String> = vals.iter().map(fmt).collect();
            let _ = writeln!(output, "{}", line.join(" "));
            Ok(Value::Unit)
        }
        Intrinsic::Print => {
            let text: Vec<String> = vals.iter().map(fmt).collect();
            let _ = write!(output, "{}", text.join(" "));
            Ok(Value::Unit)
        }
        Intrinsic::Abs => {
            debug_assert_eq!(vals.len(), 1, "Abs expects 1 arg");
            match &vals[0] {
                Value::Int(n) => Ok(Value::Int(n.abs())),
                v => Err(InterpError::Runtime(format!(
                    "__abs: expected Int, got {}",
                    fmt(v)
                ))),
            }
        }
        Intrinsic::Min => {
            debug_assert_eq!(vals.len(), 2, "Min expects 2 args");
            match (&vals[0], &vals[1]) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.min(b))),
                _ => Err(InterpError::Runtime("__min: expected (Int, Int)".into())),
            }
        }
        Intrinsic::Max => {
            debug_assert_eq!(vals.len(), 2, "Max expects 2 args");
            match (&vals[0], &vals[1]) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.max(b))),
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
            Ok(Value::Int(n))
        }
        Intrinsic::Exit => {
            debug_assert_eq!(vals.len(), 1, "Exit expects 1 arg");
            match &vals[0] {
                Value::Int(code) => std::process::exit(*code as i32),
                v => Err(InterpError::Runtime(format!(
                    "__exit: expected Int, got {}",
                    fmt(v)
                ))),
            }
        }
        // Raw-pointer ops (Memory Step A). A `Ptr<T>` is a cell handle, like a
        // reference: `read`/`write` load/store the cell, `cast` is identity.
        Intrinsic::PtrRead => {
            debug_assert_eq!(vals.len(), 1, "__ptr_read expects 1 arg");
            match &vals[0] {
                Value::Ref(c) => Ok(c.borrow().clone()),
                v => Err(InterpError::Runtime(format!(
                    "__ptr_read: expected a pointer, got {}",
                    fmt(v)
                ))),
            }
        }
        Intrinsic::PtrWrite => {
            debug_assert_eq!(vals.len(), 2, "__ptr_write expects 2 args");
            match &vals[0] {
                Value::Ref(c) => {
                    *c.borrow_mut() = vals[1].clone();
                    Ok(Value::Unit)
                }
                v => Err(InterpError::Runtime(format!(
                    "__ptr_write: expected a pointer, got {}",
                    fmt(v)
                ))),
            }
        }
        Intrinsic::PtrCast => {
            debug_assert_eq!(vals.len(), 1, "__ptr_cast expects 1 arg");
            Ok(vals.into_iter().next().unwrap())
        }
        // No-op until types acquire destructors (Step C).
        Intrinsic::DropInPlace => Ok(Value::Unit),
    }
}

/// Execute a known external (FFI) function in the HIR interpreter (Memory Step
/// A). `malloc` allocates a fresh interpreter cell and returns a `Value::Ref`
/// handle to it; `free` is a no-op (the `Rc` drop reclaims it). The cell starts
/// at `Unit` (the HIR cell has no uninitialised state).
fn eval_extern<'tcx>(symbol: &str, _vals: Vec<Value<'tcx>>) -> Result<Value<'tcx>, InterpError> {
    match symbol {
        "malloc" | "calloc" => Ok(Value::Ref(cell(Value::Unit))),
        "free" => Ok(Value::Unit),
        other => Err(InterpError::Runtime(format!(
            "extern function '{other}' is not supported by the interpreter"
        ))),
    }
}

/// Render a [`Value`] for diagnostics. A reference prints as `&<pointee>`,
/// matching the `&`-erased surface and the MIR interpreter's display.
fn fmt_value<'tcx>(v: &Value<'tcx>, ctx: &CompileCtx<'tcx>) -> String {
    match v {
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Unit => "()".to_string(),
        Value::Constructor {
            enum_ref,
            variant_idx,
            payload,
        } => {
            let def = ctx.get_enum(*enum_ref);
            let name = &def.variants[*variant_idx].name;
            let tag = if def.is_anonymous {
                format!("#{name}")
            } else {
                name.clone()
            };
            match payload {
                Some(p) => format!("{tag}({})", fmt_value(p, ctx)),
                None => tag,
            }
        }
        Value::Tuple(elems) => {
            let inner = elems
                .iter()
                .map(|e| fmt_value(e, ctx))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({inner})")
        }
        Value::Ref(c) => format!("&{}", fmt_value(&c.borrow(), ctx)),
    }
}

/// Convert a runtime [`Value`] back into the `Expression` shape callers expect
/// from `interpret`. The reconstructed `Expr` wrappers carry placeholder
/// `ty`/`kind`/`range`; only `Expr::expr` participates in equality/display, so
/// the placeholders are invisible to comparison (tests) and rendering (LSP).
fn value_to_expr<'tcx>(v: Value<'tcx>, ctx: &CompileCtx<'tcx>) -> Expression<'tcx> {
    let wrap = |expr: Expression<'tcx>| Expr {
        expr,
        ty: ctx.types.unit,
        kind: Kind::Owned,
        range: Default::default(),
    };
    match v {
        Value::Int(i) => Expression::Int(i),
        Value::Bool(b) => Expression::Bool(b),
        Value::Unit => Expression::Unit,
        Value::Constructor {
            enum_ref,
            variant_idx,
            payload,
        } => Expression::Constructor {
            enum_ref,
            variant_idx,
            payload: payload.map(|p| Box::new(wrap(value_to_expr(*p, ctx)))),
        },
        Value::Tuple(elems) => Expression::Tuple(
            elems
                .into_iter()
                .map(|e| wrap(value_to_expr(e, ctx)))
                .collect(),
        ),
        // A program's top-level result is never a bare reference: the escape
        // check forbids returning references to locals, and `main : Int`.
        Value::Ref(_) => unreachable!("a program result cannot be a bare reference"),
    }
}
