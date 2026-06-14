//! infer type of subexpression

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::Map;
use crate::compiler::structure::Range;
use crate::compiler::structure::RegionParam;
use crate::compiler::structure::TypeclassRef;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedFunction;
use crate::lang::intrinsics::INTRINSICS;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::types::EnumRef;
use crate::lang::types::Kind;
use crate::lang::types::Region;
use crate::lang::types::RegionVar;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::lang::types::TypeParamId;
use crate::passes::type_ast::TypeEnv;
use crate::passes::type_ast::check::check;
use crate::passes::type_ast::check::check_let_pattern;
use crate::passes::type_ast::check::type_check_match_arms;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::errors::TypeError;
use crate::passes::type_ast::generics::Subst;
use crate::passes::type_ast::generics::subst;
use crate::passes::type_ast::generics::unify;

pub(super) fn infer_function<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    func: &qhir::Function<'tcx>,
) -> Result<(FunRef<'tcx>, TypedFunction<'tcx>), TypeError<'tcx>> {
    // Open the function's region scope (depth 0): parameters live for the whole
    // call, so a borrow of a parameter never escapes the body (Step 8b).
    let fn_region = ctx.enter_region_scope();
    // The function's own `where 'a >= 's` clauses become the outlives
    // assumptions available while checking callee constraints at call sites.
    let prev_where = ctx.set_where_assumptions(func.where_constraints.clone());
    let prev_tc = ctx.set_type_assumptions(func.type_constraints.clone());
    let env: TypeEnv<'tcx> = func
        .parameters
        .iter()
        .map(|p| (p.name, (p.ty, Kind::Owned, p.is_mutable, fn_region)))
        .collect();

    // use check() so that bare tags in return position are resolved against the
    // declared return type.
    let body_result = check(ctx, &env, &func.body, func.ret_type);
    ctx.set_type_assumptions(prev_tc);
    ctx.set_where_assumptions(prev_where);
    ctx.exit_region_scope();
    let body = body_result.map_err(|e| TypeError {
        error: e,
        module: func.src_module,
    })?;

    // Function-return escape check (Calculus §6.3, the frame boundary): the
    // returned value's type may not name any *local* region — the function frame
    // or a block. Only *outer* regions (`'static`, lifetime parameters) outlive
    // the call, so a borrow of a by-value parameter or a local cannot be
    // returned; a borrow tied to a lifetime parameter (`&'a T`) can. This
    // tightens the per-block escape check, which alone would wrongly admit
    // `def f(x: Int): &Int := &x`.
    let mut ret_regions = Vec::new();
    body.ty.free_regions(&mut ret_regions);
    if ret_regions.iter().any(|&r| ctx.is_scope_region(r)) {
        return Err(TypeError {
            error: AstTypeError::RegionEscape { range: func.range },
            module: func.src_module,
        });
    }

    Ok((
        func.name,
        TypedFunction {
            name: func.name,
            range: func.range,
            type_params: func.type_params.clone(),
            region_params: func.region_params.clone(),
            where_constraints: func.where_constraints.clone(),
            type_constraints: func.type_constraints.clone(),
            parameters: func.parameters.to_vec(),
            ret_type: func.ret_type,
            body,
            src_module: func.src_module,
        },
    ))
}

/// Type-check an enum constructor expression. For a non-generic enum this
/// checks the payload against the declared type and yields the enum type. For a
/// generic enum it solves the type arguments, seeded from `expected` (when it
/// is an `App` of this enum, e.g. a `let x: Option<Int> = ...` context) and/or
/// inferred by unifying the declared payload against the actual payload, and
/// yields the corresponding `App` instantiation. A generic constructor whose
/// arguments cannot be determined (e.g. a bare nullary `Option#None` with no
/// annotation) is a `CannotInferTypeArguments` error.
pub(super) fn infer_constructor<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    env: &TypeEnv<'tcx>,
    expr: &qhir::Expr<'tcx>,
    enum_ref: EnumRef<'tcx>,
    variant_idx: usize,
    payload: Option<&qhir::Expr<'tcx>>,
    expected: Option<Ty<'tcx>>,
) -> Result<typed_hir::Expr<'tcx>, AstTypeError<'tcx>> {
    // `def` is arena-backed (`'tcx`) and does not borrow `ctx`, so we can keep
    // these around while calling `&mut ctx` methods below.
    let def = ctx.get_enum(enum_ref);
    let enum_name = def.name.clone();
    let variant_name = def.variants[variant_idx].name.clone();
    let declared_payload = def.variants[variant_idx].payload.get();
    let tp_ids: Vec<TypeParamId> = def.type_params.iter().map(|p| p.id).collect();
    let region_params = def.region_params.clone();
    // Region args the expected type pins (`let h: Holder<'a> = Holder#…`), used to
    // seed region params the payload doesn't constrain (e.g. a nullary variant).
    let expected_region_args: Option<Vec<Region>> = expected.and_then(|e| match e.kind() {
        TyKind::App(er, _, regs) if *er == enum_ref && regs.len() == region_params.len() => {
            Some(regs.to_vec())
        }
        _ => None,
    });

    // payload presence must match the variant's declaration
    if declared_payload.is_none() && payload.is_some() {
        return Err(AstTypeError::ConstructorPayloadMismatch {
            enum_name,
            variant: variant_name,
            expected_payload: false,
            range: payload.map(|p| p.range).unwrap_or(expr.range),
        });
    }
    if declared_payload.is_some() && payload.is_none() {
        return Err(AstTypeError::ConstructorPayloadMismatch {
            enum_name,
            variant: variant_name,
            expected_payload: true,
            range: expr.range,
        });
    }

    let make = |payload, ty| {
        Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Constructor {
                enum_ref,
                variant_idx,
                payload,
            },
            range: expr.range,
            ty,
            kind: Kind::Owned,
        })
    };

    // Non-generic enum: check payload against the declared type directly.
    if tp_ids.is_empty() {
        let typed_payload = match (declared_payload, payload) {
            (Some(decl), Some(p)) => Some(Box::new(check(ctx, env, p, decl)?)),
            _ => None,
        };
        // No region params => a plain `Enum` type. With region params, infer the
        // region args from the payload so the value's borrow lifetimes are
        // tracked in its type and the escape check can see them.
        if region_params.is_empty() {
            return make(typed_payload, ctx.enum_ty(enum_ref));
        }
        let actual_payload = typed_payload.as_ref().map(|p| p.ty);
        let region_args = ctx.infer_ctor_region_args(
            &region_params,
            declared_payload,
            actual_payload,
            expected_region_args.as_deref(),
        );
        let ty = ctx.intern_app(enum_ref, Vec::new(), region_args);
        return make(typed_payload, ty);
    }

    // Generic enum: solve the type arguments.
    let mut mapping: Subst<'tcx> = Map::new();
    if let Some(exp) = expected
        && let TyKind::App(exp_er, exp_args, _) = exp.kind()
        && *exp_er == enum_ref
        && exp_args.len() == tp_ids.len()
    {
        for (id, arg) in tp_ids.iter().zip(*exp_args) {
            mapping.insert(*id, *arg);
        }
    }

    let typed_payload = match (declared_payload, payload) {
        (Some(decl), Some(p)) => {
            let decl = subst(ctx, decl, &mapping);
            if decl.has_param() {
                // payload type still parametric: infer the argument and unify.
                let tp = infer(ctx, env, p)?;
                // A unification failure here is a payload *type* mismatch (the
                // argument's type doesn't fit the variant's declared payload), not
                // a missing payload — report it as such.
                unify(decl, tp.ty, &mut mapping).map_err(|_| AstTypeError::TypeError {
                    message: format!(
                        "constructor '{enum_name}#{variant_name}' payload has the wrong type"
                    ),
                    expected: decl,
                    found: tp.ty,
                    range: p.range,
                })?;
                Some(Box::new(tp))
            } else {
                Some(Box::new(check(ctx, env, p, decl)?))
            }
        }
        _ => None,
    };

    let mut args = Vec::with_capacity(tp_ids.len());
    for id in &tp_ids {
        match mapping.get(id) {
            Some(&t) => args.push(t),
            None => {
                return Err(AstTypeError::CannotInferTypeArguments {
                    enum_name,
                    range: expr.range,
                });
            }
        }
    }
    // Region args are inferred from the payload borrows, in declaration order.
    let actual_payload = typed_payload.as_ref().map(|p| p.ty);
    let region_args = ctx.infer_ctor_region_args(
        &region_params,
        declared_payload,
        actual_payload,
        expected_region_args.as_deref(),
    );
    let ty = ctx.intern_app(enum_ref, args, region_args);
    make(typed_payload, ty)
}

