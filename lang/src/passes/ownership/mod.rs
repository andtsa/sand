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

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::ModuleRef;
use crate::ir_types::typed_hir as th;
use crate::ir_types::typed_hir::TypedProgram;

pub fn check<'tcx>(
    ctx: &CompileCtx<'tcx>,
    program: TypedProgram<'tcx>,
) -> Result<TypedProgram<'tcx>, OwnershipCheckError<'tcx>> {
    for func in program.functions.values() {
        let checker = OwnershipChecker {
            ctx,
            module: func.src_module,
            type_constraints: func.type_constraints.clone(),
        };

        let mut env = OwnershipEnv::new();
        for param in &func.parameters {
            env.declare(param.name);
        }

        checker.check_expr(&func.body, &mut env)?;
    }
    Ok(program)
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
        stmt: &th::Statement<'tcx>,
        env: &mut OwnershipEnv<'tcx>,
    ) -> Result<(), OwnershipCheckError<'tcx>> {
        match stmt {
            th::Statement::Declaration { name, val, .. } => {
                // check RHS first (it may move other variables)
                self.check_expr(val, env)?;
                // the new variable starts as Owned
                env.declare(*name);
                Ok(())
            }

            th::Statement::Assignment { name, val, .. } => {
                self.check_expr(val, env)?;
                // the LHS variable's old value is implicitly dropped
                // for non-Copy types, re-declare as Owned
                if !self.is_copy(val.ty) {
                    env.declare(*name);
                }
                Ok(())
            }

            // `*r = e` write-through: the RHS is moved into the pointee; the
            // reference itself is read (not consumed) to write through, so a
            // plain reference variable is a non-consuming use (mirrors `Deref`).
            th::Statement::DerefAssign {
                reference, value, ..
            } => {
                self.check_expr(value, env)?;
                if !matches!(reference.expr, th::Expression::Var(_)) {
                    self.check_expr(reference, env)?;
                }
                Ok(())
            }

            th::Statement::LetTuple { elems, val, .. } => {
                self.check_expr(val, env)?;
                for (name, ..) in elems {
                    env.declare(*name);
                }
                Ok(())
            }

            th::Statement::LetPattern {
                pattern,
                val,
                else_branch,
                ..
            } => {
                self.check_expr(val, env)?;
                self.check_expr(else_branch, env)?;
                Self::declare_pattern_bindings(pattern, env);
                Ok(())
            }

            th::Statement::Expr(e) => self.check_expr(e, env),
        }
    }

    fn check_expr(
        &self,
        expr: &th::Expr<'tcx>,
        env: &mut OwnershipEnv<'tcx>,
    ) -> Result<(), OwnershipCheckError<'tcx>> {
        match &expr.expr {
            th::Expression::Int(_) | th::Expression::Bool(_) | th::Expression::Unit => Ok(()),

            // A borrow does not consume its referent: borrowing a variable is a
            // non-consuming read, so the variable stays usable (Calculus §6.2,
            // `Var-Borrow`). Borrowing a *variable* records an outstanding borrow
            // and enforces the exclusivity invariant (Step 9b): a `&mut x`
            // requires no other live borrow of `x`, and a `&x` may not coexist
            // with a live `&mut x`. Borrowing a temporary owns it exclusively, so
            // it just checks the sub-expression that produces it.
            th::Expression::Borrow(inner, mutable) => match &inner.expr {
                th::Expression::Var(v) => {
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
                    Ok(())
                }
                _ => self.check_expr(inner, env),
            },

            // `*e` reads through a reference. The reference itself is *not*
            // consumed (so `*r + *r` reads a `&mut Int` twice), hence a plain
            // reference variable is a non-consuming read; only a sub-expression
            // that *produces* the reference (e.g. `*&x`) is checked. Reading a
            // `Copy` value out duplicates it (fine); reading a non-`Copy` value
            // would move it out of the borrow, which is forbidden.
            th::Expression::Deref(inner) => {
                if !matches!(inner.expr, th::Expression::Var(_)) {
                    self.check_expr(inner, env)?;
                }
                if !self.is_copy(expr.ty) {
                    return Err(self.err(OwnershipError::MoveOutOfBorrow { range: expr.range }));
                }
                Ok(())
            }

            th::Expression::Constructor { payload, .. } => match payload {
                Some(p) => self.check_expr(p, env),
                None => Ok(()),
            },

            th::Expression::Tuple(elems) => self.check_exprs(elems, env),

            th::Expression::Var(v) => {
                if self.is_copy(expr.ty) {
                    return Ok(());
                }
                match env.get(v) {
                    Some(OwnershipState::Owned) => {
                        // A value may not be moved while a borrow of it is live
                        // (Calculus §6.2): once references are real pointers,
                        // `let r = &x; move(x); *r` is a use-after-free that no
                        // scope boundary catches. The lexical `borrows` map (Step
                        // 9b) already tracks live borrows per place.
                        if env.borrow_state(v).is_some() {
                            return Err(self.err(OwnershipError::MoveWhileBorrowed {
                                name: self.ctx.uniq_variable_name(v),
                                used_at: expr.range,
                            }));
                        }
                        env.mark_moved(*v, expr.range);
                        Ok(())
                    }
                    Some(OwnershipState::Moved { at }) => {
                        Err(self.err(OwnershipError::UseAfterMove {
                            name: self.ctx.uniq_variable_name(v),
                            moved_at: *at,
                            used_at: expr.range,
                            is_clone: self.ctx.is_clone(expr.ty),
                        }))
                    }
                    None => Ok(()),
                }
            }

            th::Expression::UnOp { right, .. } => self.check_expr(right, env),

            th::Expression::BinOp { left, right, .. } => {
                self.check_expr(left, env)?;
                self.check_expr(right, env)
            }

            th::Expression::Call { args, .. }
            | th::Expression::IntrinsicCall { args, .. }
            | th::Expression::MethodCall { args, .. } => self.check_exprs(args, env),

            th::Expression::If { cond, t, f } => {
                self.check_expr(cond, env)?;

                let mut then_env = env.clone();
                let mut else_env = env.clone();

                self.check_expr(t, &mut then_env)?;
                self.check_expr(f, &mut else_env)?;

                *env = OwnershipEnv::merge(&then_env, &else_env);
                Ok(())
            }

            // moves inside the body are forbidden
            th::Expression::While { cond, body } => {
                self.check_expr(cond, env)?;

                let mut body_env = env.clone();
                self.check_expr(body, &mut body_env)?;

                // any variable that was Owned before the loop but Moved in the
                // body cannot be guaranteed to be re-initialized on every
                // iteration
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
                // the loop may execute 0 times, so the outer env
                // is unchanged
                Ok(())
            }

            th::Expression::Block {
                statements,
                expr: ret,
            } => {
                // snapshot which variables existed before the block (to remove
                // block-local variables on exit) and which borrows were live (to
                // release borrows created inside the block on exit — a borrow's
                // lifetime is lexical, Step 9b).
                let pre_block_vars = env.var_keys();
                let pre_block_borrows = env.borrows_snapshot();

                for stmt in statements {
                    self.check_statement(stmt, env)?;
                }
                if let Some(e) = ret {
                    self.check_expr(e, env)?;
                }

                env.restrict_to(&pre_block_vars);
                env.restore_borrows(pre_block_borrows);
                Ok(())
            }

            th::Expression::Match { scrutinee, arms } => {
                // the scrutinee is consumed
                self.check_expr(scrutinee, env)?;

                // snapshot pre-arm variables so pattern bindings (which are
                // scoped to their arm) can be removed again afterwards —
                // mirrors `Block`'s `pre_block_vars`/`restrict_to` handling.
                let pre_arm_vars = env.var_keys();

                let arm_envs = arms
                    .iter()
                    .map(|arm| {
                        let mut arm_env = env.clone();
                        // per decision D3 (no partial-move tracking): the
                        // scrutinee is already fully consumed above, so each
                        // sub-value bound by the pattern starts life as a
                        // fresh, independently-`Owned` binding — no different
                        // from a `let` declaration. this requires zero changes
                        // to `OwnershipEnv`'s data model.
                        Self::declare_pattern_bindings(&arm.pattern, &mut arm_env);
                        self.check_expr(&arm.body, &mut arm_env)?;
                        arm_env.restrict_to(&pre_arm_vars);
                        Ok(arm_env)
                    })
                    .collect::<Result<Vec<_>, OwnershipCheckError<'tcx>>>()?;

                if let Some((first, rest)) = arm_envs.split_first() {
                    *env = rest
                        .iter()
                        .fold(first.clone(), |acc, e| OwnershipEnv::merge(&acc, e));
                }
                Ok(())
            }
        }
    }

    /// recursively declare every variable bound by `pattern` as freshly
    /// `Owned` in `env` (decision D3 — see `Match` arm handling above).
    fn declare_pattern_bindings(pattern: &th::MatchPattern<'tcx>, env: &mut OwnershipEnv<'tcx>) {
        match pattern {
            th::MatchPattern::Wildcard
            | th::MatchPattern::IntLit(_)
            | th::MatchPattern::BoolLit(_) => {}
            th::MatchPattern::Binding { var, .. } => env.declare(*var),
            th::MatchPattern::Tuple { elems, .. } => {
                for sub in elems {
                    Self::declare_pattern_bindings(sub, env);
                }
            }
            th::MatchPattern::Variant { payload, .. } => {
                if let Some((_, sub)) = payload {
                    Self::declare_pattern_bindings(sub, env);
                }
            }
        }
    }

    fn check_exprs(
        &self,
        exprs: &[th::Expr<'tcx>],
        env: &mut OwnershipEnv<'tcx>,
    ) -> Result<(), OwnershipCheckError<'tcx>> {
        for e in exprs {
            self.check_expr(e, env)?;
        }
        Ok(())
    }
}
