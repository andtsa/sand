//! check the type of a subexpression against an expected type

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::lang::types::EnumRef;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::passes::type_ast::TypeEnv;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::infer::infer;
use crate::passes::type_ast::infer::infer_statement;

/// type-check a list of match arms against a scrutinee of type `scrutinee_ty`.
///
/// `scrutinee_ty` may be an enum type (tag-coverage exhaustiveness, the
/// classic case) or a tuple type (every legal pattern is irrefutable — see
/// `DESTRUCTURING_PATTERNS.todo.md` decision D1 — so exhaustiveness reduces to
/// "the first arm's pattern always matches").  any other scrutinee type is a
/// `MatchNonEnumScrutinee` error.
///
/// if `forced_expected` is `Some(ty)`, all arm bodies are checked against that
/// type (check mode, enables bare-tag resolution in bodies).  if `None`, the
/// type is inferred from the first arm and subsequent arms are checked against
/// it
///
/// returns the list of typed match arms on success
pub(super) fn type_check_match_arms(
    ctx: &mut CompileCtx,
    env: &TypeEnv,
    arms: &[qhir::QMatchArm],
    scrutinee_ty: Ty,
    forced_expected: Option<Ty>,
    range: Range,
) -> Result<Vec<typed_hir::TypedMatchArm>, AstTypeError> {
    // classify the scrutinee type up front — copy out of the borrow before
    // taking `&mut ctx` again (mirrors the existing `match ctx.ty_kind(..) {
    // TyKind::Enum(er) => *er, ... }` idiom used elsewhere in this module)
    enum ScrutKind {
        Enum(EnumRef),
        Tuple,
        Other,
    }
    let kind = match ctx.ty_kind(scrutinee_ty) {
        TyKind::Enum(er) => ScrutKind::Enum(*er),
        TyKind::Tuple(_) => ScrutKind::Tuple,
        _ => ScrutKind::Other,
    };

    match kind {
        ScrutKind::Enum(enum_ref) => type_check_match_arms_inner(
            ctx,
            env,
            arms,
            scrutinee_ty,
            Some(enum_ref),
            forced_expected,
            range,
        ),
        ScrutKind::Tuple => {
            type_check_match_arms_inner(ctx, env, arms, scrutinee_ty, None, forced_expected, range)
        }
        ScrutKind::Other => Err(AstTypeError::MatchNonEnumScrutinee {
            ty: scrutinee_ty,
            range,
        }),
    }
}