/// Borrow escape check (Calculus §6.3): a block must not yield a value whose
/// *type* names a region introduced at or inside the block — such a region
/// would dangle once the block closes. `block_depth` is the block's nesting
/// depth; any free region of the result type at that depth or deeper escapes.
/// Regions live on the type, so this reads `freeRegions(ty)`, not the kind.
pub(super) fn escape_check<'tcx>(
    ctx: &CompileCtx<'tcx>,
    ty: Ty<'tcx>,
    block_depth: usize,
    range: Range,
) -> Result<(), AstTypeError<'tcx>> {
    let mut regions = Vec::new();
    ty.free_regions(&mut regions);
    if regions
        .into_iter()
        .any(|r| ctx.region_depth(r) >= block_depth)
    {
        return Err(AstTypeError::RegionEscape { range });
    }
    Ok(())
}

/// Result type of a branch join (`if`/`match`): the common structural type
/// (`structural`, which is region-blind-equal to every branch) with all its
/// regions stamped to the **meet** (shortest-lived GLB) of the branches'
/// regions — the same per-argument `meet` the call path applies (§6.3, item 8).
///
/// This is the soundness fix for branch joins: taking one branch's type
/// verbatim (the first arm, or the region-blind `expected`) drops the regions
/// of the *other* branches, so a borrow of a local escaping through a
/// non-chosen branch slipped past the enclosing escape check. Stamping the join
/// with the meet makes the result outlive *no* branch, so an escape in **any**
/// branch surfaces in the result type and is caught (Calculus §6.3, §6.9 "all
/// arms agree"). Diverging branches (kind `Never`) yield no value, so they do
/// not constrain the region.
pub(super) fn join_region_ty<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    structural: Ty<'tcx>,
    branches: &[(Ty<'tcx>, Kind)],
) -> Ty<'tcx> {
    let mut regions = Vec::new();
    for (ty, kind) in branches {
        if *kind != Kind::Never {
            ty.free_regions(&mut regions);
        }
    }
    if regions.is_empty() {
        return structural; // no borrows in play — nothing to constrain
    }
    // Meet under the enclosing function's `where` assumptions, so a branch that
    // returns a longer-lived borrow coercible to the result lifetime (e.g.
    // `&'a` where `'a >= 'b`) is admitted; callers must satisfy the `where`.
    let assumptions = ctx.where_assumptions().to_vec();
    let meet = ctx.region_meet(&regions, &assumptions);
    ctx.region_fill(structural, meet)
}

/// Call-site region inference + `where`-clause checking (Calculus §1.1, §8.10).
///
/// Infers the call's region substitution (each callee region parameter and the
/// elided region mapped to the meet of the actual argument regions, via
/// [`CompileCtx::infer_region_subst`]), then checks every callee `where 'a >=
/// 's` constraint under it — using the *enclosing* function's own clauses as
/// assumptions, so a generic caller can discharge a callee constraint. Returns
/// the substitution to stamp onto the return type.
fn instantiate_call_regions<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    fun_sig: &FunSig<'tcx>,
    decls: &[Ty<'tcx>],
    actuals: &[Ty<'tcx>],
    range: Range,
) -> Result<Map<RegionVar, Region>, AstTypeError<'tcx>> {
    let region_subst = ctx.infer_region_subst(decls, actuals, &fun_sig.region_params);
    let assumptions = ctx.where_assumptions().to_vec();
    for c in &fun_sig.where_constraints {
        let longer = apply_region(c.longer, &region_subst);
        let shorter = apply_region(c.shorter, &region_subst);
        if !ctx.outlives(longer, shorter, &assumptions) {
            return Err(AstTypeError::RegionConstraintUnsatisfied {
                longer: region_param_name(&fun_sig.region_params, c.longer),
                shorter: region_param_name(&fun_sig.region_params, c.shorter),
                range,
            });
        }
    }
    Ok(region_subst)
}

