//! affine-type ownership checker.
//!
//! this pass runs on `TypedHIR` after type checking and before MIR lowering.
//! it enforces "each owned value may be used at most once":
//!
//! # design
//!
//! the checker passes two pieces of state simultaneously through the
//! expression tree:
//!
//! * `Result<(), OwnershipCheckError>`, mirroring `Either`
//! * `&mut OwnershipEnv`, mirroring `StateT`
//!
//! this is the Rust equivalent of `StateT OwnershipEnv (Either
//! OwnershipCheckError)`.
//!
//! at every branch point (if/match) the env is **cloned** to give each branch
//! its own snapshot, then the per-branch results are *merged* conservatively:
//! a variable is Owned after the join only if *all* branches left it Owned.

pub mod env;
pub mod errors;

use env::BorrowState;
use env::OwnershipEnv;
use env::OwnershipState;
use errors::OwnershipCheckError;
use errors::OwnershipError;
use im::HashSet;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::*;

pub fn check<'tcx>(
    ctx: &CompileCtx<'tcx>,
    mut program: TypedProgram<'tcx>,
) -> Result<TypedProgram<'tcx>, OwnershipCheckError<'tcx>> {
    for func in program.functions.values_mut() {
        let checker = OwnershipChecker {
            ctx,
            module: func.src_module,
            type_constraints: func.type_constraints.clone(),
        };

        let mut env = OwnershipEnv::new();
        for param in &func.parameters {
            env.declare(param.name, param.ty);
        }

        // Rebuild the body with scope-exit drops elaborated (Step B). Then drop
        // any owned, non-`Copy` parameter at function exit — params are the only
        // bindings live before the body, so an empty "pre" set selects them.
        let new_body = checker.check_expr(&func.body, &mut env)?;
        let param_drops = checker.scope_exit_drops(&env, &HashSet::new());
        func.body = attach_drops(new_body, param_drops);
    }
    Ok(program)
}

/// Attach scope-exit `drops` that must run *after* `expr`'s value is computed.
/// If `expr` is already a block, the drops are appended to its drop list
/// (existing block-local drops have higher `idx` and so already precede these,
/// preserving reverse-declaration order); otherwise `expr` is wrapped in a
/// block whose value is `expr`.
fn attach_drops<'tcx>(mut expr: Expr<'tcx>, drops: Vec<UniqVar<'tcx>>) -> Expr<'tcx> {
    if drops.is_empty() {
        return expr;
    }
    if let Expression::Block {
        drops: block_drops, ..
    } = &mut expr.expr
    {
        block_drops.extend(drops);
        return expr;
    }
    let (range, ty, kind) = (expr.range, expr.ty, expr.kind);
    Expr {
        expr: Expression::Block {
            statements: Vec::new(),
            expr: Some(Box::new(expr)),
            drops,
        },
        range,
        ty,
        kind,
    }
}

struct OwnershipChecker<'a, 'tcx> {
    ctx: &'a CompileCtx<'tcx>,
    module: ModuleRef<'tcx>,
    /// the current function's `where T : C` constraints — a `where T : Copy`
    /// makes a parameter of type `T` implicitly copyable (Step 14c).
    type_constraints: Vec<crate::compiler::structure::TypeConstraint>,
}

impl<'tcx> OwnershipChecker<'_, 'tcx> {
    /// Is `ty` implicitly copied here, accounting for `where T : Copy` bounds?
    fn is_copy(&self, ty: crate::lang::types::Ty<'tcx>) -> bool {
        self.ctx.is_copy_under(ty, &self.type_constraints)
    }
}