/// shared driver for both enum- and tuple-scrutinee matches.
///
/// `enum_ref` is `Some` for enum scrutinees (enabling tag-coverage
/// exhaustiveness + `Variant`/`Tag` patterns) and `None` for tuple scrutinees
/// (where only irrefutable patterns are legal at all, so exhaustiveness is
/// trivial — see decision D1 in the design doc).
fn type_check_match_arms_inner(
    ctx: &mut CompileCtx,
    env: &TypeEnv,
    arms: &[qhir::QMatchArm],
    scrutinee_ty: Ty,
    enum_ref: Option<EnumRef>,
    forced_expected: Option<Ty>,
    range: Range,
) -> Result<Vec<typed_hir::TypedMatchArm>, AstTypeError> {
    let mut covered_variants: std::collections::BTreeSet<usize> = Default::default();
    // an "irrefutable pattern" is one that is statically guaranteed to match
    // any value of the scrutinee's type — `Wildcard`, `Binding`, or (for
    // tuple scrutinees) `Tuple` patterns whose element patterns are all
    // irrefutable too (guaranteed by D1: only bindings/wildcards/tuples are
    // allowed in sub-pattern position, and tuples are product types with no
    // internal refutability). seeing one means every subsequent arm is dead.
    let mut irrefutable_seen = false;
    let mut typed_arms: Vec<typed_hir::TypedMatchArm> = Vec::with_capacity(arms.len());
    // the type all arm bodies must produce
    let mut result_ty: Option<Ty> = forced_expected;

    for arm in arms {
        if irrefutable_seen {
            return Err(AstTypeError::UnreachableMatchArm { range: arm.range });
        }

        let mut bindings: Vec<(UniqVar, Ty, Range)> = Vec::new();

        // validate & translate the pattern (always at "top level" — the
        // pattern is being matched directly against the scrutinee)
        let match_pattern = match &arm.pattern {
            qhir::QPattern::Variant {
                enum_ref: pat_er,
                variant_idx,
                payload,
            } => {
                let Some(enum_ref) = enum_ref else {
                    return Err(AstTypeError::MatchNonEnumScrutinee {
                        ty: scrutinee_ty,
                        range: arm.range,
                    });
                };
                if *pat_er != enum_ref {
                    return Err(AstTypeError::MatchWrongEnumType {
                        expected_enum: ctx.get_enum(enum_ref).name.clone(),
                        found_enum: ctx.get_enum(*pat_er).name.clone(),
                        range: arm.range,
                    });
                }
                if !covered_variants.insert(*variant_idx) {
                    let variant_name = ctx.get_enum(enum_ref).variants[*variant_idx].name.clone();
                    return Err(AstTypeError::DuplicateMatchPattern {
                        pattern: variant_name,
                        range: arm.range,
                    });
                }
                let typed_payload = check_variant_payload_pattern(
                    ctx,
                    *pat_er,
                    *variant_idx,
                    payload.as_deref(),
                    arm.range,
                    &mut bindings,
                )?;
                typed_hir::MatchPattern::Variant {
                    enum_ref: *pat_er,
                    variant_idx: *variant_idx,
                    payload: typed_payload,
                }
            }
            qhir::QPattern::Tag { variant, payload } => {
                let Some(enum_ref) = enum_ref else {
                    return Err(AstTypeError::MatchNonEnumScrutinee {
                        ty: scrutinee_ty,
                        range: arm.range,
                    });
                };
                let idx = ctx.lookup_variant(enum_ref, variant).ok_or_else(|| {
                    AstTypeError::UnknownTagVariant {
                        variant: variant.clone(),
                        enum_name: ctx.get_enum(enum_ref).name.clone(),
                        range: arm.range,
                    }
                })?;
                if !covered_variants.insert(idx) {
                    return Err(AstTypeError::DuplicateMatchPattern {
                        pattern: variant.clone(),
                        range: arm.range,
                    });
                }
                let typed_payload = check_variant_payload_pattern(
                    ctx,
                    enum_ref,
                    idx,
                    payload.as_deref(),
                    arm.range,
                    &mut bindings,
                )?;
                typed_hir::MatchPattern::Variant {
                    enum_ref,
                    variant_idx: idx,
                    payload: typed_payload,
                }
            }
            qhir::QPattern::Tuple(sub_patterns) => {
                let typed =
                    check_tuple_pattern(ctx, sub_patterns, scrutinee_ty, arm.range, &mut bindings)?;
                irrefutable_seen = true;
                typed
            }
            qhir::QPattern::Binding { var, range: brange } => {
                bindings.push((*var, scrutinee_ty, *brange));
                irrefutable_seen = true;
                typed_hir::MatchPattern::Binding {
                    var: *var,
                    ty: scrutinee_ty,
                    range: *brange,
                }
            }
            qhir::QPattern::Wildcard => {
                irrefutable_seen = true;
                typed_hir::MatchPattern::Wildcard
            }
        };

        // extend the env with this arm's pattern bindings (immutable — D4)
        let mut arm_env = env.clone();
        for (var, ty, _range) in &bindings {
            arm_env.insert(*var, (*ty, false));
        }

        // typecheck the arm body
        let typed_body = match result_ty {
            Some(ty) => check(ctx, &arm_env, &arm.body, ty)?,
            None => infer(ctx, &arm_env, &arm.body)?,
        };

        // lock in the result type from the first arm
        if result_ty.is_none() {
            result_ty = Some(typed_body.ty);
        }

        typed_arms.push(typed_hir::TypedMatchArm {
            pattern: match_pattern,
            body: typed_body,
            range: arm.range,
        });
    }

    // exhaustiveness:
    //  - enum scrutinees: require every variant to be covered, unless an
    //    irrefutable pattern (wildcard/binding) was seen
    //  - tuple scrutinees: every legal pattern is irrefutable (D1), so a single arm
    //    is always exhaustive — `irrefutable_seen` is guaranteed `true` by the time
    //    we get here (every branch above that can apply to a tuple scrutinee sets
    //    it), so there is nothing further to check.
    if let Some(enum_ref) = enum_ref
        && !irrefutable_seen
    {
        let enum_def = ctx.get_enum(enum_ref);
        let num_variants = enum_def.variants.len();
        if covered_variants.len() < num_variants {
            let uncovered: Vec<String> = (0..num_variants)
                .filter(|i| !covered_variants.contains(i))
                .map(|i| enum_def.variants[i].name.clone())
                .collect();
            return Err(AstTypeError::NonExhaustiveMatch {
                enum_name: enum_def.name.clone(),
                uncovered,
                range,
            });
        }
    }

    Ok(typed_arms)
}