/// Apply a region substitution to a single region (the constraint endpoints).
fn apply_region(r: Region, subst: &Map<RegionVar, Region>) -> Region {
    match r {
        Region::Var(rv) => subst.get(&rv).copied().unwrap_or(r),
        Region::Static => Region::Static,
    }
}

/// The source-level name of a callee region (`'a`) for diagnostics.
fn region_param_name(params: &[RegionParam], r: Region) -> String {
    match r {
        Region::Static => "static".to_string(),
        Region::Var(rv) => params
            .iter()
            .find(|p| p.region == rv)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "_".to_string()),
    }
}

pub(super) fn infer_statement<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    env: &mut TypeEnv<'tcx>,
    stmt: &qhir::Statement<'tcx>,
) -> Result<typed_hir::Statement<'tcx>, AstTypeError<'tcx>> {
    match stmt {
        qhir::Statement::Declaration {
            name,
            ty: annotation,
            is_mutable,
            val,
            range,
        } => {
            // When an annotation is present use check() so bare tags are resolved.
            let (val_expr, ty) = match annotation {
                Some(declared_ty) => {
                    let checked = check(ctx, env, val, *declared_ty)?;
                    (checked, *declared_ty)
                }
                None => {
                    let inferred = infer(ctx, env, val)?;
                    let ty = inferred.ty;
                    (inferred, ty)
                }
            };
            // The binding lives in the current lexical scope; it carries the
            // value's kind so a `let r = &local` remembers it is a borrow (its
            // region drives the escape check when `r` is later yielded).
            let home = ctx.current_scope_region();
            env.insert(*name, (ty, val_expr.kind, *is_mutable, home));
            Ok(typed_hir::Statement::Declaration {
                name: *name,
                range: *range,
                ty,
                val: val_expr,
            })
        }
        qhir::Statement::Assignment { name, val, range } => {
            let (var_ty, _kind, is_mutable, home) =
                env.get(name)
                    .copied()
                    .ok_or_else(|| AstTypeError::UnboundVariable {
                        name: ctx.uniq_variable_name(name),
                        range: *range,
                    })?;
            if !is_mutable {
                return Err(AstTypeError::ImmutableAssignment {
                    name: ctx.uniq_variable_name(name),
                    range: *range,
                });
            }
            // Use check() so that bare tags are resolved against the variable's
            // known type (e.g. `result = #gt` when result: #gt | #lt | #eq).
            let val_expr = check(ctx, env, val, var_ty)?;
            // Reseat-escape (Calculus §6.3, item 11): the new value must live at
            // least as long as the variable it is assigned into — re-pointing an
            // *outer* reference at an *inner*-scope borrow would dangle, and no
            // scope *result* crosses a boundary to trigger the escape check, so it
            // is caught here: every free region of the RHS must outlive the
            // variable's home region.
            let mut regions = Vec::new();
            val_expr.ty.free_regions(&mut regions);
            if regions.iter().any(|&r| !ctx.outlives(r, home, &[])) {
                return Err(AstTypeError::RegionEscape { range: *range });
            }
            Ok(typed_hir::Statement::Assignment {
                name: *name,
                range: *range,
                val: val_expr,
            })
        }
        qhir::Statement::DerefAssign {
            reference,
            value,
            range,
        } => {
            // `*reference = value` (Calculus §3.2): write-through requires a
            // *mutable* reference; a shared `&T` deref is a read-only place.
            let ref_expr = infer(ctx, env, reference)?;
            let (pointee_ty, ref_region) = match ref_expr.ty.kind() {
                TyKind::RefMut(r, t) => (*t, *r),
                TyKind::Ref(..) => {
                    return Err(AstTypeError::TypeError {
                        message: format!(
                            "cannot write through a shared reference of type {}: \
                             write-through requires `&mut`",
                            ctx.display_ty(ref_expr.ty)
                        ),
                        expected: ref_expr.ty,
                        found: ref_expr.ty,
                        range: *range,
                    });
                }
                _ => {
                    return Err(AstTypeError::DerefOfNonReference {
                        ty: ref_expr.ty,
                        range: *range,
                    });
                }
            };
            // RHS checked against the pointee type.
            let val_expr = check(ctx, env, value, pointee_ty)?;
            // Write-through escape (Calculus §6.3, item 11): the stored value must
            // outlive the region the reference points into, else it would dangle.
            let mut regions = Vec::new();
            val_expr.ty.free_regions(&mut regions);
            if regions.iter().any(|&r| !ctx.outlives(r, ref_region, &[])) {
                return Err(AstTypeError::RegionEscape { range: *range });
            }
            Ok(typed_hir::Statement::DerefAssign {
                reference: ref_expr,
                value: val_expr,
                range: *range,
            })
        }
        qhir::Statement::LetTuple {
            elems,
            ty: annotation,
            val,
            range,
        } => {
            // Infer or check the RHS; it must be a tuple type of matching arity.
            let val_expr = match annotation {
                Some(declared_ty) => check(ctx, env, val, *declared_ty)?,
                None => infer(ctx, env, val)?,
            };
            let elem_tys = match val_expr.ty.kind() {
                TyKind::Tuple(tys) => *tys,
                _ => {
                    return Err(AstTypeError::PatternTypeMismatch {
                        message: format!(
                            "let-tuple pattern used against non-tuple type {}",
                            ctx.display_ty(val_expr.ty)
                        ),
                        range: *range,
                    });
                }
            };
            if elem_tys.len() != elems.len() {
                return Err(AstTypeError::PatternArityMismatch {
                    expected: elem_tys.len(),
                    found: elems.len(),
                    range: *range,
                });
            }
            // Register each element in the environment and collect typed elems.
            let typed_elems: Vec<(
                crate::compiler::structure::UniqVar,
                Ty,
                bool,
                crate::compiler::structure::Range,
            )> = elems
                .iter()
                .zip(elem_tys.iter())
                .map(|((name, is_mutable, elem_range), &ty)| {
                    let home = ctx.current_scope_region();
                    env.insert(*name, (ty, Kind::Owned, *is_mutable, home));
                    (*name, ty, *is_mutable, *elem_range)
                })
                .collect();
            Ok(typed_hir::Statement::LetTuple {
                elems: typed_elems,
                range: *range,
                val: val_expr,
            })
        }

        qhir::Statement::LetPattern {
            pattern,
            ty: annotation,
            val,
            else_branch,
            range,
        } => {
            // 1. Type-check (or infer) the main value expression.
            let val_expr = match annotation {
                Some(declared_ty) => check(ctx, env, val, *declared_ty)?,
                None => infer(ctx, env, val)?,
            };
            let scrutinee_ty = val_expr.ty;

            // 2. Resolve the pattern against scrutinee_ty; collect bindings.
            let (typed_pattern, bindings) = check_let_pattern(ctx, pattern, scrutinee_ty, *range)?;

            // 3. Extract the expected variant_idx from the typed pattern for the else
            //    check.
            let expected_variant_idx = match &typed_pattern {
                typed_hir::MatchPattern::Variant { variant_idx, .. } => *variant_idx,
                _ => unreachable!("check_let_pattern always returns a Variant pattern"),
            };

            // 4. Type-check the else branch against the same type as `val`.
            let else_expr = check(ctx, env, else_branch, scrutinee_ty)?;

            // 5. Verify the else expression is a constructor of the same variant so that
            //    destructuring the fallback is always guaranteed to succeed.
            match &else_expr.expr {
                typed_hir::Expression::Constructor {
                    variant_idx: else_vi,
                    ..
                } if *else_vi == expected_variant_idx => {}
                _ => {
                    return Err(AstTypeError::LetPatternElseNotIrrefutable { range: *range });
                }
            }

            // 6. Register all bindings in the environment.
            let home = ctx.current_scope_region();
            for (var, ty, _range) in &bindings {
                env.insert(*var, (*ty, Kind::Owned, false, home)); // all let-pattern bindings are immutable
            }

            Ok(typed_hir::Statement::LetPattern {
                pattern: typed_pattern,
                val: val_expr,
                else_branch: else_expr,
                range: *range,
            })
        }

        qhir::Statement::Expr(e) => {
            let e_expr = infer(ctx, env, e)?;
            Ok(typed_hir::Statement::Expr(e_expr))
        }
    }
}

