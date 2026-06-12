//! a function's context for explicate control

use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::internal_bug;
use crate::ir_types::mir::*;
use crate::ir_types::typed_hir as th;
use crate::lang::ops::Bop;
use crate::lang::ops::CompOp;
use crate::lang::ops::Uop;
use crate::lang::types::CommonTypes;
use crate::lang::types::Kind;
use crate::lang::types::Ty;

pub(super) struct FnCx<'tcx> {
    #[allow(dead_code)]
    name: FunRef<'tcx>,
    #[allow(dead_code)]
    range: Range,
    #[allow(dead_code)]
    ret_type: Ty<'tcx>,

    pub(super) locals: Vec<LocalDecl<'tcx>>,
    local_map: Map<UniqVar<'tcx>, LocalId>,

    pub(super) blocks: Vec<BasicBlock<'tcx>>,
    next_temp: usize,

    types: CommonTypes<'tcx>,
}

impl<'tcx> FnCx<'tcx> {
    pub(super) fn new(
        name: FunRef<'tcx>,
        range: Range,
        ret_type: Ty<'tcx>,
        types: CommonTypes<'tcx>,
    ) -> Self {
        Self {
            name,
            range,
            ret_type,
            locals: Vec::new(),
            local_map: Map::new(),
            blocks: Vec::new(),
            next_temp: 0,
            types,
        }
    }

