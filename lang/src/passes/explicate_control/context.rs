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
use crate::lang::types::Ty;

pub(super) struct FnCx {
    #[allow(dead_code)]
    name: FunRef,
    #[allow(dead_code)]
    range: Range,
    #[allow(dead_code)]
    ret_type: Ty,

    pub(super) locals: Vec<LocalDecl>,
    local_map: Map<UniqVar, LocalId>,

    pub(super) blocks: Vec<BasicBlock>,
    next_temp: usize,
}

impl FnCx {
    pub(super) fn new(name: FunRef, range: Range, ret_type: Ty) -> Self {
        Self {
            name,
            range,
            ret_type,
            locals: Vec::new(),
            local_map: Map::new(),
            blocks: Vec::new(),
            next_temp: 0,
        }
    }

    pub(super) fn new_block(
        &mut self,
        statements: Vec<Statement>,
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
        statements: Vec<Statement>,
        terminator: Terminator,
    ) {
        self.blocks[id.0] = BasicBlock {
            id,
            statements,
            terminator,
        };
    }

    pub(super) fn get_or_create_local(&mut self, name: UniqVar, ty: Ty, range: Range) -> LocalId {
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

    pub(super) fn fresh_temp(&mut self, hint: &'static str, ty: Ty, range: Range) -> LocalId {
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

    pub(super) fn const_operand(expr: &th::Expr) -> Option<Operand> {
        match &expr.expr {
            th::Expression::Int(i) => Some(Operand::Const(Constant::Int(*i))),
            th::Expression::Bool(b) => Some(Operand::Const(Constant::Bool(*b))),
            th::Expression::Unit => Some(Operand::Const(Constant::Unit)),
            th::Expression::Constructor {
                enum_ref,
                variant_idx,
            } => Some(Operand::Const(Constant::EnumVariant {
                enum_ref: *enum_ref,
                variant_idx: *variant_idx,
            })),
            _ => None,
        }
    }

    pub(super) fn var_operand(&self, name: &UniqVar) -> Operand {
        let local = *self
            .local_map
            .get(name)
            .unwrap_or_else(|| internal_bug!("missing local for variable {name:?}"));
        Operand::Copy(Self::place(local))
    }

    pub(super) fn simple_operand(&self, expr: &th::Expr) -> Option<Operand> {
        if let Some(c) = Self::const_operand(expr) {
            return Some(c);
        }

        match &expr.expr {
            th::Expression::Var(v) => Some(self.var_operand(v)),
            _ => None,
        }
    }

    pub(super) fn assign_stmt(&self, dst: LocalId, value: RValue, range: Range) -> Statement {
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

    pub(super) fn lower_tail(&mut self, expr: &th::Expr) -> BlockId {
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

            _ if expr.ty == Ty::UNIT => {
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
        statements: &[th::Statement],
        cont: BlockId,
    ) -> BlockId {
        statements
            .iter()
            .rev()
            .fold(cont, |k, stmt| self.lower_statement(stmt, k))
    }

    pub(super) fn lower_statement(&mut self, stmt: &th::Statement, cont: BlockId) -> BlockId {
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

            th::Statement::Expr(e) => self.lower_effect(e, cont),
        }
    }

    pub(super) fn lower_assign(&mut self, expr: &th::Expr, dst: LocalId, cont: BlockId) -> BlockId {
        match &expr.expr {
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
            | th::Expression::Constructor { .. }
            | th::Expression::Var(_) => {
                let op = self
                    .simple_operand(expr)
                    .expect("simple expression should lower to operand");

                self.new_block(
                    vec![self.assign_stmt(dst, RValue::Use(op), expr.range)],
                    Terminator::Goto { target: cont },
                )
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

                // lower each arm body (all continue to `cont` after assigning `dst`).
                let arm_bbs: Vec<BlockId> = arms
                    .iter()
                    .map(|arm| self.lower_assign(&arm.body, dst, cont))
                    .collect();

                // build the dispatch chain backwards.
                // after the last comparison falls through -> unreachable (requiring exhaustive
                // match)
                let mut fallthrough_bb = self.new_block(vec![], Terminator::Unreachable);

                for (arm, arm_bb) in arms.iter().zip(arm_bbs.iter()).rev() {
                    match &arm.pattern {
                        th::MatchPattern::Wildcard => {
                            // wildcard: unconditional fall-through to this arm
                            fallthrough_bb = *arm_bb;
                        }
                        th::MatchPattern::Variant {
                            enum_ref,
                            variant_idx,
                        } => {
                            let cmp_tmp = self.fresh_temp("match_cmp", Ty::BOOL, arm.range);
                            let check_bb = self.new_block(
                                vec![self.assign_stmt(
                                    cmp_tmp,
                                    RValue::BinaryOp {
                                        op: Bop::Comp(CompOp::Eq),
                                        left: Operand::Copy(Self::place(scrut_tmp)),
                                        right: Operand::Const(Constant::EnumVariant {
                                            enum_ref: *enum_ref,
                                            variant_idx: *variant_idx,
                                        }),
                                    },
                                    arm.range,
                                )],
                                Terminator::Branch {
                                    cond: Operand::Copy(Self::place(cmp_tmp)),
                                    then_bb: *arm_bb,
                                    else_bb: fallthrough_bb,
                                },
                            );
                            fallthrough_bb = check_bb;
                        }
                    }
                }

                // lower the scrutinee, then jump into the dispatch chain
                self.lower_assign(scrutinee, scrut_tmp, fallthrough_bb)
            }
        }
    }

    pub(super) fn lower_effect(&mut self, expr: &th::Expr, cont: BlockId) -> BlockId {
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
        expr: &th::Expr,
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

                let cmp_tmp = self.fresh_temp("pred_binop_comp", Ty::BOOL, expr.range);

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