/// Check that a `where T : class` constraint is satisfied for the concrete type
/// `ty` at a call site. A concrete type needs a registered instance;
/// a still-abstract type parameter must itself be constrained in the caller
/// (the bound propagates upward).
fn check_type_constraint<'tcx>(
    ctx: &CompileCtx<'tcx>,
    class: TypeclassRef,
    ty: Ty<'tcx>,
    range: Range,
) -> Result<(), AstTypeError<'tcx>> {
    if let TyKind::Param(pid) = ty.kind() {
        let licensed = ctx
            .type_assumptions()
            .iter()
            .any(|tc| tc.param == *pid && ctx.class_satisfies(tc.class, class));
        return if licensed {
            Ok(())
        } else {
            Err(AstTypeError::TypeclassNoInstance {
                class: ctx.get_typeclass(class).name.clone(),
                ty,
                range,
            })
        };
    }
    let ok = ctx
        .type_head(ty)
        .map(|head| ctx.lookup_instance(class, head).is_some())
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(AstTypeError::TypeclassNoInstance {
            class: ctx.get_typeclass(class).name.clone(),
            ty,
            range,
        })
    }
}

/// Type-check a raw-pointer op (`__ptr_read` / `__ptr_write` / `__ptr_cast`,
/// Memory Step A). These are generic, so they bypass the monomorphic
/// `INTRINSICS` table: the element type is recovered from the `Ptr<T>` argument
/// (`read`/`write`) or the expected type (`cast`'s target). `expected` is the
/// checking-mode context, required by `__ptr_cast`.
pub(super) fn infer_ptr_op<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    env: &TypeEnv<'tcx>,
    op: Intrinsic,
    args: &[qhir::Expr<'tcx>],
    expected: Option<Ty<'tcx>>,
    range: Range,
) -> Result<typed_hir::Expr<'tcx>, AstTypeError<'tcx>> {
    let arity_err = |found: usize, want: usize| AstTypeError::PtrOpError {
        message: format!("{op} expects {want} argument(s) but found {found}"),
        range,
    };
    let mk = |args, ty| typed_hir::Expr {
        expr: typed_hir::Expression::IntrinsicCall { fn_name: op, args },
        range,
        ty,
        kind: Kind::Owned,
    };
    match op {
        Intrinsic::PtrRead => {
            if args.len() != 1 {
                return Err(arity_err(args.len(), 1));
            }
            let p = infer(ctx, env, &args[0])?;
            let TyKind::Ptr(elem) = p.ty.kind() else {
                return Err(AstTypeError::PtrOpError {
                    message: format!(
                        "__ptr_read expects a `Ptr<T>`, found {}",
                        ctx.display_ty(p.ty)
                    ),
                    range,
                });
            };
            let elem = *elem;
            Ok(mk(vec![p], elem))
        }
        Intrinsic::PtrWrite => {
            if args.len() != 2 {
                return Err(arity_err(args.len(), 2));
            }
            let p = infer(ctx, env, &args[0])?;
            let TyKind::Ptr(elem) = p.ty.kind() else {
                return Err(AstTypeError::PtrOpError {
                    message: format!(
                        "__ptr_write expects a `Ptr<T>`, found {}",
                        ctx.display_ty(p.ty)
                    ),
                    range,
                });
            };
            let elem = *elem;
            let v = check(ctx, env, &args[1], elem)?;
            Ok(mk(vec![p, v], ctx.types.unit))
        }
        Intrinsic::PtrCast => {
            if args.len() != 1 {
                return Err(arity_err(args.len(), 1));
            }
            let p = infer(ctx, env, &args[0])?;
            if !matches!(p.ty.kind(), TyKind::Ptr(_)) {
                return Err(AstTypeError::PtrOpError {
                    message: format!(
                        "__ptr_cast expects a `Ptr<A>`, found {}",
                        ctx.display_ty(p.ty)
                    ),
                    range,
                });
            }
            let target = match expected {
                Some(t) if matches!(t.kind(), TyKind::Ptr(_)) => t,
                Some(t) => {
                    return Err(AstTypeError::PtrOpError {
                        message: format!(
                            "__ptr_cast must produce a `Ptr<B>`, but the context expects {}",
                            ctx.display_ty(t)
                        ),
                        range,
                    });
                }
                None => {
                    return Err(AstTypeError::PtrOpError {
                        message:
                            "__ptr_cast needs a `Ptr<B>` type annotation to know its target type"
                                .to_string(),
                        range,
                    });
                }
            };
            Ok(mk(vec![p], target))
        }
        _ => unreachable!("infer_ptr_op called for non-ptr-op intrinsic {op}"),
    }
}

