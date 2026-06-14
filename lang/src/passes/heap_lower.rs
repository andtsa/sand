//! Memory Step C.5 — heap lowering.
//!
//! Rewrites every `deriving Heaped` enum into a `Unique<Node>` handle over the
//! core-lib allocator, so that **no heaped enum survives** into ownership,
//! monomorphisation, or codegen — those passes only ever see ordinary enums,
//! `Unique` handles, and `Ptr` ops.
//!
//! The transformation, per the Step C plan:
//!   * **Node synthesis** — for each heaped enum `E<T…>`, synthesise a
//!     non-recursive node enum `E$Node<T…>` with the same variants, every
//!     heaped field rewritten to its `Unique<…$Node>` handle (so the type is
//!     finite — recursion goes through a pointer).
//!   * **Type rewrite** — a homomorphism `R` applied uniformly to every type in
//!     the program: a heaped `E<a>` becomes `Unique<E$Node<a>>`.
//!   * **Construct** — `E#C(p)` becomes `unique_alloc(E$Node#C(p))`.
//!   * **Consuming match** — `match s { … }` (when `s` is heaped and some arm
//!     inspects a variant) becomes `{ let n = unique_take(s); match n { … } }`,
//!     the patterns retargeted to the node enum. Every payload position is
//!     bound (wildcards become fresh bindings) so ownership's scope-exit drops
//!     reclaim any field the arm does not move out — no leak.
//!
//! Runs **before** ownership (so drops are inserted uniformly on the resulting
//! `Unique` handles) and before mono (so the injected generic `unique_*` calls
//! and node types are instantiated like any other generic).

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::Range;
use crate::compiler::structure::VarDeclType;
use crate::internal_bug;
use crate::ir_types::typed_hir::*;
use crate::lang::types::EnumRef;
use crate::lang::types::Kind;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;

// `node_of` is keyed by `EnumRef`, whose interior `Cell` (variant payloads) is
// never part of its hash/eq (it hashes by arena identity), so the mutable-key
// lint does not apply — mirroring the other enum-keyed maps in the compiler.
#[allow(clippy::mutable_key_type)]
pub fn lower<'tcx>(ctx: &mut CompileCtx<'tcx>, program: TypedProgram<'tcx>) -> TypedProgram<'tcx> {
    let heaped: Vec<EnumRef<'tcx>> = ctx
        .all_enums()
        .filter(|e| ctx.get_enum(*e).heaped_strategy().is_some())
        .collect();
    if heaped.is_empty() {
        return program;
    }

    let unique_er = ctx
        .unique_enum()
        .expect("the `Unique` lang-item must be loaded (core.sand) for heap lowering");
    let alloc_fn = ctx
        .lookup_function_by_name("unique_alloc")
        .expect("core.sand defines `unique_alloc`");
    let take_fn = ctx
        .lookup_function_by_name("unique_take")
        .expect("core.sand defines `unique_take`");

    // Phase 1: register every node enum with empty payloads, so mutually
    // recursive heaped types can reference each other's nodes.
    let mut node_of: Map<EnumRef<'tcx>, EnumRef<'tcx>> = Map::default();
    for &e in &heaped {
        let def = ctx.get_enum(e);
        let node_name = format!("{}$Node", def.name);
        let variant_names = def.variants.iter().map(|v| v.name.clone()).collect();
        let type_params = def.type_params.clone();
        let region_params = def.region_params.clone();
        let range = def.range;
        let module = def.src_module;
        let node_er = ctx
            .register_enum(
                &node_name,
                variant_names,
                type_params,
                region_params,
                range,
                module,
                vec![],
            )
            .expect("synthesised node enum name (with `$`) cannot collide with a source enum");
        node_of.insert(e, node_er);
    }

    let mut hl = HeapLower {
        ctx,
        node_of,
        unique_er,
        alloc_fn,
        take_fn,
    };

    // Phase 2: fill each node enum's payloads with the rewritten field types.
    for &e in &heaped {
        let node_er = hl.node_of[&e];
        let arity = hl.ctx.get_enum(e).variants.len();
        for i in 0..arity {
            if let Some(payload) = hl.ctx.get_enum(e).variants[i].payload.get() {
                let rewritten = hl.rewrite_ty(payload);
                hl.ctx.set_variant_payload(node_er, i, rewritten);
            }
        }
    }

    // Phase 3: rewrite every function (signature + body).
    let mut functions = Map::default();
    for (fref, mut func) in program.functions {
        func.ret_type = hl.rewrite_ty(func.ret_type);
        for p in &mut func.parameters {
            p.ty = hl.rewrite_ty(p.ty);
        }
        func.body = hl.rewrite_expr(func.body);
        functions.insert(fref, func);
    }
    TypedProgram { functions }
}

