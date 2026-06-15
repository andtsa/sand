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
use crate::lang::types::TyKind;

/// One cell of the pattern matrix used by the Maranget decision-tree compiler
/// ([`FnCx::compile_match_matrix`]). A `Wild` is a synthesised wildcard
/// produced when a wildcard row is specialised against a constructor (it has no
/// backing pattern node); `Pat` borrows a real pattern from an arm.
#[derive(Clone, Copy)]
enum Cell<'a, 'tcx> {
    Wild,
    Pat(&'a th::MatchPattern<'tcx>),
}

impl<'a, 'tcx> Cell<'a, 'tcx> {
    /// A cell that imposes no test and so cannot fail to match: a synthesised
    /// wildcard, a source `_`, or a variable binding (binding extraction
    /// happens separately at the matched arm, walking the original
    /// pattern).
    fn is_wild(self) -> bool {
        matches!(
            self,
            Cell::Wild | Cell::Pat(th::MatchPattern::Wildcard | th::MatchPattern::Binding { .. })
        )
    }
}

/// One row of the pattern matrix: a pattern cell per current occurrence column,
/// tagged with the source arm it came from (so a matched leaf routes to that
/// arm's already-built entry block).
struct Row<'a, 'tcx> {
    cells: Vec<Cell<'a, 'tcx>>,
    arm: usize,
}

/// `v` with element `idx` replaced by the elements of `replacement`
/// (the matrix specialisation operation: one column becomes a constructor's
/// sub-columns).
fn splice<T: Clone>(v: &[T], idx: usize, replacement: &[T]) -> Vec<T> {
    let mut out = Vec::with_capacity(v.len() + replacement.len());
    out.extend_from_slice(&v[..idx]);
    out.extend_from_slice(replacement);
    out.extend_from_slice(&v[idx + 1..]);
    out
}

/// `v` with element `idx` removed (the default-matrix column drop).
fn remove<T: Clone>(v: &[T], idx: usize) -> Vec<T> {
    let mut out = Vec::with_capacity(v.len() - 1);
    out.extend_from_slice(&v[..idx]);
    out.extend_from_slice(&v[idx + 1..]);
    out
}

/// Specialise a tuple column's cell into `arity` sub-cells: a tuple pattern
/// contributes its element patterns, a wildcard/binding contributes wildcards.
fn expand_tuple_cell<'a, 'tcx>(cell: Cell<'a, 'tcx>, arity: usize) -> Vec<Cell<'a, 'tcx>> {
    match cell {
        Cell::Pat(th::MatchPattern::Tuple { elems, .. }) => elems.iter().map(Cell::Pat).collect(),
        _ => vec![Cell::Wild; arity],
    }
}