impl<'tcx> OwnershipChecker<'_, 'tcx> {
    fn err(&self, error: OwnershipError) -> OwnershipCheckError<'tcx> {
        OwnershipCheckError {
            error,
            module: self.module,
        }
    }

    fn check_statement(
        &self,
        stmt: &Statement<'tcx>,
        env: &mut OwnershipEnv<'tcx>,
    ) -> Result<Statement<'tcx>, OwnershipCheckError<'tcx>> {
        match stmt {
            Statement::Declaration {
                name,
                range,
                ty,
                val,
            } => {
                // check RHS first (it may move other variables)
                let val = self.check_expr(val, env)?;
                // the new variable starts as Owned
                env.declare(*name, *ty);
                Ok(Statement::Declaration {
                    name: *name,
                    range: *range,
                    ty: *ty,
                    val,
                })
            }

            Statement::Assignment { name, range, val } => {
                let val = self.check_expr(val, env)?;
                // the LHS variable's old value is implicitly dropped; for
                // non-Copy types, re-declare as Owned.
                if !self.is_copy(val.ty) {
                    env.declare(*name, val.ty);
                }
                Ok(Statement::Assignment {
                    name: *name,
                    range: *range,
                    val,
                })
            }

            // `*r = e` write-through: the RHS is moved into the pointee; the
            // reference itself is read (not consumed) to write through, so a
            // plain reference variable is a non-consuming use (mirrors `Deref`).
            Statement::DerefAssign {
                reference,
                value,
                range,
            } => {
                let value = self.check_expr(value, env)?;
                let reference = if matches!(reference.expr, Expression::Var(_)) {
                    reference.clone()
                } else {
                    self.check_expr(reference, env)?
                };
                Ok(Statement::DerefAssign {
                    reference,
                    value,
                    range: *range,
                })
            }

            Statement::LetTuple { elems, range, val } => {
                let val = self.check_expr(val, env)?;
                for (name, ty, ..) in elems {
                    env.declare(*name, *ty);
                }
                Ok(Statement::LetTuple {
                    elems: elems.clone(),
                    range: *range,
                    val,
                })
            }

            Statement::LetPattern {
                pattern,
                val,
                else_branch,
                range,
            } => {
                let val = self.check_expr(val, env)?;
                let else_branch = self.check_expr(else_branch, env)?;
                Self::declare_pattern_bindings(pattern, env);
                Ok(Statement::LetPattern {
                    pattern: pattern.clone(),
                    val,
                    else_branch,
                    range: *range,
                })
            }

            Statement::Expr(e) => Ok(Statement::Expr(self.check_expr(e, env)?)),
        }
    }

    fn check_expr(
        &self,
        expr: &Expr<'tcx>,
        env: &mut OwnershipEnv<'tcx>,
    ) -> Result<Expr<'tcx>, OwnershipCheckError<'tcx>> {
        let (range, ty, kind) = (expr.range, expr.ty, expr.kind);
        let rebuild = |expression| Expr {
            expr: expression,
            range,
            ty,
            kind,
        };
        let new_expr = match &expr.expr {
            Expression::Int(_) | Expression::Bool(_) | Expression::Unit => expr.expr.clone(),

            // A borrow does not consume its referent: borrowing a variable is a
            // non-consuming read, so the variable stays usable (Calculus §6.2,
            // `Var-Borrow`). Borrowing a *variable* records an outstanding borrow
            // and enforces the exclusivity invariant (Step 9b): a `&mut x`
            // requires no other live borrow of `x`, and a `&x` may not coexist
            // with a live `&mut x`. Borrowing a temporary owns it exclusively, so
            // it just checks the sub-expression that produces it.
            Expression::Borrow(inner, mutable) => match &inner.expr {
                Expression::Var(v) => {
                    if let Some(existing) = env.borrow_state(v) {
                        let conflict = *mutable || existing == BorrowState::Mut;
                        if conflict {
                            return Err(self.err(OwnershipError::ConflictingBorrow {
                                name: self.ctx.uniq_variable_name(v),
                                mutable: *mutable,
                                existing_mutable: existing == BorrowState::Mut,
                                range: expr.range,
                            }));
                        }
                    }
                    env.add_borrow(*v, *mutable);
                    expr.expr.clone()
                }
                _ => Expression::Borrow(Box::new(self.check_expr(inner, env)?), *mutable),
            },

            // `*e` reads through a reference. The reference itself is *not*
            // consumed (so `*r + *r` reads a `&mut Int` twice), hence a plain
            // reference variable is a non-consuming read; only a sub-expression
            // that *produces* the reference (e.g. `*&x`) is checked. Reading a
            // `Copy` value out duplicates it (fine); reading a non-`Copy` value
            // would move it out of the borrow, which is forbidden.
            Expression::Deref(inner) => {
                if !self.is_copy(expr.ty) {
                    return Err(self.err(OwnershipError::MoveOutOfBorrow { range: expr.range }));
                }
                if matches!(inner.expr, Expression::Var(_)) {
                    expr.expr.clone()
                } else {
                    Expression::Deref(Box::new(self.check_expr(inner, env)?))
                }
            }

            Expression::Constructor {
                enum_ref,
                variant_idx,
                payload,
            } => Expression::Constructor {
                enum_ref: *enum_ref,
                variant_idx: *variant_idx,
                payload: match payload {
                    Some(p) => Some(Box::new(self.check_expr(p, env)?)),
                    None => None,
                },
            },

            Expression::Tuple(elems) => Expression::Tuple(self.check_exprs(elems, env)?),

            Expression::Var(v) => {
                if !self.is_copy(expr.ty) {
                    match env.get(v) {
                        Some(OwnershipState::Owned) => {
                            // A value may not be moved while a borrow of it is
                            // live (Calculus §6.2): once references are real
                            // pointers, `let r = &x; move(x); *r` is a
                            // use-after-free no scope boundary catches.
                            if env.borrow_state(v).is_some() {
                                return Err(self.err(OwnershipError::MoveWhileBorrowed {
                                    name: self.ctx.uniq_variable_name(v),
                                    used_at: expr.range,
                                }));
                            }
                            env.mark_moved(*v, expr.range);
                        }
                        Some(OwnershipState::Moved { at }) => {
                            return Err(self.err(OwnershipError::UseAfterMove {
                                name: self.ctx.uniq_variable_name(v),
                                moved_at: *at,
                                used_at: expr.range,
                                is_clone: self.ctx.is_clone(expr.ty),
                            }));
                        }
                        None => {}
                    }
                }
                expr.expr.clone()
            }

            Expression::UnOp { op, right } => Expression::UnOp {
                op: *op,
                right: Box::new(self.check_expr(right, env)?),
            },

            Expression::BinOp { left, op, right } => {
                let left = Box::new(self.check_expr(left, env)?);
                let right = Box::new(self.check_expr(right, env)?);
                Expression::BinOp {
                    left,
                    op: *op,
                    right,
                }
            }

            Expression::Call { fn_name, args } => Expression::Call {
                fn_name: *fn_name,
                args: self.check_exprs(args, env)?,
            },
            Expression::IntrinsicCall { fn_name, args } => Expression::IntrinsicCall {
                fn_name: *fn_name,
                args: self.check_exprs(args, env)?,
            },
            Expression::MethodCall {
                class,
                method,
                self_ty,
                args,
            } => Expression::MethodCall {
                class: *class,
                method: method.clone(),
                self_ty: *self_ty,
                args: self.check_exprs(args, env)?,
            },

            Expression::If { cond, t, f } => {
                let cond = Box::new(self.check_expr(cond, env)?);

                let mut then_env = env.clone();
                let mut else_env = env.clone();

                let t = self.check_expr(t, &mut then_env)?;
                let f = self.check_expr(f, &mut else_env)?;

                // Completing drops (Step B, Calculus §6.11): a value owned on one
                // branch but moved on the other is dropped on the owning branch,
                // so it is uniformly consumed at the merge.
                let (merged, drop_in_then, drop_in_else) =
                    OwnershipEnv::merge_with_drops(&then_env, &else_env);
                *env = merged;
                let t = attach_drops(t, drop_in_then);
                let f = attach_drops(f, drop_in_else);

                Expression::If {
                    cond,
                    t: Box::new(t),
                    f: Box::new(f),
                }
            }

            // moves inside the body are forbidden
            Expression::While { cond, body } => {
                let cond = Box::new(self.check_expr(cond, env)?);

                let mut body_env = env.clone();
                let body = self.check_expr(body, &mut body_env)?;

                // any variable Owned before the loop but Moved in the body
                // cannot be guaranteed re-initialised on every iteration.
                for (var, state) in body_env.iter() {
                    if let OwnershipState::Moved { at } = state
                        && matches!(env.get(var), Some(OwnershipState::Owned))
                    {
                        return Err(self.err(OwnershipError::MoveInLoop {
                            name: self.ctx.uniq_variable_name(var),
                            moved_at: *at,
                            loop_range: expr.range,
                        }));
                    }
                }
                // the loop may execute 0 times, so the outer env is unchanged.
                Expression::While {
                    cond,
                    body: Box::new(body),
                }
            }

            Expression::Block {
                statements,
                expr: ret,
                ..
            } => {
                // snapshot which variables existed before the block (to remove
                // block-local variables on exit) and which borrows were live (to
                // release borrows created inside the block on exit — a borrow's
                // lifetime is lexical, Step 9b).
                let pre_block_vars = env.var_keys();
                let pre_block_borrows = env.borrows_snapshot();

                let statements = statements
                    .iter()
                    .map(|s| self.check_statement(s, env))
                    .collect::<Result<Vec<_>, _>>()?;
                let ret = match ret {
                    Some(e) => Some(Box::new(self.check_expr(e, env)?)),
                    None => None,
                };

                // scope-exit drops: block-local bindings still owned (non-`Copy`),
                // in reverse declaration order, run after the block's value.
                let drops = self.scope_exit_drops(env, &pre_block_vars);

                env.restrict_to(&pre_block_vars);
                env.restore_borrows(pre_block_borrows);
                Expression::Block {
                    statements,
                    expr: ret,
                    drops,
                }
            }

            Expression::Match { scrutinee, arms } => {
                // the scrutinee is consumed
                let scrutinee = Box::new(self.check_expr(scrutinee, env)?);

                // snapshot pre-arm variables so pattern bindings (scoped to their
                // arm) can be removed again — mirrors `Block`.
                let pre_arm_vars = env.var_keys();

                let checked: Vec<(TypedMatchArm<'tcx>, OwnershipEnv<'tcx>)> = arms
                    .iter()
                    .map(|arm| {
                        let mut arm_env = env.clone();
                        // per decision D3: the scrutinee is fully consumed above,
                        // so each pattern-bound sub-value starts as a fresh,
                        // independently-`Owned` binding (like a `let`).
                        Self::declare_pattern_bindings(&arm.pattern, &mut arm_env);
                        let body = self.check_expr(&arm.body, &mut arm_env)?;
                        arm_env.restrict_to(&pre_arm_vars);
                        Ok((
                            TypedMatchArm {
                                pattern: arm.pattern.clone(),
                                body,
                                range: arm.range,
                            },
                            arm_env,
                        ))
                    })
                    .collect::<Result<Vec<_>, OwnershipCheckError<'tcx>>>()?;

                // Fold the arm envs into the merged result, collecting each arm's
                // completing drops: at each step, vars the running merge already
                // moved that this arm still owns drop on this arm; vars this arm
                // moves that earlier arms still own drop on those earlier arms.
                let mut arm_drops: Vec<Vec<UniqVar<'tcx>>> = vec![Vec::new(); checked.len()];
                if let Some((_, first_env)) = checked.first() {
                    let mut acc = first_env.clone();
                    for i in 1..checked.len() {
                        let (merged, drop_acc, drop_arm) =
                            OwnershipEnv::merge_with_drops(&acc, &checked[i].1);
                        for slot in arm_drops.iter_mut().take(i) {
                            slot.extend(drop_acc.iter().copied());
                        }
                        arm_drops[i].extend(drop_arm);
                        acc = merged;
                    }
                    *env = acc;
                }

                let arms_out: Vec<TypedMatchArm<'tcx>> = checked
                    .into_iter()
                    .zip(arm_drops)
                    .map(|((arm, _), mut drops)| {
                        // reverse-declaration order (descending `idx`).
                        drops.sort_by(|a, b| b.cmp(a));
                        let range = arm.range;
                        TypedMatchArm {
                            pattern: arm.pattern,
                            body: attach_drops(arm.body, drops),
                            range,
                        }
                    })
                    .collect();

                Expression::Match {
                    scrutinee,
                    arms: arms_out,
                }
            }
        };
        Ok(rebuild(new_expr))
    }

    /// recursively declare every variable bound by `pattern` as freshly
    /// `Owned` in `env` (decision D3 — see `Match` arm handling above).
    fn declare_pattern_bindings(pattern: &MatchPattern<'tcx>, env: &mut OwnershipEnv<'tcx>) {
        match pattern {
            MatchPattern::Wildcard | MatchPattern::IntLit(_) | MatchPattern::BoolLit(_) => {}
            MatchPattern::Binding { var, ty, .. } => env.declare(*var, *ty),
            MatchPattern::Tuple { elems, .. } => {
                for sub in elems {
                    Self::declare_pattern_bindings(sub, env);
                }
            }
            MatchPattern::Variant { payload, .. } => {
                if let Some((_, sub)) = payload {
                    Self::declare_pattern_bindings(sub, env);
                }
            }
        }
    }

    fn check_exprs(
        &self,
        exprs: &[Expr<'tcx>],
        env: &mut OwnershipEnv<'tcx>,
    ) -> Result<Vec<Expr<'tcx>>, OwnershipCheckError<'tcx>> {
        exprs.iter().map(|e| self.check_expr(e, env)).collect()
    }

    /// The bindings to drop at this scope's exit: those declared *since* `pre`
    /// (the block-locals) that are still `Owned` and not `Copy`, in reverse
    /// declaration order (the env iterates ascending `UniqVar` `idx`).
    fn scope_exit_drops(
        &self,
        env: &OwnershipEnv<'tcx>,
        pre: &HashSet<UniqVar<'tcx>>,
    ) -> Vec<UniqVar<'tcx>> {
        let mut drops: Vec<UniqVar<'tcx>> = env
            .iter()
            .filter(|&(var, st)| {
                matches!(st, OwnershipState::Owned)
                    && !pre.contains(var)
                    && env.var_ty(var).is_some_and(|ty| !self.is_copy(ty))
            })
            .map(|(var, _)| *var)
            .collect();
        drops.reverse();
        drops
    }
}