    pub(super) fn new_block(
        &mut self,
        statements: Vec<Statement<'tcx>>,
        terminator: Terminator,
    ) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock {
            id,
            statements,
            terminator,
        });
        id
    }

    pub(super) fn reserve_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock {
            id,
            statements: Vec::new(),
            terminator: Terminator::Unreachable,
        });
        id
    }

    pub(super) fn set_block(
        &mut self,
        id: BlockId,
        statements: Vec<Statement<'tcx>>,
        terminator: Terminator,
    ) {
        self.blocks[id.0] = BasicBlock {
            id,
            statements,
            terminator,
        };
    }

    pub(super) fn get_or_create_local(
        &mut self,
        name: UniqVar<'tcx>,
        ty: Ty<'tcx>,
        range: Range,
    ) -> LocalId {
        if let Some(id) = self.local_map.get(&name) {
            return *id;
        }

        let id = LocalId(self.locals.len());
        self.locals.push(LocalDecl {
            id,
            name: LocalName::User(name),
            ty,
            range,
        });
        self.local_map.insert(name, id);
        id
    }

    pub(super) fn fresh_temp(&mut self, hint: &'static str, ty: Ty<'tcx>, range: Range) -> LocalId {
        let id = LocalId(self.locals.len());
        let name = LocalName::Temp(self.next_temp, hint);
        self.next_temp += 1;

        self.locals.push(LocalDecl {
            id,
            name: name.clone(),
            ty,
            range,
        });
        id
    }

    pub(super) fn place(local: LocalId) -> Place {
        Place { local }
    }

    pub(super) fn const_operand(expr: &th::Expr<'_>) -> Option<Operand> {
        match &expr.expr {
            th::Expression::Int(i) => Some(Operand::Const(Constant::Int(*i))),
            th::Expression::Bool(b) => Some(Operand::Const(Constant::Bool(*b))),
            th::Expression::Unit => Some(Operand::Const(Constant::Unit)),
            // constructors — including nullary ones — are no longer constants:
            // they are `Aggregate([Const::Int(variant_idx), ...])` in MIR.
            _ => None,
        }
    }

    pub(super) fn var_operand(&self, name: &UniqVar<'tcx>) -> Operand {
        let local = *self
            .local_map
            .get(name)
            .unwrap_or_else(|| internal_bug!("missing local for variable {name:?}"));
        Operand::Copy(Self::place(local))
    }

    pub(super) fn simple_operand(&self, expr: &th::Expr<'tcx>) -> Option<Operand> {
        if let Some(c) = Self::const_operand(expr) {
            return Some(c);
        }

        match &expr.expr {
            th::Expression::Var(v) => Some(self.var_operand(v)),
            _ => None,
        }
    }

    pub(super) fn assign_stmt(
        &self,
        dst: LocalId,
        value: RValue<'tcx>,
        range: Range,
    ) -> Statement<'tcx> {
        Statement::Assign {
            dst: Self::place(dst),
            value,
            range,
        }
    }

    pub(super) fn goto_block(&mut self, target: BlockId) -> BlockId {
        self.new_block(Vec::new(), Terminator::Goto { target })
    }

    pub(super) fn unit_assign_then_goto(
        &mut self,
        dst: LocalId,
        range: Range,
        target: BlockId,
    ) -> BlockId {
        self.new_block(
            vec![self.assign_stmt(dst, RValue::Use(Operand::Const(Constant::Unit)), range)],
            Terminator::Goto { target },
        )
    }

    /// recursively emit statements that bind the variables of `pattern`,
    /// given that the value to match against is available as `source` (an
    /// already-materialized `Operand`, e.g. `Copy(scrut_place)`).
    ///
    /// `Wildcard`s and direct `Binding`s need no intermediate storage —
    /// they either discard the value (no statement emitted; sound because
    /// decision D3 performs no partial-move tracking, so a discarded
    /// sub-value simply remains owned by whatever already holds `source`)
    /// or are assigned straight from `source`. Compound sub-patterns
    /// (`Tuple`, or a `Variant`'s payload) delegate to
    /// [`Self::lower_projected_pattern`], which materializes the projected
    /// value into a fresh temp before recursing — see its doc comment for
    /// why that's necessary.
    pub(super) fn lower_pattern_bindings(
        &mut self,
        pattern: &th::MatchPattern<'tcx>,
        source: Operand,
        range: Range,
        statements: &mut Vec<Statement<'tcx>>,
    ) {
        match pattern {
            th::MatchPattern::Wildcard
            | th::MatchPattern::IntLit(_)
            | th::MatchPattern::BoolLit(_) => {
                // no bindings — the check was already done in the dispatch
                // chain
            }
            th::MatchPattern::Binding {
                var,
                ty,
                range: brange,
            } => {
                let local = self.get_or_create_local(*var, *ty, *brange);
                statements.push(self.assign_stmt(local, RValue::Use(source), *brange));
            }
            th::MatchPattern::Tuple { elems, .. } => {
                for (i, sub) in elems.iter().enumerate() {
                    self.lower_projected_pattern(sub, source.clone(), i, range, statements);
                }
            }
            th::MatchPattern::Variant { payload, .. } => {
                if let Some((_, sub)) = payload {
                    // payload is always field 1 (field 0 is the discriminant)
                    self.lower_projected_pattern(sub, source, 1, range, statements);
                }
            }
        }
    }

    /// bind `pattern` against the result of projecting `projection` out of
    /// `base`. simple patterns (`Binding`, `Wildcard`) consume the
    /// `RValue::Field` directly — `Binding` is assigned straight from it,
    /// `Wildcard` discards it (no statement). compound patterns (`Tuple`,
    /// `Variant`) must first materialize the projection into a fresh temp,
    /// because `RValue::Field` isn't an `Operand` — further projection needs
    /// `Copy(Place)`/`Const`, so the intermediate value needs a `Place` to be
    /// copied from.
    ///
    /// For `Variant` sub-patterns, `field = Field(base, index)` is the nested
    /// enum value; we materialize it into a temp and recurse into its payload
    /// (`field 1`) to extract any deeper bindings. The discriminant check for
    /// the nested variant was already emitted in the dispatch chain by
    /// `build_arm_check_chain`; here we only extract the bindings (assuming
    /// the checks passed).
    pub(super) fn lower_projected_pattern(
        &mut self,
        pattern: &th::MatchPattern<'tcx>,
        base: Operand,
        index: usize,
        range: Range,
        statements: &mut Vec<Statement<'tcx>>,
    ) {
        let field = RValue::Field { base, index };
        match pattern {
            th::MatchPattern::Wildcard => {
                // discard — D3, no partial-move tracking, no statement needed
            }
            th::MatchPattern::Binding {
                var,
                ty,
                range: brange,
            } => {
                let local = self.get_or_create_local(*var, *ty, *brange);
                statements.push(self.assign_stmt(local, field, *brange));
            }
            th::MatchPattern::Tuple { ty, elems } => {
                let tmp = self.fresh_temp("pattern_extract", *ty, range);
                statements.push(self.assign_stmt(tmp, field, range));
                let base = Operand::Copy(Self::place(tmp));
                for (i, sub) in elems.iter().enumerate() {
                    self.lower_projected_pattern(sub, base.clone(), i, range, statements);
                }
            }
            th::MatchPattern::Variant {
                ty,
                payload: Some((_, inner_sub)),
                ..
            } => {
                // `field` IS the nested enum value (of type `ty`). Materialise it
                // into a correctly-typed temp, then recurse into field 1 (the payload)
                // to extract deeper bindings.  We use `ty` (the enum type, e.g.
                // `Result`) rather than `payload_ty` (the payload type, e.g. `Int`)
                // because `field` holds the whole inner enum, not just its payload.
                let tmp = self.fresh_temp("nested_variant_extract", *ty, range);
                statements.push(self.assign_stmt(tmp, field, range));
                let base = Operand::Copy(Self::place(tmp));
                self.lower_projected_pattern(inner_sub, base, 1, range, statements);
            }
            th::MatchPattern::Variant { payload: None, .. } => {
                // nullary inner variant — no payload, so nothing to bind.
                // The discriminant check was done in the dispatch chain.
            }
            th::MatchPattern::IntLit(_) | th::MatchPattern::BoolLit(_) => internal_bug!(
                "nested IntLit/BoolLit pattern reached MIR lowering \
                 (RefutableNestedPattern should have rejected this at typecheck)"
            ),
        }
    }

    /// Returns true if `pattern` requires a runtime check (i.e. is refutable).
    /// Wildcard, Binding, and Tuple are irrefutable; Variant, IntLit, BoolLit
    /// are refutable.
    fn pattern_is_refutable(pattern: &th::MatchPattern) -> bool {
        matches!(
            pattern,
            th::MatchPattern::Variant { .. }
                | th::MatchPattern::IntLit(_)
                | th::MatchPattern::BoolLit(_)
        )
    }

    /// Build the check blocks for a single arm's pattern, emitting as many
    /// basic blocks as there are nested refutable layers.
    ///
    /// - `value_tmp`: the `LocalId` holding the value to match against
    ///   `pattern`.
    /// - `arm_bb`: destination when *all* checks in the chain pass.
    /// - `fallthrough_bb`: destination when *any* check in the chain fails.
    ///
    /// Returns the entry block of the check chain (the outermost check for
    /// refutable patterns, or `arm_bb` directly for irrefutable ones).
    ///
    /// For `Variant { payload: Some((payload_ty, inner_pat)) }` where
    /// `inner_pat` is also refutable, the method emits:
    ///
    /// ```text
    /// outer_check_bb:
    ///   disc_tmp   = Field(value_tmp, 0)
    ///   cmp_tmp    = (disc_tmp == variant_idx)
    ///   Branch(cmp_tmp → extract_bb, else → fallthrough_bb)
    ///
    /// extract_bb:
    ///   payload_tmp = Field(value_tmp, 1)
    ///   Goto inner_check_entry
    ///
    /// inner_check_entry …  (recurse)
    /// ```
    fn build_arm_check_chain(
        &mut self,
        pattern: &th::MatchPattern<'tcx>,
        value_tmp: LocalId,
        arm_bb: BlockId,
        fallthrough_bb: BlockId,
        range: Range,
    ) -> BlockId {
        match pattern {
            // Irrefutable patterns need no check — jump straight to the arm.
            th::MatchPattern::Wildcard
            | th::MatchPattern::Binding { .. }
            | th::MatchPattern::Tuple { .. } => arm_bb,

            th::MatchPattern::IntLit(n) => {
                let cmp_tmp = self.fresh_temp("match_int_cmp", self.types.bool, range);
                self.new_block(
                    vec![self.assign_stmt(
                        cmp_tmp,
                        RValue::BinaryOp {
                            op: Bop::Comp(CompOp::Eq),
                            left: Operand::Copy(Self::place(value_tmp)),
                            right: Operand::Const(Constant::Int(*n)),
                        },
                        range,
                    )],
                    Terminator::Branch {
                        cond: Operand::Copy(Self::place(cmp_tmp)),
                        then_bb: arm_bb,
                        else_bb: fallthrough_bb,
                    },
                )
            }

            th::MatchPattern::BoolLit(b) => {
                let cmp_tmp = self.fresh_temp("match_bool_cmp", self.types.bool, range);
                self.new_block(
                    vec![self.assign_stmt(
                        cmp_tmp,
                        RValue::BinaryOp {
                            op: Bop::Comp(CompOp::Eq),
                            left: Operand::Copy(Self::place(value_tmp)),
                            right: Operand::Const(Constant::Bool(*b)),
                        },
                        range,
                    )],
                    Terminator::Branch {
                        cond: Operand::Copy(Self::place(cmp_tmp)),
                        then_bb: arm_bb,
                        else_bb: fallthrough_bb,
                    },
                )
            }

            th::MatchPattern::Variant {
                variant_idx,
                payload,
                ..
            } => {
                // Determine where to go when the outer disc check passes.
                // If the payload itself contains a refutable sub-pattern, we
                // need to extract the payload into a temp and continue checking
                // it before reaching arm_bb.
                let inner_target = match payload {
                    Some((payload_ty, inner_pat)) if Self::pattern_is_refutable(inner_pat) => {
                        // Allocate a local for the inner enum (the payload).
                        let payload_tmp = self.fresh_temp("check_payload", *payload_ty, range);
                        // Recursively build the check chain for the inner pattern.
                        let inner_check_entry = self.build_arm_check_chain(
                            inner_pat,
                            payload_tmp,
                            arm_bb,
                            fallthrough_bb,
                            range,
                        );
                        // Build an extraction block: payload_tmp = Field(value_tmp, 1);
                        // then jump into the inner check chain.
                        self.new_block(
                            vec![self.assign_stmt(
                                payload_tmp,
                                RValue::Field {
                                    base: Operand::Copy(Self::place(value_tmp)),
                                    index: 1,
                                },
                                range,
                            )],
                            Terminator::Goto {
                                target: inner_check_entry,
                            },
                        )
                    }
                    _ => arm_bb,
                };

                // Build the outer discriminant check.
                let disc_tmp = self.fresh_temp("match_disc", self.types.int, range);
                let cmp_tmp = self.fresh_temp("match_cmp", self.types.bool, range);
                self.new_block(
                    vec![
                        self.assign_stmt(
                            disc_tmp,
                            RValue::Field {
                                base: Operand::Copy(Self::place(value_tmp)),
                                index: 0,
                            },
                            range,
                        ),
                        self.assign_stmt(
                            cmp_tmp,
                            RValue::BinaryOp {
                                op: Bop::Comp(CompOp::Eq),
                                left: Operand::Copy(Self::place(disc_tmp)),
                                right: Operand::Const(Constant::Int(*variant_idx as i64)),
                            },
                            range,
                        ),
                    ],
                    Terminator::Branch {
                        cond: Operand::Copy(Self::place(cmp_tmp)),
                        then_bb: inner_target,
                        else_bb: fallthrough_bb,
                    },
                )
            }
        }
    }

    pub(super) fn lower_tail(&mut self, expr: &th::Expr<'tcx>) -> BlockId {
        // A diverging expression never returns: lower it for its effects and
        // terminate the path as unreachable (no value is produced).
        if expr.kind == Kind::Never {
            let unreachable = self.new_block(Vec::new(), Terminator::Unreachable);
            return self.lower_effect(expr, unreachable);
        }
        match &expr.expr {
            th::Expression::If { cond, t, f } => {
                let then_bb = self.lower_tail(t);
                let else_bb = self.lower_tail(f);
                self.lower_pred(cond, then_bb, else_bb)
            }

            th::Expression::Block { statements, expr } => {
                let cont = if let Some(e) = expr {
                    self.lower_tail(e)
                } else {
                    self.new_block(Vec::new(), Terminator::Return { value: None })
                };
                self.lower_statements(statements, cont)
            }

            _ if expr.ty == self.types.unit => {
                let ret = self.new_block(Vec::new(), Terminator::Return { value: None });
                self.lower_effect(expr, ret)
            }

            _ => {
                let tmp = self.fresh_temp("lower_tail_tmp", expr.ty, expr.range);
                let ret = self.new_block(
                    Vec::new(),
                    Terminator::Return {
                        value: Some(Operand::Copy(Self::place(tmp))),
                    },
                );
                self.lower_assign(expr, tmp, ret)
            }
        }
    }

    pub(super) fn lower_statements(
        &mut self,
        statements: &[th::Statement<'tcx>],
        cont: BlockId,
    ) -> BlockId {
        statements
            .iter()
            .rev()
            .fold(cont, |k, stmt| self.lower_statement(stmt, k))
    }

    pub(super) fn lower_statement(&mut self, stmt: &th::Statement<'tcx>, cont: BlockId) -> BlockId {
        match stmt {
            th::Statement::Declaration {
                name,
                range,
                ty,
                val,
            } => {
                let dst = self.get_or_create_local(*name, *ty, *range);
                self.lower_assign(val, dst, cont)
            }

            th::Statement::Assignment { name, range, val } => {
                let dst = self.get_or_create_local(*name, val.ty, *range);
                self.lower_assign(val, dst, cont)
            }

            th::Statement::LetTuple { elems, range, val } => {
                // Desugar to: tuple_tmp = val; a = Field(tmp, 0); b = Field(tmp, 1); ...
                let tuple_tmp = self.fresh_temp("let_tuple_tmp", val.ty, *range);
                // Build an extraction block that fills each element local.
                let mut extract_stmts = Vec::with_capacity(elems.len());
                for (i, (name, elem_ty, _, elem_range)) in elems.iter().enumerate() {
                    let local = self.get_or_create_local(*name, *elem_ty, *elem_range);
                    extract_stmts.push(self.assign_stmt(
                        local,
                        RValue::Field {
                            base: Operand::Copy(Self::place(tuple_tmp)),
                            index: i,
                        },
                        *elem_range,
                    ));
                }
                let extract_bb = self.new_block(extract_stmts, Terminator::Goto { target: cont });
                self.lower_assign(val, tuple_tmp, extract_bb)
            }

            th::Statement::LetPattern {
                pattern,
                val,
                else_branch,
                range,
            } => {
                // Desugar `let E#V(payload) = val else fallback`:
                //
                //   scrut_tmp  = eval(val)
                //   disc_tmp   = Field(scrut_tmp, 0)        // discriminant
                //   cmp_tmp    = disc_tmp == variant_idx
                //   branch cmp_tmp:
                //     then_bb: [extract bindings from scrut_tmp] → cont
                //     else_bb: fallback_tmp = eval(fallback)
                //              [extract bindings from fallback_tmp] → cont
                //
                // The type checker guarantees `fallback` is a constructor of
                // the same variant, so the extraction in else_bb always succeeds.
                let th::MatchPattern::Variant {
                    ty: scrut_ty,
                    variant_idx,
                    payload,
                    ..
                } = pattern
                else {
                    unreachable!("LetPattern always has a Variant pattern at the top level");
                };

                let scrut_tmp = self.fresh_temp("let_pattern_scrut", *scrut_ty, *range);

                // ── then branch: extract from the matched value ─────────────────────────
                let mut then_stmts = Vec::new();
                if let Some((_, sub)) = payload {
                    self.lower_projected_pattern(
                        sub,
                        Operand::Copy(Self::place(scrut_tmp)),
                        1,
                        *range,
                        &mut then_stmts,
                    );
                }
                let then_bb = self.new_block(then_stmts, Terminator::Goto { target: cont });

                // ── else branch: evaluate fallback; extract from it ─────────────────────
                let fallback_tmp = self.fresh_temp("let_pattern_fallback", *scrut_ty, *range);
                let mut else_stmts = Vec::new();
                if let Some((_, sub)) = payload {
                    self.lower_projected_pattern(
                        sub,
                        Operand::Copy(Self::place(fallback_tmp)),
                        1,
                        *range,
                        &mut else_stmts,
                    );
                }
                let after_extract_bb =
                    self.new_block(else_stmts, Terminator::Goto { target: cont });
                let else_bb = self.lower_assign(else_branch, fallback_tmp, after_extract_bb);

                // ── discriminant check: disc == variant_idx ──────────────────────────────
                let disc_tmp = self.fresh_temp("let_pattern_disc", self.types.int, *range);
                let cmp_tmp = self.fresh_temp("let_pattern_cmp", self.types.bool, *range);
                let check_bb = self.new_block(
                    vec![
                        self.assign_stmt(
                            disc_tmp,
                            RValue::Field {
                                base: Operand::Copy(Self::place(scrut_tmp)),
                                index: 0,
                            },
                            *range,
                        ),
                        self.assign_stmt(
                            cmp_tmp,
                            RValue::BinaryOp {
                                op: Bop::Comp(CompOp::Eq),
                                left: Operand::Copy(Self::place(disc_tmp)),
                                right: Operand::Const(Constant::Int(*variant_idx as i64)),
                            },
                            *range,
                        ),
                    ],
                    Terminator::Branch {
                        cond: Operand::Copy(Self::place(cmp_tmp)),
                        then_bb,
                        else_bb,
                    },
                );

                // ── evaluate main value into scrut_tmp ──────────────────────────────────
                self.lower_assign(val, scrut_tmp, check_bb)
            }

            th::Statement::Expr(e) => self.lower_effect(e, cont),
        }
    }

    pub(super) fn lower_assign(
        &mut self,
        expr: &th::Expr<'tcx>,
        dst: LocalId,
        cont: BlockId,
    ) -> BlockId {
        // A diverging expression never produces a value to assign: lower it for
        // effects and leave the destination/continuation unreachable.
        if expr.kind == Kind::Never {
            let unreachable = self.new_block(Vec::new(), Terminator::Unreachable);
            return self.lower_effect(expr, unreachable);
        }
        match &expr.expr {
            // a shared borrow is transparent at runtime: assign the referent.
            th::Expression::Borrow(inner) => self.lower_assign(inner, dst, cont),
            th::Expression::If { cond, t, f } => {
                let then_bb = self.lower_assign(t, dst, cont);
                let else_bb = self.lower_assign(f, dst, cont);
                self.lower_pred(cond, then_bb, else_bb)
            }

            th::Expression::While { .. } => {
                let after = self.unit_assign_then_goto(dst, expr.range, cont);
                self.lower_effect(expr, after)
            }

            th::Expression::Block {
                statements,
                expr: inner_expr,
            } => {
                let k = if let Some(e) = inner_expr {
                    self.lower_assign(e, dst, cont)
                } else {
                    self.unit_assign_then_goto(dst, expr.range, cont)
                };
                self.lower_statements(statements, k)
            }

            th::Expression::Int(_)
            | th::Expression::Bool(_)
            | th::Expression::Unit
            | th::Expression::Var(_) => {
                let op = self
                    .simple_operand(expr)
                    .expect("simple expression should lower to operand");

                self.new_block(
                    vec![self.assign_stmt(dst, RValue::Use(op), expr.range)],
                    Terminator::Goto { target: cont },
                )
            }

            th::Expression::Constructor {
                variant_idx,
                payload,
                ..
            } => {
                // All enum values — including nullary variants — are Aggregates
                // in MIR. field 0 is always the discriminant (variant index as
                // Int); field 1 (if present) is the payload.
                let disc = Operand::Const(Constant::Int(*variant_idx as i64));
                match payload {
                    None => self.new_block(
                        vec![self.assign_stmt(dst, RValue::Aggregate(vec![disc]), expr.range)],
                        Terminator::Goto { target: cont },
                    ),
                    Some(p) => {
                        let p_tmp = self.fresh_temp("ctor_payload", p.ty, p.range);
                        let final_bb = self.new_block(
                            vec![self.assign_stmt(
                                dst,
                                RValue::Aggregate(vec![disc, Operand::Copy(Self::place(p_tmp))]),
                                expr.range,
                            )],
                            Terminator::Goto { target: cont },
                        );
                        self.lower_assign(p, p_tmp, final_bb)
                    }
                }
            }

            th::Expression::Tuple(elems) => {
                let elem_temps = elems
                    .iter()
                    .map(|e| self.fresh_temp("tuple_elem", e.ty, e.range))
                    .collect::<Vec<_>>();

                let final_bb = self.new_block(
                    vec![
                        self.assign_stmt(
                            dst,
                            RValue::Aggregate(
                                elem_temps
                                    .iter()
                                    .map(|id| Operand::Copy(Self::place(*id)))
                                    .collect(),
                            ),
                            expr.range,
                        ),
                    ],
                    Terminator::Goto { target: cont },
                );

                elems
                    .iter()
                    .zip(elem_temps)
                    .rev()
                    .fold(final_bb, |k, (e, tmp)| self.lower_assign(e, tmp, k))
            }

            th::Expression::UnOp { op, right } => {
                let r_tmp = self.fresh_temp("unop_right", right.ty, right.range);
                let final_bb = self.new_block(
                    vec![self.assign_stmt(
                        dst,
                        RValue::UnaryOp {
                            op: *op,
                            right: Operand::Copy(Self::place(r_tmp)),
                        },
                        expr.range,
                    )],
                    Terminator::Goto { target: cont },
                );
                self.lower_assign(right, r_tmp, final_bb)
            }

            th::Expression::BinOp { left, op, right } => {
                let l_tmp = self.fresh_temp("assign_binop_left", left.ty, left.range);
                let r_tmp = self.fresh_temp("assign_binop_left", right.ty, right.range);

                let final_bb = self.new_block(
                    vec![self.assign_stmt(
                        dst,
                        RValue::BinaryOp {
                            op: *op,
                            left: Operand::Copy(Self::place(l_tmp)),
                            right: Operand::Copy(Self::place(r_tmp)),
                        },
                        expr.range,
                    )],
                    Terminator::Goto { target: cont },
                );

                let right_bb = self.lower_assign(right, r_tmp, final_bb);
                self.lower_assign(left, l_tmp, right_bb)
            }

            th::Expression::Call { fn_name, args } => {
                let arg_temps = args
                    .iter()
                    .map(|a| self.fresh_temp("assign_call_argument", a.ty, a.range))
                    .collect::<Vec<_>>();

                let final_bb = self.new_block(
                    vec![
                        self.assign_stmt(
                            dst,
                            RValue::Call {
                                fn_name: *fn_name,
                                args: arg_temps
                                    .iter()
                                    .map(|id| Operand::Copy(Self::place(*id)))
                                    .collect(),
                            },
                            expr.range,
                        ),
                    ],
                    Terminator::Goto { target: cont },
                );

                args.iter()
                    .zip(arg_temps)
                    .rev()
                    .fold(final_bb, |k, (arg, tmp)| self.lower_assign(arg, tmp, k))
            }

            th::Expression::IntrinsicCall { fn_name, args } => {
                let arg_temps = args
                    .iter()
                    .map(|a| self.fresh_temp("assign_intrinsic_call_argument", a.ty, a.range))
                    .collect::<Vec<_>>();

                let final_bb = self.new_block(
                    vec![
                        self.assign_stmt(
                            dst,
                            RValue::IntrinsicCall {
                                fn_name: *fn_name,
                                args: arg_temps
                                    .iter()
                                    .map(|id| Operand::Copy(Self::place(*id)))
                                    .collect(),
                            },
                            expr.range,
                        ),
                    ],
                    Terminator::Goto { target: cont },
                );

                args.iter()
                    .zip(arg_temps)
                    .rev()
                    .fold(final_bb, |k, (arg, tmp)| self.lower_assign(arg, tmp, k))
            }

            th::Expression::Match { scrutinee, arms } => {
                // evaluate scrutinee into a fresh temp.
                let scrut_tmp = self.fresh_temp("match_scrutinee", scrutinee.ty, scrutinee.range);
                let scrut_operand = Operand::Copy(Self::place(scrut_tmp));

                // for each arm: first emit an "extraction block" that binds the
                // pattern's variables (registering their locals as a side
                // effect — *before* lowering the body, since the body may
                // reference them by `Var`/`var_operand`, which requires the
                // local to already exist in `local_map`), then the body block,
                // and chain extraction -> body. arms whose pattern binds
                // nothing (`Wildcard`, or a `Variant`/`Tuple` with only
                // wildcards/no payload) skip the extraction block entirely.
                let arm_bbs: Vec<BlockId> = arms
                    .iter()
                    .map(|arm| {
                        let mut bind_stmts = Vec::new();
                        self.lower_pattern_bindings(
                            &arm.pattern,
                            scrut_operand.clone(),
                            arm.range,
                            &mut bind_stmts,
                        );
                        let body_bb = self.lower_assign(&arm.body, dst, cont);
                        if bind_stmts.is_empty() {
                            body_bb
                        } else {
                            self.new_block(bind_stmts, Terminator::Goto { target: body_bb })
                        }
                    })
                    .collect();

                // build the dispatch chain backwards.
                // after the last comparison falls through -> unreachable (requiring exhaustive
                // match)
                let mut fallthrough_bb = self.new_block(vec![], Terminator::Unreachable);

                for (arm, arm_bb) in arms.iter().zip(arm_bbs.iter()).rev() {
                    match &arm.pattern {
                        th::MatchPattern::Wildcard
                        | th::MatchPattern::Binding { .. }
                        | th::MatchPattern::Tuple { .. } => {
                            // irrefutable — unconditional fall-through to this arm
                            fallthrough_bb = *arm_bb;
                        }
                        _ => {
                            // refutable pattern: delegate to the unified check-chain
                            // builder which handles Variant (with optional nested
                            // refutable payloads), IntLit, and BoolLit uniformly.
                            let check_entry = self.build_arm_check_chain(
                                &arm.pattern,
                                scrut_tmp,
                                *arm_bb,
                                fallthrough_bb,
                                arm.range,
                            );
                            fallthrough_bb = check_entry;
                        }
                    }
                }

                // lower the scrutinee, then jump into the dispatch chain
                self.lower_assign(scrutinee, scrut_tmp, fallthrough_bb)
            }
        }
    }

    pub(super) fn lower_effect(&mut self, expr: &th::Expr<'tcx>, cont: BlockId) -> BlockId {
        match &expr.expr {
            th::Expression::If { cond, t, f } => {
                let then_bb = self.lower_effect(t, cont);
                let else_bb = self.lower_effect(f, cont);
                self.lower_pred(cond, then_bb, else_bb)
            }

            th::Expression::While { cond, body } => {
                let loop_head = self.reserve_block();

                let back_edge = self.goto_block(loop_head);
                let body_bb = self.lower_effect(body, back_edge);
                let cond_entry = self.lower_pred(cond, body_bb, cont);

                self.set_block(
                    loop_head,
                    Vec::new(),
                    Terminator::Goto { target: cond_entry },
                );

                loop_head
            }

            th::Expression::Block { statements, expr } => {
                let k = if let Some(e) = expr {
                    self.lower_effect(e, cont)
                } else {
                    cont
                };
                self.lower_statements(statements, k)
            }

            th::Expression::Call { fn_name, args } => {
                let arg_temps = args
                    .iter()
                    .map(|a| self.fresh_temp("effect_call_argument", a.ty, a.range))
                    .collect::<Vec<_>>();

                let final_bb = self.new_block(
                    vec![Statement::Eval {
                        value: RValue::Call {
                            fn_name: *fn_name,
                            args: arg_temps
                                .iter()
                                .map(|id| Operand::Copy(Self::place(*id)))
                                .collect(),
                        },
                        range: expr.range,
                    }],
                    Terminator::Goto { target: cont },
                );

                args.iter()
                    .zip(arg_temps)
                    .rev()
                    .fold(final_bb, |k, (arg, tmp)| self.lower_assign(arg, tmp, k))
            }

            th::Expression::IntrinsicCall { fn_name, args } => {
                let arg_temps = args
                    .iter()
                    .map(|a| self.fresh_temp("effect_intrinsic_call_argument", a.ty, a.range))
                    .collect::<Vec<_>>();

                let final_bb = self.new_block(
                    vec![Statement::Eval {
                        value: RValue::IntrinsicCall {
                            fn_name: *fn_name,
                            args: arg_temps
                                .iter()
                                .map(|id| Operand::Copy(Self::place(*id)))
                                .collect(),
                        },
                        range: expr.range,
                    }],
                    Terminator::Goto { target: cont },
                );

                args.iter()
                    .zip(arg_temps)
                    .rev()
                    .fold(final_bb, |k, (arg, tmp)| self.lower_assign(arg, tmp, k))
            }

            _ => {
                let tmp = self.fresh_temp("effect_tmp", expr.ty, expr.range);
                self.lower_assign(expr, tmp, cont)
            }
        }
    }

    pub(super) fn lower_pred(
        &mut self,
        expr: &th::Expr<'tcx>,
        then_bb: BlockId,
        else_bb: BlockId,
    ) -> BlockId {
        match &expr.expr {
            th::Expression::Bool(true) => then_bb,
            th::Expression::Bool(false) => else_bb,

            th::Expression::If { cond, t, f } => {
                let t_bb = self.lower_pred(t, then_bb, else_bb);
                let f_bb = self.lower_pred(f, then_bb, else_bb);
                self.lower_pred(cond, t_bb, f_bb)
            }

            th::Expression::Block { statements, expr } => {
                let last = expr.as_ref().map(|e| &**e).unwrap_or_else(|| {
                    internal_bug!("block in predicate position should have a final expression")
                });
                let k = self.lower_pred(last, then_bb, else_bb);
                self.lower_statements(statements, k)
            }

            th::Expression::UnOp {
                op: Uop::Not,
                right,
            } => self.lower_pred(right, else_bb, then_bb),

            th::Expression::BinOp {
                left,
                op: Bop::Comp(_),
                right,
            } => {
                let l_tmp = self.fresh_temp("pred_binop_left", left.ty, left.range);
                let r_tmp = self.fresh_temp("pred_binop_right", right.ty, right.range);

                let cmp_tmp = self.fresh_temp("pred_binop_comp", self.types.bool, expr.range);

                let branch_bb = self.new_block(
                    Vec::new(),
                    Terminator::Branch {
                        cond: Operand::Copy(Self::place(cmp_tmp)),
                        then_bb,
                        else_bb,
                    },
                );

                let cmp_bb = self.new_block(
                    vec![self.assign_stmt(
                        cmp_tmp,
                        RValue::BinaryOp {
                            op: match &expr.expr {
                                th::Expression::BinOp { op, .. } => *op,
                                _ => unreachable!(),
                            },
                            left: Operand::Copy(Self::place(l_tmp)),
                            right: Operand::Copy(Self::place(r_tmp)),
                        },
                        expr.range,
                    )],
                    Terminator::Goto { target: branch_bb },
                );

                let right_bb = self.lower_assign(right, r_tmp, cmp_bb);
                self.lower_assign(left, l_tmp, right_bb)
            }

            th::Expression::Var(v) => self.new_block(
                Vec::new(),
                Terminator::Branch {
                    cond: self.var_operand(v),
                    then_bb,
                    else_bb,
                },
            ),

            _ => {
                let tmp = self.fresh_temp("lower_pred_result", expr.ty, expr.range);
                let branch_bb = self.new_block(
                    Vec::new(),
                    Terminator::Branch {
                        cond: Operand::Copy(Self::place(tmp)),
                        then_bb,
                        else_bb,
                    },
                );
                self.lower_assign(expr, tmp, branch_bb)
            }
        }
    }
}