/// validate & translate an (optional) payload sub-pattern attached to a
/// `Variant`/`Tag` pattern, against the variant's declared payload type.
/// returns the typed sub-pattern (or `None` for nullary variants / patterns
/// that don't destructure).
fn check_variant_payload_pattern(
    ctx: &mut CompileCtx,
    enum_ref: EnumRef,
    variant_idx: usize,
    payload_pattern: Option<&qhir::QPattern>,
    arm_range: Range,
    bindings: &mut Vec<(UniqVar, Ty, Range)>,
) -> Result<Option<(Ty, Box<typed_hir::MatchPattern>)>, AstTypeError> {
    let declared_payload = ctx.get_enum(enum_ref).variants[variant_idx].payload;
    match (declared_payload, payload_pattern) {
        (None, None) => Ok(None),
        (Some(payload_ty), Some(sub)) => {
            let typed_sub = check_subpattern(ctx, sub, payload_ty, arm_range, bindings)?;
            Ok(Some((payload_ty, Box::new(typed_sub))))
        }
        (declared, pattern) => {
            let enum_name = ctx.get_enum(enum_ref).name.clone();
            let variant = ctx.get_enum(enum_ref).variants[variant_idx].name.clone();
            Err(AstTypeError::PatternPayloadMismatch {
                enum_name,
                variant,
                expected_payload: declared.is_some() && pattern.is_none(),
                range: arm_range,
            })
        }
    }
}

/// validate & translate a tuple pattern `(p1, p2, ...)` against `scrutinee_ty`
/// (which must be a tuple type of matching arity).
fn check_tuple_pattern(
    ctx: &mut CompileCtx,
    sub_patterns: &[qhir::QPattern],
    scrutinee_ty: Ty,
    arm_range: Range,
    bindings: &mut Vec<(UniqVar, Ty, Range)>,
) -> Result<typed_hir::MatchPattern, AstTypeError> {
    let elem_tys = match ctx.ty_kind(scrutinee_ty) {
        TyKind::Tuple(tys) => tys.clone(),
        _ => {
            return Err(AstTypeError::PatternTypeMismatch {
                message: format!(
                    "tuple pattern used against non-tuple type {}",
                    ctx.display_ty(scrutinee_ty)
                ),
                range: arm_range,
            });
        }
    };
    if elem_tys.len() != sub_patterns.len() {
        return Err(AstTypeError::PatternArityMismatch {
            expected: elem_tys.len(),
            found: sub_patterns.len(),
            range: arm_range,
        });
    }
    let typed_elems = sub_patterns
        .iter()
        .zip(elem_tys.iter())
        .map(|(p, ty)| check_subpattern(ctx, p, *ty, arm_range, bindings))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(typed_hir::MatchPattern::Tuple {
        ty: scrutinee_ty,
        elems: typed_elems,
    })
}

/// validate & translate a pattern that appears in a *nested* (sub-pattern)
/// position — inside a payload or a tuple element. per decision D1, only
/// irrefutable forms are allowed here: bindings, wildcards, and recursive
/// tuple-destructuring. `Variant`/`Tag` patterns (which would be refutable,
/// requiring full pattern-matrix exhaustiveness analysis to support soundly)
/// are rejected with `RefutableNestedPattern`.
fn check_subpattern(
    ctx: &mut CompileCtx,
    pattern: &qhir::QPattern,
    expected_ty: Ty,
    arm_range: Range,
    bindings: &mut Vec<(UniqVar, Ty, Range)>,
) -> Result<typed_hir::MatchPattern, AstTypeError> {
    match pattern {
        qhir::QPattern::Variant {
            enum_ref,
            variant_idx,
            ..
        } => Err(AstTypeError::RefutableNestedPattern {
            enum_name: ctx.get_enum(*enum_ref).name.clone(),
            variant: ctx.get_enum(*enum_ref).variants[*variant_idx].name.clone(),
            range: arm_range,
        }),
        qhir::QPattern::Tag { variant, .. } => {
            // bare tags in nested position: we don't know the enum statically
            // without the expected type being one — try to resolve it so the
            // error message can name the right variant; fall back gracefully.
            let enum_name = match ctx.ty_kind(expected_ty) {
                TyKind::Enum(er) => ctx.get_enum(*er).name.clone(),
                _ => "<unknown>".to_string(),
            };
            Err(AstTypeError::RefutableNestedPattern {
                enum_name,
                variant: variant.clone(),
                range: arm_range,
            })
        }
        qhir::QPattern::Tuple(sub_patterns) => {
            check_tuple_pattern(ctx, sub_patterns, expected_ty, arm_range, bindings)
        }
        qhir::QPattern::Binding { var, range } => {
            bindings.push((*var, expected_ty, *range));
            Ok(typed_hir::MatchPattern::Binding {
                var: *var,
                ty: expected_ty,
                range: *range,
            })
        }
        qhir::QPattern::Wildcard => Ok(typed_hir::MatchPattern::Wildcard),
    }
}