/// Resolve a typeclass method call. Infers the arguments, solves the
/// class's type parameter (the receiver) by unifying the method's declared
/// signature against the actual argument types, then:
/// - **concrete receiver** -> look up the instance and rewrite to a direct
///   `Call` of the impl method;
/// - **type-parameter receiver** -> dispatch is deferred to monomorphisation
///   (emitted as `typed_hir::MethodCall`), provided a `where T : C` bound
///   licenses it
fn infer_method_call<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    env: &TypeEnv<'tcx>,
    expr: &qhir::Expr<'tcx>,
    class: TypeclassRef,
    method: &str,
    args: &[qhir::Expr<'tcx>],
) -> Result<typed_hir::Expr<'tcx>, AstTypeError<'tcx>> {
    let arg_exprs: Vec<typed_hir::Expr<'tcx>> = args
        .iter()
        .map(|a| infer(ctx, env, a))
        .collect::<Result<_, _>>()?;

    let mdef = ctx.get_typeclass(class).methods[method].clone();
    let class_param = ctx.get_typeclass(class).param;
    let class_name = ctx.get_typeclass(class).name.clone();

    // Solve the class parameter (the receiver type) from the arguments.
    let mut mapping: Subst<'tcx> = Map::new();
    if mdef.param_tys.len() == arg_exprs.len() {
        for (decl, a) in mdef.param_tys.iter().zip(&arg_exprs) {
            let _ = unify(*decl, a.ty, &mut mapping);
        }
    }
    let receiver =
        mapping
            .get(&class_param)
            .copied()
            .ok_or_else(|| AstTypeError::TypeclassCannotResolve {
                method: method.to_string(),
                range: expr.range,
            })?;

    let result_ty = subst(ctx, mdef.ret_ty, &mapping);

    // Type-parameter receiver: licensed only by a `where T : C` bound (or a
    // bound on a subclass of `C`). If licensed, defer dispatch to mono; else
    // reject. `mono` resolves the emitted `MethodCall` once the parameter is
    // concrete.
    if let TyKind::Param(pid) = receiver.kind() {
        let licensed = ctx
            .type_assumptions()
            .iter()
            .any(|tc| tc.param == *pid && ctx.class_satisfies(tc.class, class));
        if !licensed {
            return Err(AstTypeError::TypeclassNeedsConstraint {
                method: method.to_string(),
                range: expr.range,
            });
        }
        return Ok(typed_hir::Expr {
            expr: typed_hir::Expression::MethodCall {
                class,
                method: method.to_string(),
                self_ty: receiver,
                args: arg_exprs,
            },
            ty: result_ty,
            kind: Kind::Owned,
            range: expr.range,
        });
    }

    let head = ctx
        .type_head(receiver)
        .ok_or_else(|| AstTypeError::TypeclassNoInstance {
            class: class_name.clone(),
            ty: receiver,
            range: expr.range,
        })?;
    let impl_fn = ctx
        .lookup_instance(class, head)
        .and_then(|idef| idef.methods.get(method).copied())
        .ok_or(AstTypeError::TypeclassNoInstance {
            class: class_name,
            ty: receiver,
            range: expr.range,
        })?;

    Ok(typed_hir::Expr {
        expr: typed_hir::Expression::Call {
            fn_name: impl_fn,
            args: arg_exprs,
        },
        ty: result_ty,
        kind: Kind::Owned,
        range: expr.range,
    })
}