struct HeapLower<'a, 'tcx> {
    ctx: &'a mut CompileCtx<'tcx>,
    node_of: Map<EnumRef<'tcx>, EnumRef<'tcx>>,
    unique_er: EnumRef<'tcx>,
    alloc_fn: FunRef<'tcx>,
    take_fn: FunRef<'tcx>,
}

impl<'tcx> HeapLower<'_, 'tcx> {
    /// `Unique<inner>`.
    fn unique_of(&mut self, inner: Ty<'tcx>) -> Ty<'tcx> {
        self.ctx.intern_app(self.unique_er, vec![inner], vec![])
    }

    /// `unique_take(handle) : node_ty` — move the node out of a handle.
    fn take(&self, handle: Expr<'tcx>, node_ty: Ty<'tcx>, range: Range) -> Expr<'tcx> {
        Expr {
            expr: Expression::Call {
                fn_name: self.take_fn,
                args: vec![handle],
            },
            ty: node_ty,
            kind: Kind::Owned,
            range,
        }
    }

    /// The inner type of a `Unique<X>` handle (`X`).
    fn strip_unique(&self, ty: Ty<'tcx>) -> Ty<'tcx> {
        match ty.kind() {
            TyKind::App(er, args, _) if *er == self.unique_er && args.len() == 1 => args[0],
            _ => internal_bug!("expected a Unique<_> handle type, found {ty:?}"),
        }
    }

    /// Was this (pre-rewrite) type a heaped enum?
    fn is_heaped(&self, ty: Ty<'tcx>) -> bool {
        match ty.kind() {
            TyKind::Enum(e) | TyKind::App(e, _, _) => self.node_of.contains_key(e),
            _ => false,
        }
    }

    /// The type homomorphism `R`: heaped `E<a>` → `Unique<E$Node<a>>`,
    /// recursing structurally everywhere else.
    fn rewrite_ty(&mut self, ty: Ty<'tcx>) -> Ty<'tcx> {
        match ty.kind() {
            TyKind::Enum(e) => {
                if let Some(&node) = self.node_of.get(e) {
                    let node_ty = self.ctx.enum_ty(node);
                    self.unique_of(node_ty)
                } else {
                    ty
                }
            }
            TyKind::App(e, args, regions) => {
                let args: Vec<Ty<'tcx>> = args.iter().map(|a| self.rewrite_ty(*a)).collect();
                let regions = regions.to_vec();
                if let Some(&node) = self.node_of.get(e) {
                    let node_app = self.ctx.intern_app(node, args, regions);
                    self.unique_of(node_app)
                } else {
                    self.ctx.intern_app(*e, args, regions)
                }
            }
            TyKind::Tuple(elems) => {
                let elems: Vec<Ty<'tcx>> = elems.iter().map(|e| self.rewrite_ty(*e)).collect();
                self.ctx.intern_tuple(elems)
            }
            TyKind::Ptr(inner) => {
                let inner = self.rewrite_ty(*inner);
                self.ctx.ptr_ty(inner)
            }
            TyKind::Ref(r, inner) => {
                let inner = self.rewrite_ty(*inner);
                self.ctx.ref_ty(*r, inner)
            }
            TyKind::RefMut(r, inner) => {
                let inner = self.rewrite_ty(*inner);
                self.ctx.ref_mut_ty(*r, inner)
            }
            TyKind::Region(inner, r) => {
                let inner = self.rewrite_ty(*inner);
                self.ctx.region_ty(inner, *r)
            }
            // Int, Bool, Unit, Param, Top, … carry no heaped sub-structure.
            _ => ty,
        }
    }

    fn rewrite_expr(&mut self, e: Expr<'tcx>) -> Expr<'tcx> {
        let ty = self.rewrite_ty(e.ty);
        let range = e.range;
        let kind = e.kind;
        let expr = self.rewrite_expression(e.expr, ty, range);
        Expr {
            expr,
            ty,
            kind,
            range,
        }
    }

    fn rewrite_expression(
        &mut self,
        expr: Expression<'tcx>,
        // The already-rewritten type of the enclosing `Expr` (the handle type
        // for a heaped constructor; the arm-result type for a match).
        ty: Ty<'tcx>,
        range: Range,
    ) -> Expression<'tcx> {
        match expr {
            Expression::Constructor {
                enum_ref,
                variant_idx,
                payload,
            } => {
                let payload = payload.map(|p| Box::new(self.rewrite_expr(*p)));
                if let Some(&node) = self.node_of.get(&enum_ref) {
                    // `E#C(p)` → `unique_alloc(E$Node#C(p))`.
                    let node_ty = self.strip_unique(ty);
                    let node_ctor = Expr {
                        expr: Expression::Constructor {
                            enum_ref: node,
                            variant_idx,
                            payload,
                        },
                        ty: node_ty,
                        kind: Kind::Owned,
                        range,
                    };
                    Expression::Call {
                        fn_name: self.alloc_fn,
                        args: vec![node_ctor],
                    }
                } else {
                    Expression::Constructor {
                        enum_ref,
                        variant_idx,
                        payload,
                    }
                }
            }

            Expression::Match { scrutinee, arms } => {
                let scrut_was_heaped = self.is_heaped(scrutinee.ty);
                let scrutinee = self.rewrite_expr(*scrutinee);
                let inspects_variant = arms
                    .iter()
                    .any(|a| matches!(a.pattern, MatchPattern::Variant { .. }));

                if scrut_was_heaped && inspects_variant {
                    self.lower_heaped_match(scrutinee, arms, ty, range)
                } else {
                    // No variant inspection (or non-heaped scrutinee): leave the
                    // match shape, just rewrite arm patterns/bodies.
                    let arms = arms
                        .into_iter()
                        .map(|arm| TypedMatchArm {
                            pattern: self.rewrite_pattern_types(arm.pattern),
                            body: self.rewrite_expr(arm.body),
                            range: arm.range,
                        })
                        .collect();
                    Expression::Match {
                        scrutinee: Box::new(scrutinee),
                        arms,
                    }
                }
            }

            // ── structural recursion for everything else ───────────────────
            Expression::If { cond, t, f } => Expression::If {
                cond: Box::new(self.rewrite_expr(*cond)),
                t: Box::new(self.rewrite_expr(*t)),
                f: Box::new(self.rewrite_expr(*f)),
            },
            Expression::While { cond, body } => Expression::While {
                cond: Box::new(self.rewrite_expr(*cond)),
                body: Box::new(self.rewrite_expr(*body)),
            },
            Expression::BinOp { left, op, right } => Expression::BinOp {
                left: Box::new(self.rewrite_expr(*left)),
                op,
                right: Box::new(self.rewrite_expr(*right)),
            },
            Expression::UnOp { op, right } => Expression::UnOp {
                op,
                right: Box::new(self.rewrite_expr(*right)),
            },
            Expression::Call { fn_name, args } => Expression::Call {
                fn_name,
                args: args.into_iter().map(|a| self.rewrite_expr(a)).collect(),
            },
            Expression::IntrinsicCall {
                fn_name,
                args,
                type_args,
            } => Expression::IntrinsicCall {
                fn_name,
                args: args.into_iter().map(|a| self.rewrite_expr(a)).collect(),
                type_args: type_args.into_iter().map(|t| self.rewrite_ty(t)).collect(),
            },
            Expression::MethodCall {
                class,
                method,
                self_ty,
                args,
            } => Expression::MethodCall {
                class,
                method,
                self_ty: self.rewrite_ty(self_ty),
                args: args.into_iter().map(|a| self.rewrite_expr(a)).collect(),
            },
            Expression::Borrow(inner, is_mut) => {
                Expression::Borrow(Box::new(self.rewrite_expr(*inner)), is_mut)
            }
            Expression::Deref(inner) => Expression::Deref(Box::new(self.rewrite_expr(*inner))),
            Expression::Block {
                statements,
                expr,
                drops,
            } => Expression::Block {
                statements: statements
                    .into_iter()
                    .map(|s| self.rewrite_stmt(s))
                    .collect(),
                expr: expr.map(|e| Box::new(self.rewrite_expr(*e))),
                drops,
            },
            Expression::Tuple(elems) => {
                Expression::Tuple(elems.into_iter().map(|e| self.rewrite_expr(e)).collect())
            }
            // leaves
            Expression::Var(_) | Expression::Int(_) | Expression::Bool(_) | Expression::Unit => {
                expr
            }
        }
    }

    /// `match s { … }` with `s : Unique<Node>` →
    /// `{ let n = unique_take(s); match n { … } }`.
    fn lower_heaped_match(
        &mut self,
        scrutinee: Expr<'tcx>,
        arms: Vec<TypedMatchArm<'tcx>>,
        result_ty: Ty<'tcx>,
        range: Range,
    ) -> Expression<'tcx> {
        let node_ty = self.strip_unique(scrutinee.ty);
        let node_er = match node_ty.kind() {
            TyKind::Enum(e) | TyKind::App(e, _, _) => *e,
            _ => internal_bug!("heaped match node type is not an enum: {node_ty:?}"),
        };

        let n = self
            .ctx
            .fresh_synthetic_var("node", range, VarDeclType::Declaration);
        let take_call = self.take(scrutinee, node_ty, range);
        let decl = Statement::Declaration {
            name: n,
            range,
            ty: node_ty,
            val: take_call,
        };

        let arms = arms
            .into_iter()
            .map(|arm| self.lower_node_arm(arm, node_ty, node_er, range))
            .collect();
        let inner_match = Expr {
            expr: Expression::Match {
                scrutinee: Box::new(Expr {
                    expr: Expression::Var(n),
                    ty: node_ty,
                    kind: Kind::Owned,
                    range,
                }),
                arms,
            },
            ty: result_ty,
            kind: Kind::Owned,
            range,
        };
        Expression::Block {
            statements: vec![decl],
            expr: Some(Box::new(inner_match)),
            drops: vec![],
        }
    }

    /// Retarget one arm of a (now node-valued) consuming match: variant
    /// patterns point at the node enum, every payload position is bound, and
    /// wildcard/catch-all arms bind the node so ownership drops it.
    fn lower_node_arm(
        &mut self,
        arm: TypedMatchArm<'tcx>,
        node_ty: Ty<'tcx>,
        node_er: EnumRef<'tcx>,
        range: Range,
    ) -> TypedMatchArm<'tcx> {
        let pattern = match arm.pattern {
            MatchPattern::Variant {
                enum_ref: _,
                variant_idx,
                payload,
                ..
            } => {
                let payload = payload.map(|(pty, sub)| {
                    let pty = self.rewrite_ty(pty);
                    (pty, Box::new(self.bind_all(*sub, pty, range)))
                });
                MatchPattern::Variant {
                    ty: node_ty,
                    enum_ref: node_er,
                    variant_idx,
                    payload,
                }
            }
            // A catch-all `_` binds the node so its heaped fields are dropped.
            MatchPattern::Wildcard => MatchPattern::Binding {
                var: self
                    .ctx
                    .fresh_synthetic_var("drop", range, VarDeclType::PatternBinding),
                ty: node_ty,
                range,
            },
            MatchPattern::Binding { .. } => internal_bug!(
                "a named binding arm on a heaped match rebinds the handle; \
                 unsupported in Step C.5 (use variant arms)"
            ),
            MatchPattern::Tuple { .. } | MatchPattern::IntLit(_) | MatchPattern::BoolLit(_) => {
                internal_bug!("tuple/literal pattern as a top-level arm on an enum scrutinee")
            }
        };
        TypedMatchArm {
            pattern,
            body: self.rewrite_expr(arm.body),
            range: arm.range,
        }
    }

    /// Rewrite a payload sub-pattern of a consuming match, binding **every**
    /// position (wildcards → fresh bindings) so ownership reclaims any field
    /// the arm does not move out. `expected` is the (already-rewritten)
    /// type at this position.
    fn bind_all(
        &mut self,
        pattern: MatchPattern<'tcx>,
        expected: Ty<'tcx>,
        range: Range,
    ) -> MatchPattern<'tcx> {
        match pattern {
            MatchPattern::Wildcard => MatchPattern::Binding {
                var: self
                    .ctx
                    .fresh_synthetic_var("drop", range, VarDeclType::PatternBinding),
                ty: expected,
                range,
            },
            MatchPattern::Binding { var, range, .. } => MatchPattern::Binding {
                var,
                ty: expected,
                range,
            },
            MatchPattern::Tuple { elems, .. } => {
                let elem_tys = match expected.kind() {
                    TyKind::Tuple(ts) => ts,
                    _ => internal_bug!("tuple pattern against non-tuple type {expected:?}"),
                };
                let elems = elems
                    .into_iter()
                    .zip(elem_tys.iter())
                    .map(|(e, t)| self.bind_all(e, *t, range))
                    .collect();
                MatchPattern::Tuple {
                    ty: expected,
                    elems,
                }
            }
            MatchPattern::Variant { .. } => internal_bug!(
                "nested variant pattern on a heaped field is unsupported in Step C.5 \
                 (it would need a recursive `unique_take`)"
            ),
            // literals match Copy scalar positions — left as-is.
            lit @ (MatchPattern::IntLit(_) | MatchPattern::BoolLit(_)) => lit,
        }
    }

    /// Rewrite only the type annotations carried by a pattern (the
    /// non-consuming / non-heaped path): no retargeting, no wildcard
    /// desugaring.
    fn rewrite_pattern_types(&mut self, pattern: MatchPattern<'tcx>) -> MatchPattern<'tcx> {
        match pattern {
            MatchPattern::Variant {
                ty,
                enum_ref,
                variant_idx,
                payload,
            } => {
                if self.node_of.contains_key(&enum_ref) {
                    internal_bug!(
                        "nested variant pattern on a heaped field is unsupported in Step C.5"
                    );
                }
                MatchPattern::Variant {
                    ty: self.rewrite_ty(ty),
                    enum_ref,
                    variant_idx,
                    payload: payload.map(|(pty, sub)| {
                        (
                            self.rewrite_ty(pty),
                            Box::new(self.rewrite_pattern_types(*sub)),
                        )
                    }),
                }
            }
            MatchPattern::Tuple { ty, elems } => MatchPattern::Tuple {
                ty: self.rewrite_ty(ty),
                elems: elems
                    .into_iter()
                    .map(|e| self.rewrite_pattern_types(e))
                    .collect(),
            },
            MatchPattern::Binding { var, ty, range } => MatchPattern::Binding {
                var,
                ty: self.rewrite_ty(ty),
                range,
            },
            leaf
            @ (MatchPattern::Wildcard | MatchPattern::IntLit(_) | MatchPattern::BoolLit(_)) => leaf,
        }
    }

    fn rewrite_stmt(&mut self, stmt: Statement<'tcx>) -> Statement<'tcx> {
        match stmt {
            Statement::Declaration {
                name,
                range,
                ty,
                val,
            } => Statement::Declaration {
                name,
                range,
                ty: self.rewrite_ty(ty),
                val: self.rewrite_expr(val),
            },
            Statement::LetTuple { elems, range, val } => Statement::LetTuple {
                elems: elems
                    .into_iter()
                    .map(|(v, t, m, r)| (v, self.rewrite_ty(t), m, r))
                    .collect(),
                range,
                val: self.rewrite_expr(val),
            },
            Statement::LetPattern {
                pattern,
                val,
                else_branch,
                range,
            } => {
                let heaped_variant = matches!(
                    &pattern,
                    MatchPattern::Variant { enum_ref, .. } if self.node_of.contains_key(enum_ref)
                );
                if heaped_variant {
                    // `let E#V(sub) = val else fb` →
                    // `let E$Node#V(sub) = unique_take(val) else unique_take(fb)`:
                    // both sides become the (non-heaped) node, so the ordinary
                    // refutable-let lowering applies directly. `bind_all` binds
                    // every payload position so unbound heaped fields are dropped.
                    let handle_ty = self.rewrite_ty(val.ty);
                    let node_ty = self.strip_unique(handle_ty);
                    let node_er = match node_ty.kind() {
                        TyKind::Enum(e) | TyKind::App(e, _, _) => *e,
                        _ => internal_bug!("heaped let-pattern node type is not an enum"),
                    };
                    let pattern = match pattern {
                        MatchPattern::Variant {
                            variant_idx,
                            payload,
                            ..
                        } => MatchPattern::Variant {
                            ty: node_ty,
                            enum_ref: node_er,
                            variant_idx,
                            payload: payload.map(|(pty, sub)| {
                                let pty = self.rewrite_ty(pty);
                                (pty, Box::new(self.bind_all(*sub, pty, range)))
                            }),
                        },
                        _ => internal_bug!("heaped let-pattern with a non-variant pattern"),
                    };
                    let val = self.rewrite_expr(val);
                    let val = self.take(val, node_ty, range);
                    let else_branch = self.rewrite_expr(else_branch);
                    let else_branch = self.take(else_branch, node_ty, range);
                    Statement::LetPattern {
                        pattern,
                        val,
                        else_branch,
                        range,
                    }
                } else {
                    Statement::LetPattern {
                        pattern: self.rewrite_pattern_types(pattern),
                        val: self.rewrite_expr(val),
                        else_branch: self.rewrite_expr(else_branch),
                        range,
                    }
                }
            }
            Statement::Assignment { name, range, val } => Statement::Assignment {
                name,
                range,
                val: self.rewrite_expr(val),
            },
            Statement::DerefAssign {
                reference,
                value,
                range,
            } => Statement::DerefAssign {
                reference: self.rewrite_expr(reference),
                value: self.rewrite_expr(value),
                range,
            },
            Statement::Expr(e) => Statement::Expr(self.rewrite_expr(e)),
        }
    }
}