/// Whether a literal cell matches the constant `k`.
fn cell_matches_const(cell: Cell<'_, '_>, k: &Constant) -> bool {
    match cell {
        Cell::Pat(th::MatchPattern::IntLit(n)) => *k == Constant::Int(*n),
        Cell::Pat(th::MatchPattern::BoolLit(b)) => *k == Constant::Bool(*b),
        _ => false,
    }
}

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
        Place::local(local)
    }

    pub(super) fn const_operand(expr: &th::Expr<'_>) -> Option<Operand> {
        match &expr.expr {
            th::Expression::Int(i) => Some(Operand::Const(Constant::Int(*i))),
            th::Expression::Bool(b) => Some(Operand::Const(Constant::Bool(*b))),
            th::Expression::Unit => Some(Operand::Const(Constant::Unit)),
            // constructors, including nullary ones, are no longer constants:
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

    /// Lower a block's scope-exit `drops` to MIR `Statement::Drop`s,
    /// one per variable, in the order given (already reverse-declaration
    /// order).
    fn drop_stmts(&self, drops: &[UniqVar<'tcx>], range: Range) -> Vec<Statement<'tcx>> {
        drops
            .iter()
            .map(|v| {
                let local = *self
                    .local_map
                    .get(v)
                    .unwrap_or_else(|| internal_bug!("missing local for dropped variable {v:?}"));
                Statement::Drop {
                    place: Place::local(local),
                    range,
                }
            })
            .collect()
    }

    /// A continuation that runs `drops` then jumps to `cont`, used to drop a
    /// block's bindings *after* its value flows to `cont`. Returns `cont`
    /// unchanged when there is nothing to drop.
    fn drop_cont(&mut self, drops: &[UniqVar<'tcx>], range: Range, cont: BlockId) -> BlockId {
        if drops.is_empty() {
            return cont;
        }
        let stmts = self.drop_stmts(drops, range);
        self.new_block(stmts, Terminator::Goto { target: cont })
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
    /// `Wildcard`s and direct `Binding`s need no intermediate storage:
    /// they either discard the value (no statement emitted; sound because
    /// there is no partial-move tracking, so a discarded sub-value simply
    /// remains owned by whatever already holds `source`)
    /// or are assigned straight from `source`. Compound sub-patterns
    /// (`Tuple`, or a `Variant`'s payload) delegate to
    /// [`Self::lower_projected_pattern`], which materializes the projected
    /// value into a fresh temp before recursing (see its doc comment for
    /// why that's necessary).
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
                // no bindings; the check was already done in the dispatch
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
    /// `RValue::Field` directly: `Binding` is assigned straight from it,
    /// `Wildcard` discards it (no statement). compound patterns (`Tuple`,
    /// `Variant`) must first materialize the projection into a fresh temp,
    /// because `RValue::Field` isn't an `Operand`: further projection needs
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
                // discard: no partial-move tracking, no statement needed
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
                // nullary inner variant: no payload, so nothing to bind.
                // The discriminant check was done in the dispatch chain.
            }
            th::MatchPattern::IntLit(_) | th::MatchPattern::BoolLit(_) => {
                // A nested literal binds nothing; its equality test is emitted
                // by the decision tree (which extracts this
                // sub-occurrence itself).
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

            th::Expression::Block {
                statements,
                expr: inner,
                drops,
            } if drops.is_empty() => {
                let cont = if let Some(e) = inner {
                    self.lower_tail(e)
                } else {
                    self.new_block(Vec::new(), Terminator::Return { value: None })
                };
                self.lower_statements(statements, cont)
            }

            // A block with scope-exit drops: the value must be computed *before*
            // the drops run, and the drops *before* the `Return`. Route the value
            // through a temp (non-unit) or as an effect (unit), then drop, then
            // return, so the drops land between value and `Return`.
            th::Expression::Block {
                statements,
                expr: inner,
                drops,
            } => {
                let drop_then_return = |cx: &mut Self, value: Option<Operand>| {
                    let stmts = cx.drop_stmts(drops, expr.range);
                    cx.new_block(stmts, Terminator::Return { value })
                };
                let cont = match inner {
                    Some(e) if expr.ty != self.types.unit => {
                        let tmp = self.fresh_temp("tail_block_tmp", e.ty, e.range);
                        let ret = drop_then_return(self, Some(Operand::Copy(Self::place(tmp))));
                        self.lower_assign(e, tmp, ret)
                    }
                    Some(e) => {
                        let ret = drop_then_return(self, None);
                        self.lower_effect(e, ret)
                    }
                    None => drop_then_return(self, None),
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

            // `*reference = value`: evaluate `value` into a temp, then store it
            // *through* the reference (`Assign { dst: Place::deref(ref), .. }`).
            th::Statement::DerefAssign {
                reference,
                value,
                range,
            } => {
                let value_tmp = self.fresh_temp("deref_assign_val", value.ty, *range);
                let ref_is_var = matches!(reference.expr, th::Expression::Var(_));
                let ref_local = match &reference.expr {
                    th::Expression::Var(v) => {
                        self.get_or_create_local(*v, reference.ty, reference.range)
                    }
                    _ => self.fresh_temp("deref_assign_ref", reference.ty, reference.range),
                };
                let store = Statement::Assign {
                    dst: Place::deref(ref_local),
                    value: RValue::Use(Operand::Copy(Self::place(value_tmp))),
                    range: *range,
                };
                let store_bb = self.new_block(vec![store], Terminator::Goto { target: cont });
                let after_val = self.lower_assign(value, value_tmp, store_bb);
                if ref_is_var {
                    after_val
                } else {
                    self.lower_assign(reference, ref_local, after_val)
                }
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
            // `&inner`: a real pointer to the referent's
            // storage. A borrow of a *variable* points at that variable's local;
            // a borrow of a *temporary* materialises it into a fresh local first,
            // then points at that. The inverse of a `*` (`[Deref]`) projection.
            th::Expression::Borrow(inner, _) => {
                if let th::Expression::Var(v) = &inner.expr {
                    let local = self.get_or_create_local(*v, inner.ty, inner.range);
                    let stmt = self.assign_stmt(dst, RValue::Ref(Self::place(local)), expr.range);
                    self.new_block(vec![stmt], Terminator::Goto { target: cont })
                } else {
                    let tmp = self.fresh_temp("borrow_operand", inner.ty, inner.range);
                    let stmt = self.assign_stmt(dst, RValue::Ref(Self::place(tmp)), expr.range);
                    let assign = self.new_block(vec![stmt], Terminator::Goto { target: cont });
                    self.lower_assign(inner, tmp, assign)
                }
            }
            // `*inner`: a load through the reference; a `[Deref]`
            // place reads the value the reference points at.
            th::Expression::Deref(inner) => {
                if let th::Expression::Var(v) = &inner.expr {
                    let local = self.get_or_create_local(*v, inner.ty, inner.range);
                    let stmt = self.assign_stmt(
                        dst,
                        RValue::Use(Operand::Copy(Place::deref(local))),
                        expr.range,
                    );
                    self.new_block(vec![stmt], Terminator::Goto { target: cont })
                } else {
                    let tmp = self.fresh_temp("deref_operand", inner.ty, inner.range);
                    let stmt = self.assign_stmt(
                        dst,
                        RValue::Use(Operand::Copy(Place::deref(tmp))),
                        expr.range,
                    );
                    let assign = self.new_block(vec![stmt], Terminator::Goto { target: cont });
                    self.lower_assign(inner, tmp, assign)
                }
            }
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
                drops,
            } => {
                // drops run after the block's value is assigned, before `cont`.
                let after = self.drop_cont(drops, expr.range, cont);
                let k = if let Some(e) = inner_expr {
                    self.lower_assign(e, dst, after)
                } else {
                    self.unit_assign_then_goto(dst, expr.range, after)
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
                // All enum values, including nullary variants, are Aggregates
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

            // A lifted closure value: a fat pointer to the lifted
            // function plus its captured operands (the enclosing locals it
            // closes over), packed into the environment.
            th::Expression::Closure { func, captures } => {
                let env = captures.iter().map(|(v, _)| self.var_operand(v)).collect();
                let stmt = self.assign_stmt(
                    dst,
                    RValue::Closure {
                        fn_name: *func,
                        env,
                    },
                    expr.range,
                );
                self.new_block(vec![stmt], Terminator::Goto { target: cont })
            }

            // Indirect call: evaluate the callee and argument into
            // temps, then call through the closure value.
            th::Expression::Apply { func, arg } => {
                let func_tmp = self.fresh_temp("apply_callee", func.ty, func.range);
                let arg_tmp = self.fresh_temp("apply_arg", arg.ty, arg.range);
                let final_bb = self.new_block(
                    vec![self.assign_stmt(
                        dst,
                        RValue::CallIndirect {
                            callee: Operand::Copy(Self::place(func_tmp)),
                            args: vec![Operand::Copy(Self::place(arg_tmp))],
                        },
                        expr.range,
                    )],
                    Terminator::Goto { target: cont },
                );
                let after_arg = self.lower_assign(arg, arg_tmp, final_bb);
                self.lower_assign(func, func_tmp, after_arg)
            }

            th::Expression::Lambda { .. } => {
                internal_bug!("lambda should have been lifted during monomorphisation")
            }

            th::Expression::IntrinsicCall {
                fn_name,
                args,
                type_args,
            } if fn_name.is_type_arg_intrinsic() => {
                // `size_of::<T>()`: no value args; assign the size directly
                // (the concrete type is carried on `RValue::SizeOf`).
                let stmt = self.assign_stmt(dst, RValue::SizeOf(type_args[0]), expr.range);
                self.new_block(vec![stmt], Terminator::Goto { target: cont })
            }

            th::Expression::IntrinsicCall { fn_name, args, .. } => {
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

            // Typeclass method calls are resolved to concrete `Call`s by the type
            // checker (concrete) or monomorphisation (generic), so none survive
            // into explication.
            th::Expression::MethodCall { .. } => {
                internal_bug!("typeclass method call survived monomorphisation")
            }

            th::Expression::Match { scrutinee, arms } => {
                // evaluate scrutinee into a fresh temp.
                let scrut_tmp = self.fresh_temp("match_scrutinee", scrutinee.ty, scrutinee.range);
                let scrut_operand = Operand::Copy(Self::place(scrut_tmp));

                // for each arm: first emit an "extraction block" that binds the
                // pattern's variables (registering their locals as a side
                // effect, *before* lowering the body, since the body may
                // reference them by `Var`/`var_operand`, which requires the
                // local to already exist in `local_map`), then the body block,
                // and chain extraction -> body. arms whose pattern binds
                // nothing (`Wildcard`, or a `Variant`/`Tuple` with only
                // wildcards/no payload) skip the extraction block entirely.
                //
                // Binding extraction always walks the *original* arm pattern from
                // the scrutinee, so it is independent of how the decision tree
                // routes control: the tree only decides *which* arm matches first.
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

                // Compile the arms into a Maranget decision tree: a single shared
                // block per occurrence/constructor test, falling through to a lone
                // unreachable block (the match is exhaustive).
                let fail_bb = self.new_block(vec![], Terminator::Unreachable);
                let rows: Vec<Row> = arms
                    .iter()
                    .enumerate()
                    .map(|(i, arm)| Row {
                        cells: vec![Cell::Pat(&arm.pattern)],
                        arm: i,
                    })
                    .collect();
                let occ = vec![(scrut_tmp, scrutinee.ty)];
                let fallthrough_bb =
                    self.compile_match_matrix(&occ, &rows, &arm_bbs, fail_bb, scrutinee.range);

                // lower the scrutinee, then jump into the dispatch chain
                self.lower_assign(scrutinee, scrut_tmp, fallthrough_bb)
            }
        }
    }

    /// Compile a pattern matrix into a decision tree
    /// (Maranget, *Compiling Pattern Matching to Good Decision Trees*, ML'08).
    ///
    /// `occ` are the occurrences: the temps holding the sub-values currently
    /// under scrutiny, one per matrix column, paired with their types. `rows`
    /// is the matrix: each row is one source arm's pattern cells over those
    /// columns. Returns the entry block that routes control to the first
    /// matching arm's `arm_bbs` entry, or to `fail_bb` if nothing matches.
    ///
    /// At each step it selects the first column where the *first* row is
    /// refutable (guaranteeing progress on that row), switches on the
    /// constructor there once, and recurses into the specialised sub-matrices,
    /// so each occurrence is tested at most once along any path and common
    /// sub-trees are shared, unlike a per-arm backtracking chain. Variable
    /// bindings are *not* handled here: they are extracted at the matched arm
    /// by walking the original pattern from the scrutinee (see the `Match`
    /// arm), so a binding cell is treated exactly like a wildcard for
    /// dispatch.
    fn compile_match_matrix(
        &mut self,
        occ: &[(LocalId, Ty<'tcx>)],
        rows: &[Row<'_, 'tcx>],
        arm_bbs: &[BlockId],
        fail_bb: BlockId,
        range: Range,
    ) -> BlockId {
        // No rows left: nothing can match here.
        let Some(first) = rows.first() else {
            return fail_bb;
        };
        // The first row imposes no remaining test → it matches unconditionally.
        if first.cells.iter().all(|c| c.is_wild()) {
            return arm_bbs[first.arm];
        }
        // Otherwise switch on the first column where the first row is refutable.
        let col = first.cells.iter().position(|c| !c.is_wild()).unwrap();
        // Sub-occurrence types come from the constructor patterns (or the tuple
        // type), so the occurrence's own type is not needed here, only its local.
        let occ_local = occ[col].0;

        match first.cells[col] {
            // unreachable: `is_wild` excluded these, and a `Pat` at `col` must
            // exist (we found a non-wild cell there).
            Cell::Wild
            | Cell::Pat(th::MatchPattern::Wildcard | th::MatchPattern::Binding { .. }) => {
                internal_bug!("selected matrix column is wildcard")
            }

            // A tuple has exactly one constructor, so it never branches: expand
            // the column into one sub-column per element and recurse.
            Cell::Pat(th::MatchPattern::Tuple { ty: tup_ty, elems }) => {
                let arity = elems.len();
                let elem_tys: Vec<Ty<'tcx>> = match tup_ty.kind() {
                    TyKind::Tuple(es) => es.to_vec(),
                    _ => internal_bug!("tuple pattern with non-tuple type {tup_ty}"),
                };
                let sub_occ: Vec<(LocalId, Ty<'tcx>)> = elem_tys
                    .iter()
                    .map(|t| (self.fresh_temp("match_tuple_elem", *t, range), *t))
                    .collect();
                let new_occ = splice(occ, col, &sub_occ);
                let new_rows: Vec<Row> = rows
                    .iter()
                    .map(|r| Row {
                        cells: splice(&r.cells, col, &expand_tuple_cell(r.cells[col], arity)),
                        arm: r.arm,
                    })
                    .collect();
                let body = self.compile_match_matrix(&new_occ, &new_rows, arm_bbs, fail_bb, range);
                // Materialise the element temps, then continue.
                let stmts = sub_occ
                    .iter()
                    .enumerate()
                    .map(|(i, (tmp, _))| {
                        self.assign_stmt(
                            *tmp,
                            RValue::Field {
                                base: Operand::Copy(Self::place(occ_local)),
                                index: i,
                            },
                            range,
                        )
                    })
                    .collect();
                self.new_block(stmts, Terminator::Goto { target: body })
            }

            // An enum column: switch on the discriminant.
            Cell::Pat(th::MatchPattern::Variant { .. }) => {
                // Constructors present in this column, in first-appearance order.
                let mut ctors: Vec<(usize, Option<Ty<'tcx>>)> = Vec::new();
                for r in rows {
                    if let Cell::Pat(th::MatchPattern::Variant {
                        variant_idx,
                        payload,
                        ..
                    }) = r.cells[col]
                        && !ctors.iter().any(|(v, _)| v == variant_idx)
                    {
                        ctors.push((*variant_idx, payload.as_ref().map(|(t, _)| *t)));
                    }
                }

                // Default sub-matrix: rows whose column is a wildcard (reached
                // when the discriminant is none of the tested constructors).
                let default_bb = {
                    let default_rows: Vec<Row> = rows
                        .iter()
                        .filter(|r| r.cells[col].is_wild())
                        .map(|r| Row {
                            cells: remove(&r.cells, col),
                            arm: r.arm,
                        })
                        .collect();
                    let default_occ = remove(occ, col);
                    if default_rows.is_empty() {
                        fail_bb
                    } else {
                        self.compile_match_matrix(
                            &default_occ,
                            &default_rows,
                            arm_bbs,
                            fail_bb,
                            range,
                        )
                    }
                };

                // One specialised sub-tree per constructor.
                let ctor_bbs: Vec<BlockId> = ctors
                    .iter()
                    .map(|(vi, payload_ty)| {
                        self.specialize_variant(
                            occ,
                            rows,
                            col,
                            occ_local,
                            *vi,
                            *payload_ty,
                            arm_bbs,
                            fail_bb,
                            range,
                        )
                    })
                    .collect();

                // Read the discriminant once, then a chain of equality tests.
                let disc = self.fresh_temp("match_disc", self.types.int, range);
                let mut else_bb = default_bb;
                for ((vi, _), target) in ctors.iter().zip(&ctor_bbs).rev() {
                    else_bb =
                        self.eq_branch(disc, Constant::Int(*vi as i64), *target, else_bb, range);
                }
                self.new_block(
                    vec![self.assign_stmt(
                        disc,
                        RValue::Field {
                            base: Operand::Copy(Self::place(occ_local)),
                            index: 0,
                        },
                        range,
                    )],
                    Terminator::Goto { target: else_bb },
                )
            }

            // Literal columns: compare the occurrence value directly. Literals
            // carry no payload, so the column is simply dropped on a match.
            Cell::Pat(th::MatchPattern::IntLit(_)) | Cell::Pat(th::MatchPattern::BoolLit(_)) => {
                let mut consts: Vec<Constant> = Vec::new();
                for r in rows {
                    let k = match r.cells[col] {
                        Cell::Pat(th::MatchPattern::IntLit(n)) => Some(Constant::Int(*n)),
                        Cell::Pat(th::MatchPattern::BoolLit(b)) => Some(Constant::Bool(*b)),
                        _ => None,
                    };
                    if let Some(k) = k
                        && !consts.contains(&k)
                    {
                        consts.push(k);
                    }
                }

                let default_rows: Vec<Row> = rows
                    .iter()
                    .filter(|r| r.cells[col].is_wild())
                    .map(|r| Row {
                        cells: remove(&r.cells, col),
                        arm: r.arm,
                    })
                    .collect();
                let default_occ = remove(occ, col);
                let default_bb = if default_rows.is_empty() {
                    fail_bb
                } else {
                    self.compile_match_matrix(&default_occ, &default_rows, arm_bbs, fail_bb, range)
                };

                let mut else_bb = default_bb;
                for k in consts.iter().rev() {
                    // Rows matching this literal (or wildcard), column dropped.
                    let lit_rows: Vec<Row> = rows
                        .iter()
                        .filter(|r| cell_matches_const(r.cells[col], k) || r.cells[col].is_wild())
                        .map(|r| Row {
                            cells: remove(&r.cells, col),
                            arm: r.arm,
                        })
                        .collect();
                    let lit_occ = remove(occ, col);
                    let target =
                        self.compile_match_matrix(&lit_occ, &lit_rows, arm_bbs, fail_bb, range);
                    else_bb = self.eq_branch(occ_local, k.clone(), target, else_bb, range);
                }
                else_bb
            }
        }
    }

    /// Build the specialised sub-tree for one enum constructor `vi` of the
    /// column `col` (occurrence `occ_local`): extract the payload (if any)
    /// into a fresh occurrence and recurse on the rows that match `vi` or
    /// are wildcards.
    #[allow(clippy::too_many_arguments)]
    fn specialize_variant(
        &mut self,
        occ: &[(LocalId, Ty<'tcx>)],
        rows: &[Row<'_, 'tcx>],
        col: usize,
        occ_local: LocalId,
        vi: usize,
        payload_ty: Option<Ty<'tcx>>,
        arm_bbs: &[BlockId],
        fail_bb: BlockId,
        range: Range,
    ) -> BlockId {
        let arity = if payload_ty.is_some() { 1 } else { 0 };
        let payload_tmp = payload_ty.map(|t| (self.fresh_temp("match_payload", t, range), t));
        let sub_occ: Vec<(LocalId, Ty<'tcx>)> = payload_tmp.into_iter().collect();
        let new_occ = splice(occ, col, &sub_occ);

        let new_rows: Vec<Row> = rows
            .iter()
            .filter_map(|r| {
                let sub = match r.cells[col] {
                    Cell::Pat(th::MatchPattern::Variant {
                        variant_idx,
                        payload,
                        ..
                    }) if *variant_idx == vi => match payload {
                        Some((_, inner)) => vec![Cell::Pat(inner.as_ref())],
                        None => vec![],
                    },
                    c if c.is_wild() => vec![Cell::Wild; arity],
                    // a different constructor: this row cannot match `vi`.
                    _ => return None,
                };
                Some(Row {
                    cells: splice(&r.cells, col, &sub),
                    arm: r.arm,
                })
            })
            .collect();

        let body = self.compile_match_matrix(&new_occ, &new_rows, arm_bbs, fail_bb, range);
        match payload_tmp {
            Some((tmp, _)) => self.new_block(
                vec![self.assign_stmt(
                    tmp,
                    RValue::Field {
                        base: Operand::Copy(Self::place(occ_local)),
                        index: 1,
                    },
                    range,
                )],
                Terminator::Goto { target: body },
            ),
            None => body,
        }
    }

    /// A block testing `lhs == k`, branching to `then_bb` on equality and
    /// `else_bb` otherwise. Returns the new block's id.
    fn eq_branch(
        &mut self,
        lhs: LocalId,
        k: Constant,
        then_bb: BlockId,
        else_bb: BlockId,
        range: Range,
    ) -> BlockId {
        let cmp = self.fresh_temp("match_cmp", self.types.bool, range);
        self.new_block(
            vec![self.assign_stmt(
                cmp,
                RValue::BinaryOp {
                    op: Bop::Comp(CompOp::Eq),
                    left: Operand::Copy(Self::place(lhs)),
                    right: Operand::Const(k),
                },
                range,
            )],
            Terminator::Branch {
                cond: Operand::Copy(Self::place(cmp)),
                then_bb,
                else_bb,
            },
        )
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

            th::Expression::Block {
                statements,
                expr: inner,
                drops,
            } => {
                let after = self.drop_cont(drops, expr.range, cont);
                let k = if let Some(e) = inner {
                    self.lower_effect(e, after)
                } else {
                    after
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

            // `size_of::<T>()` is pure; in effect position its value is
            // discarded, so it lowers to nothing.
            th::Expression::IntrinsicCall { fn_name, .. } if fn_name.is_type_arg_intrinsic() => {
                cont
            }

            th::Expression::IntrinsicCall { fn_name, args, .. } => {
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

            th::Expression::Block {
                statements,
                expr: inner,
                drops,
            } => {
                let last = inner.as_ref().map(|e| &**e).unwrap_or_else(|| {
                    internal_bug!("block in predicate position should have a final expression")
                });
                let k = if drops.is_empty() {
                    self.lower_pred(last, then_bb, else_bb)
                } else {
                    // Compute the predicate into a temp, run the block's drops,
                    // then branch, so drops land after the value, before the
                    // branch on it.
                    let tmp = self.fresh_temp("pred_block_tmp", last.ty, last.range);
                    let stmts = self.drop_stmts(drops, expr.range);
                    let br = self.new_block(
                        stmts,
                        Terminator::Branch {
                            cond: Operand::Copy(Self::place(tmp)),
                            then_bb,
                            else_bb,
                        },
                    );
                    self.lower_assign(last, tmp, br)
                };
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