pub(super) fn infer<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    env: &TypeEnv<'tcx>,
    expr: &qhir::Expr<'tcx>,
) -> Result<typed_hir::Expr<'tcx>, AstTypeError<'tcx>> {
    match &expr.expr {
        qhir::Expression::Int(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Int(*x),
            range: expr.range,
            ty: ctx.types.int,
            kind: Kind::Owned,
        }),
        qhir::Expression::Bool(x) => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Bool(*x),
            range: expr.range,
            ty: ctx.types.bool,
            kind: Kind::Owned,
        }),
        qhir::Expression::Unit => Ok(typed_hir::Expr {
            expr: typed_hir::Expression::Unit,
            range: expr.range,
            ty: ctx.types.unit,
            kind: Kind::Owned,
        }),
        // `&e` / `&mut e` (Calculus §3.2): a shared (`K-Borrow`) or exclusive
        // (`K-BorrowMut`) borrow, of type `&'r T` / `&'r mut T`. The *type*'s
        // region stays the shared elided region (so `&e` matches a `&T`
        // annotation), while the *kind* carries the borrow's real scope — the
        // referent's home region for a borrowed variable, or the current scope
        // for a temporary — so the escape check (Step 8b) can tell a local's
        // borrow from a parameter's. (Mutable-borrow exclusivity is enforced
        // separately, in the ownership pass.)
        qhir::Expression::Borrow(inner, mutable) => {
            // `&mut x` of a *variable* requires that `x` be a mutable binding
            // (a `let mut`/`mut` parameter). Borrowing a temporary is always
            // fine — the borrower owns it exclusively.
            if *mutable
                && let qhir::Expression::Var(v) = &inner.expr
                && !env.get(v).map(|b| b.2).unwrap_or(false)
            {
                return Err(AstTypeError::MutBorrowOfImmutable {
                    name: ctx.uniq_variable_name(v),
                    range: expr.range,
                });
            }
            let inner_expr = infer(ctx, env, inner)?;
            // the borrow's region is the referent's storage region — a borrowed
            // variable's home region, or the current scope for a temporary — and
            // it lives on the *type* (`&'r T`). The kind records only capability.
            let region = match &inner.expr {
                qhir::Expression::Var(v) => env
                    .get(v)
                    .map(|b| b.3)
                    .unwrap_or(ctx.current_scope_region()),
                _ => ctx.current_scope_region(),
            };
            let ty = if *mutable {
                ctx.ref_mut_ty(region, inner_expr.ty)
            } else {
                ctx.ref_ty(region, inner_expr.ty)
            };
            let kind = if *mutable {
                Kind::BorrowedMut
            } else {
                Kind::Borrowed
            };
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Borrow(Box::new(inner_expr), *mutable),
                range: expr.range,
                ty,
                kind,
            })
        }
        // `*e` (dereference): reading through a reference yields the pointee.
        // `&'r T` and `&'r mut T` both deref to `T`. Borrows are erased at
        // runtime, so this lowers transparently; the result is an owned value.
        qhir::Expression::Deref(inner) => {
            let inner_expr = infer(ctx, env, inner)?;
            let pointee = match inner_expr.ty.kind() {
                TyKind::Ref(_, t) | TyKind::RefMut(_, t) => *t,
                _ => {
                    return Err(AstTypeError::DerefOfNonReference {
                        ty: inner_expr.ty,
                        range: expr.range,
                    });
                }
            };
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Deref(Box::new(inner_expr)),
                range: expr.range,
                ty: pointee,
                kind: Kind::Owned,
            })
        }
        qhir::Expression::Var(x) => {
            let (ty, kind, _, _) =
                env.get(x)
                    .copied()
                    .ok_or_else(|| AstTypeError::UnboundVariable {
                        name: ctx.uniq_variable_name(x),
                        range: expr.range,
                    })?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Var(*x),
                range: expr.range,
                ty,
                kind,
            })
        }
        qhir::Expression::Constructor {
            enum_ref,
            variant_idx,
            payload,
        } => infer_constructor(
            ctx,
            env,
            expr,
            *enum_ref,
            *variant_idx,
            payload.as_deref(),
            None,
        ),

        qhir::Expression::Tag { variant, .. } => Err(AstTypeError::TagWithoutContext {
            variant: variant.clone(),
            range: expr.range,
        }),

        qhir::Expression::Tuple(elems) => {
            let typed_elems = elems
                .iter()
                .map(|e| infer(ctx, env, e))
                .collect::<Result<Vec<_>, _>>()?;
            let elem_tys: Vec<Ty<'tcx>> = typed_elems.iter().map(|e| e.ty).collect();
            let ty = ctx.intern_tuple(elem_tys);
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Tuple(typed_elems),
                range: expr.range,
                ty,
                kind: Kind::Owned,
            })
        }

        qhir::Expression::BinOp { left, op, right } => {
            // When one side is a bare Tag, infer the other side first, then
            // use check() to resolve the tag against the inferred type.
            let (left_expr, right_expr) = match (&left.expr, &right.expr) {
                (_, qhir::Expression::Tag { .. }) => {
                    let l = infer(ctx, env, left)?;
                    let r = check(ctx, env, right, l.ty)?;
                    (l, r)
                }
                (qhir::Expression::Tag { .. }, _) => {
                    let r = infer(ctx, env, right)?;
                    let l = check(ctx, env, left, r.ty)?;
                    (l, r)
                }
                _ => (infer(ctx, env, left)?, infer(ctx, env, right)?),
            };

            let ty = op
                .accepts_types(&ctx.types, left_expr.ty, right_expr.ty)
                .map_err(|expected_ty| AstTypeError::TypeError {
                    message: format!(
                        "operator '{:?}' does not accept types {} and {}",
                        op, left_expr.ty, right_expr.ty
                    ),
                    expected: expected_ty,
                    found: left_expr.ty,
                    range: expr.range,
                })?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::BinOp {
                    left: Box::new(left_expr),
                    op: *op,
                    right: Box::new(right_expr),
                },
                range: expr.range,
                ty,
                kind: Kind::Owned,
            })
        }
        qhir::Expression::UnOp { op, right } => {
            let right_expr = infer(ctx, env, right)?;

            let ty = op
                .accepts_type(&ctx.types, right_expr.ty)
                .map_err(|expected_ty| AstTypeError::TypeError {
                    message: format!("operator '{:?}' does not accept type {}", op, right_expr.ty),
                    expected: expected_ty,
                    found: right_expr.ty,
                    range: expr.range,
                })?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::UnOp {
                    op: *op,
                    right: Box::new(right_expr),
                },
                range: expr.range,
                ty,
                kind: Kind::Owned,
            })
        }
        qhir::Expression::If { cond, t, f } => {
            let cond_expr = infer(ctx, env, cond)?;
            if cond_expr.ty != ctx.types.bool {
                return Err(AstTypeError::TypeError {
                    message: format!("condition of 'if' must be Bool, found {}", cond_expr.ty),
                    expected: ctx.types.bool,
                    found: cond_expr.ty,
                    range: cond.range,
                });
            }

            let t_expr = infer(ctx, env, t)?;

            let (f_expr, ty) = match f {
                Some(f) => {
                    let f_expr = infer(ctx, env, f)?;
                    // A diverging branch (kind `Never`) does not constrain the
                    // result type, so we just take from the other branch.
                    let ty = match (t_expr.kind, f_expr.kind) {
                        (Kind::Never, _) => f_expr.ty,
                        (_, Kind::Never) => t_expr.ty,
                        _ => {
                            if t_expr.ty != f_expr.ty {
                                return Err(AstTypeError::TypeError {
                                    message: format!(
                                        "branches of 'if' expression must have the same type, found {} and {}",
                                        t_expr.ty, f_expr.ty
                                    ),
                                    expected: t_expr.ty,
                                    found: f_expr.ty,
                                    range: expr.range,
                                });
                            }
                            t_expr.ty
                        }
                    };
                    (f_expr, ty)
                }
                None => {
                    if t_expr.ty != ctx.types.unit {
                        return Err(AstTypeError::TypeError {
                            message: format!(
                                "'if' without 'else' must have type Unit, but then-branch has type {}",
                                t_expr.ty
                            ),
                            expected: ctx.types.unit,
                            found: t_expr.ty,
                            range: t.range,
                        });
                    }
                    let f_expr = typed_hir::Expr {
                        expr: typed_hir::Expression::Unit,
                        range: expr.range,
                        ty: ctx.types.unit,
                        kind: Kind::Owned,
                    };
                    (f_expr, ctx.types.unit)
                }
            };

            let kind = t_expr.kind.join(f_expr.kind);
            // Stamp the join with the meet of the branch regions so a borrow
            // escaping through *either* branch is caught (see `join_region_ty`).
            let ty = join_region_ty(
                ctx,
                ty,
                &[(t_expr.ty, t_expr.kind), (f_expr.ty, f_expr.kind)],
            );
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::If {
                    cond: Box::new(cond_expr),
                    t: Box::new(t_expr),
                    f: Box::new(f_expr),
                },
                range: expr.range,
                ty,
                kind,
            })
        }
        qhir::Expression::While { cond, body } => {
            let cond_expr = infer(ctx, env, cond)?;
            if cond_expr.ty != ctx.types.bool {
                return Err(AstTypeError::TypeError {
                    message: format!("condition of 'while' must be Bool, found {}", cond_expr.ty),
                    expected: ctx.types.bool,
                    found: cond_expr.ty,
                    range: cond.range,
                });
            }

            let body_expr = infer(ctx, env, body)?;

            // A `while true do …` loop has no exit (the language has no
            // `break`), so it diverges: its kind is `Never`.
            let kind = if matches!(cond_expr.expr, typed_hir::Expression::Bool(true)) {
                Kind::Never
            } else {
                Kind::Owned
            };
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::While {
                    cond: Box::new(cond_expr),
                    body: Box::new(body_expr),
                },
                range: expr.range,
                ty: ctx.types.unit,
                kind,
            })
        }
        qhir::Expression::MethodCall {
            class,
            method,
            args,
        } => infer_method_call(ctx, env, expr, *class, method, args),

        qhir::Expression::Call { fn_name, args } => {
            let fun_sig = ctx.fun_sig(fn_name);
            let raw_expected: Vec<Ty<'tcx>> = fun_sig.args.iter().map(|p| p.1).collect();
            let raw_ret = fun_sig.ret_ty;
            // Region inference: a callee's reference regions are inferred at the
            // call site, so arguments match region-*blind* (`f<'r>(x: &'r T)` is
            // callable with any borrow); the *result* region is the `meet` of the
            // argument regions (Calculus §6.3, item 8), stamped onto the return
            // type once the arguments are typed.
            let expected_tys: Vec<Ty<'tcx>> =
                raw_expected.iter().map(|t| ctx.region_erase(*t)).collect();

            if args.len() != expected_tys.len() {
                // Arity mismatch: infer args just for the error message.
                let arg_exprs = args
                    .iter()
                    .map(|arg| infer(ctx, env, arg))
                    .collect::<Result<Vec<_>, _>>()?;
                let arg_tys: Vec<Ty<'tcx>> = arg_exprs.iter().map(|e| e.ty).collect();
                return Err(AstTypeError::FunctionCallTypeError {
                    message: format!(
                        "function '{}' expects {} arguments but found {}",
                        ctx.original_fun_name(*fn_name),
                        expected_tys.len(),
                        arg_tys.len()
                    ),
                    expected: expected_tys,
                    found: arg_tys,
                    range: expr.range,
                });
            }

            // A call is generic when the declared signature still mentions any
            // type parameter; otherwise the existing concrete path applies.
            let is_generic = expected_tys.iter().any(|t| t.has_param()) || raw_ret.has_param();

            if !is_generic {
                // check() each argument against the declared parameter type so that
                // bare tags in argument position resolve from the expected context.
                let arg_exprs = args
                    .iter()
                    .zip(&expected_tys)
                    .map(|(arg, &expected_ty)| check(ctx, env, arg, expected_ty))
                    .collect::<Result<Vec<_>, _>>()?;
                // Infer the call's region substitution per lifetime parameter and
                // check the callee's `where` clauses, then stamp the return type.
                let arg_tys: Vec<Ty<'tcx>> = arg_exprs.iter().map(|a| a.ty).collect();
                let region_subst =
                    instantiate_call_regions(ctx, &fun_sig, &raw_expected, &arg_tys, expr.range)?;
                let ret_ty = ctx.region_subst_ty(raw_ret, &region_subst);
                return Ok(typed_hir::Expr {
                    expr: typed_hir::Expression::Call {
                        fn_name: *fn_name,
                        args: arg_exprs,
                    },
                    range: expr.range,
                    ty: ret_ty,
                    kind: Kind::Owned,
                });
            }

            // Generic call: infer parametric arguments, check concrete ones, then
            // unify to solve the type parameters and substitute into the return.
            let arg_exprs = args
                .iter()
                .zip(&expected_tys)
                .map(|(arg, &decl)| {
                    if decl.has_param() {
                        infer(ctx, env, arg)
                    } else {
                        check(ctx, env, arg, decl)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;

            let mut mapping: Subst<'tcx> = Map::new();
            for (&decl, a) in expected_tys.iter().zip(&arg_exprs) {
                if decl.has_param() {
                    unify(decl, a.ty, &mut mapping).map_err(|_| {
                        AstTypeError::FunctionCallTypeError {
                            message: format!(
                                "could not infer type parameters of '{}' from its arguments",
                                ctx.original_fun_name(*fn_name)
                            ),
                            expected: expected_tys.clone(),
                            found: arg_exprs.iter().map(|e| e.ty).collect(),
                            range: expr.range,
                        }
                    })?;
                }
            }
            // Check the callee's `where T : C` constraints against the solved
            // type arguments (Step 10b): the instantiation must have an instance.
            for tc in &fun_sig.type_constraints {
                if let Some(&arg_ty) = mapping.get(&tc.param) {
                    check_type_constraint(ctx, tc.class, arg_ty, expr.range)?;
                }
            }
            // Infer the call's region substitution per lifetime parameter and
            // check the callee's `where` clauses, stamp the return type, then
            // substitute the solved type parameters.
            let arg_tys: Vec<Ty<'tcx>> = arg_exprs.iter().map(|a| a.ty).collect();
            let region_subst =
                instantiate_call_regions(ctx, &fun_sig, &raw_expected, &arg_tys, expr.range)?;
            let filled_ret = ctx.region_subst_ty(raw_ret, &region_subst);
            let ret_ty = subst(ctx, filled_ret, &mapping);

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Call {
                    fn_name: *fn_name,
                    args: arg_exprs,
                },
                range: expr.range,
                ty: ret_ty,
                kind: Kind::Owned,
            })
        }
        qhir::Expression::IntrinsicCall { fn_name, args } if fn_name.is_ptr_op() => {
            infer_ptr_op(ctx, env, *fn_name, args, None, expr.range)
        }
        qhir::Expression::IntrinsicCall { fn_name, args } => {
            let (_, fn_sig) = &INTRINSICS[fn_name];
            let (expected_tys, ret_ty) = fn_sig.resolve(&ctx.types);

            if args.len() != expected_tys.len() {
                // Arity mismatch: infer args just for the error message.
                let arg_exprs = args
                    .iter()
                    .map(|arg| infer(ctx, env, arg))
                    .collect::<Result<Vec<_>, _>>()?;
                let arg_tys: Vec<Ty<'tcx>> = arg_exprs.iter().map(|e| e.ty).collect();
                return Err(AstTypeError::FunctionCallTypeError {
                    message: format!(
                        "intrinsic '{}' expects {} arguments but found {}",
                        fn_name,
                        expected_tys.len(),
                        arg_tys.len()
                    ),
                    expected: expected_tys,
                    found: arg_tys,
                    range: expr.range,
                });
            }

            // check() each argument against the declared parameter type.
            let arg_exprs = args
                .iter()
                .zip(&expected_tys)
                .map(|(arg, &expected_ty)| check(ctx, env, arg, expected_ty))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::IntrinsicCall {
                    fn_name: *fn_name,
                    args: arg_exprs,
                },
                range: expr.range,
                ty: ret_ty,
                kind: Kind::Owned,
            })
        }
        qhir::Expression::Block {
            statements,
            expr: ret_expr,
        } => {
            // A block opens a fresh lexical region scope; locals bound here live
            // in it, and the block may not yield a borrow of one (Calculus §6.3).
            let block_region = ctx.enter_region_scope();
            let block_depth = ctx.region_depth(block_region);

            let computed = (|| {
                let (typed_statements, final_env) = statements.iter().try_fold(
                    (Vec::with_capacity(statements.len()), env.clone()),
                    |(mut stmts, mut env), stmt| {
                        stmts.push(infer_statement(ctx, &mut env, stmt)?);
                        Ok((stmts, env))
                    },
                )?;

                let (typed_expr, ret_ty, kind) = if let Some(e) = ret_expr {
                    let t_expr = infer(ctx, &final_env, e)?;
                    let ret_ty = t_expr.ty;
                    let kind = t_expr.kind;
                    (Some(Box::new(t_expr)), ret_ty, kind)
                } else {
                    (None, ctx.types.unit, Kind::Owned)
                };
                Ok::<_, AstTypeError<'tcx>>((typed_statements, typed_expr, ret_ty, kind))
            })();
            ctx.exit_region_scope();

            let (typed_statements, typed_expr, ret_ty, kind) = computed?;
            escape_check(ctx, ret_ty, block_depth, expr.range)?;

            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Block {
                    statements: typed_statements,
                    expr: typed_expr,
                    // Drops are inserted by the ownership pass (Step B).
                    drops: Vec::new(),
                },
                range: expr.range,
                ty: ret_ty,
                kind,
            })
        }
        qhir::Expression::Match { scrutinee, arms } => {
            let scrut_expr = infer(ctx, env, scrutinee)?;
            let typed_arms =
                type_check_match_arms(ctx, env, arms, scrut_expr.ty, None, expr.range)?;
            let structural = typed_arms
                .first()
                .map(|a| a.body.ty)
                .unwrap_or(ctx.types.unit);
            // Stamp the join with the meet of the arm regions so a borrow
            // escaping through *any* arm is caught (see `join_region_ty`).
            let branches: Vec<(Ty<'tcx>, Kind)> = typed_arms
                .iter()
                .map(|a| (a.body.ty, a.body.kind))
                .collect();
            let result_ty = join_region_ty(ctx, structural, &branches);
            // The match diverges only if every arm does.
            let kind = typed_arms
                .iter()
                .map(|a| a.body.kind)
                .fold(Kind::Never, Kind::join);
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Match {
                    scrutinee: Box::new(scrut_expr),
                    arms: typed_arms,
                },
                ty: result_ty,
                range: expr.range,
                kind,
            })
        }
    }
}