/// Bidirectional type checking: verify that `expr` has type `expected`,
/// propagating the expected type into sub-expressions where useful (primarily
/// bare `#Tag` expressions and the trailing expression of if-else / blocks).
pub(super) fn check(
    ctx: &mut CompileCtx,
    env: &TypeEnv,
    expr: &qhir::Expr,
    expected: Ty,
) -> Result<typed_hir::Expr, AstTypeError> {
    match &expr.expr {
        // Resolve a bare #Tag against the expected enum type.
        qhir::Expression::Tag { variant } => {
            let er = match ctx.ty_kind(expected) {
                TyKind::Enum(er) => *er,
                _ => {
                    return Err(AstTypeError::TagInNonEnumContext {
                        variant: variant.clone(),
                        range: expr.range,
                    });
                }
            };
            let variant_idx =
                ctx.lookup_variant(er, variant)
                    .ok_or_else(|| AstTypeError::UnknownTagVariant {
                        variant: variant.clone(),
                        enum_name: ctx.get_enum(er).name.clone(),
                        range: expr.range,
                    })?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Constructor {
                    enum_ref: er,
                    variant_idx,
                    payload: None,
                },
                ty: expected,
                range: expr.range,
            })
        }

        // Propagate check mode into both branches of an if-else.
        qhir::Expression::If {
            cond,
            t,
            f: Some(f),
        } => {
            let cond_expr = infer(ctx, env, cond)?;
            if cond_expr.ty != Ty::BOOL {
                return Err(AstTypeError::TypeError {
                    message: format!("condition of 'if' must be Bool, found {}", cond_expr.ty),
                    expected: Ty::BOOL,
                    found: cond_expr.ty,
                    range: cond.range,
                });
            }
            let t_expr = check(ctx, env, t, expected)?;
            let f_expr = check(ctx, env, f, expected)?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::If {
                    cond: Box::new(cond_expr),
                    t: Box::new(t_expr),
                    f: Box::new(f_expr),
                },
                ty: expected,
                range: expr.range,
            })
        }

        // Propagate check mode into the trailing expression of a block.
        qhir::Expression::Block {
            statements,
            expr: Some(ret),
        } => {
            let (typed_statements, final_env) = statements.iter().try_fold(
                (Vec::with_capacity(statements.len()), env.clone()),
                |(mut stmts, mut env), stmt| {
                    stmts.push(infer_statement(ctx, &mut env, stmt)?);
                    Ok((stmts, env))
                },
            )?;
            let typed_ret = check(ctx, &final_env, ret, expected)?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Block {
                    statements: typed_statements,
                    expr: Some(Box::new(typed_ret)),
                },
                ty: expected,
                range: expr.range,
            })
        }

        // propagate check mode into all arms of a match expression
        qhir::Expression::Match { scrutinee, arms } => {
            let scrut_expr = infer(ctx, env, scrutinee)?;
            let typed_arms =
                type_check_match_arms(ctx, env, arms, scrut_expr.ty, Some(expected), expr.range)?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Match {
                    scrutinee: Box::new(scrut_expr),
                    arms: typed_arms,
                },
                ty: expected,
                range: expr.range,
            })
        }

        // Push the expected element types down into a tuple literal so that
        // bare tags inside it (`(#red, 5)`) resolve against the declared
        // element types, the same way Tag resolution works at top level.
        qhir::Expression::Tuple(elems) => {
            if let TyKind::Tuple(expected_tys) = ctx.ty_kind(expected)
                && expected_tys.len() == elems.len()
            {
                let expected_tys = expected_tys.clone();
                let typed_elems = elems
                    .iter()
                    .zip(expected_tys.iter())
                    .map(|(e, ety)| check(ctx, env, e, *ety))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(typed_hir::Expr {
                    expr: typed_hir::Expression::Tuple(typed_elems),
                    ty: expected,
                    range: expr.range,
                });
            }
            let e = infer(ctx, env, expr)?;
            if e.ty.type_neq(&expected) {
                return Err(AstTypeError::TypeError {
                    message: format!("expected type {} but found {}", expected, e.ty),
                    expected,
                    found: e.ty,
                    range: expr.range,
                });
            }
            Ok(e)
        }

        // everything else: infer, then verify type matches expected.
        _ => {
            let e = infer(ctx, env, expr)?;
            if e.ty.type_neq(&expected) {
                return Err(AstTypeError::TypeError {
                    message: format!("expected type {} but found {}", expected, e.ty),
                    expected,
                    found: e.ty,
                    range: expr.range,
                });
            }
            Ok(e)
        }
    }
}
