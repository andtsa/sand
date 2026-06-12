//! check the type of a subexpression against an expected type

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::lang::types::EnumRef;
use crate::lang::types::Kind;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::passes::type_ast::TypeEnv;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::generics::Subst;
use crate::passes::type_ast::generics::subst;
use crate::passes::type_ast::infer::infer;
use crate::passes::type_ast::infer::infer_constructor;
use crate::passes::type_ast::infer::infer_statement;

/// Variable bindings introduced by a pattern: each is the uniquified variable,
/// the type bound to it, and its declaration range.
type PatternBindings<'tcx> = Vec<(UniqVar<'tcx>, Ty<'tcx>, Range)>;

/// If `e` diverges (kind `Never`), re-type it to `expected` and return it:
/// a diverging expression inhabits every type (Calculus §6.1, `Never <: k`),
/// so checking it against any `expected` succeeds. Returns `None` otherwise.
fn coerce_never<'tcx>(
    e: &typed_hir::Expr<'tcx>,
    expected: Ty<'tcx>,
) -> Option<typed_hir::Expr<'tcx>> {
    (e.kind == Kind::Never).then(|| typed_hir::Expr {
        expr: e.expr.clone(),
        ty: expected,
        kind: Kind::Never,
        range: e.range,
    })
}

/// View a type as an enum scrutinee: the base enum plus a substitution mapping
/// its type parameters to the instantiation's arguments (empty for a plain,
/// non-generic `Enum`). Returns `None` for non-enum types.
fn enum_instantiation<'tcx>(
    ctx: &CompileCtx<'tcx>,
    ty: Ty<'tcx>,
) -> Option<(EnumRef<'tcx>, Subst<'tcx>)> {
    match ty.kind() {
        TyKind::Enum(er) => Some((*er, Subst::new())),
        TyKind::App(er, args) => {
            let mapping = ctx
                .get_enum(*er)
                .type_params
                .iter()
                .map(|p| p.id)
                .zip(args.iter().copied())
                .collect();
            Some((*er, mapping))
        }
        _ => None,
    }
}

/// type-check a list of match arms against a scrutinee of type `scrutinee_ty`.
///
/// `scrutinee_ty` may be an enum type (tag-coverage exhaustiveness, the
/// classic case) or a tuple type (every legal pattern is irrefutable — see
/// `DESTRUCTURING_PATTERNS.todo.md` decision D1 — so exhaustiveness reduces to
/// "the first arm's pattern always matches").  any other scrutinee type is a
/// `MatchNonAggregateScrutinee` error.
///
/// if `forced_expected` is `Some(ty)`, all arm bodies are checked against that
/// type (check mode, enables bare-tag resolution in bodies).  if `None`, the
/// type is inferred from the first arm and subsequent arms are checked against
/// it
///
/// returns the list of typed match arms on success
pub(super) fn type_check_match_arms<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    env: &TypeEnv<'tcx>,
    arms: &[qhir::QMatchArm<'tcx>],
    scrutinee_ty: Ty<'tcx>,
    forced_expected: Option<Ty<'tcx>>,
    range: Range,
) -> Result<Vec<typed_hir::TypedMatchArm<'tcx>>, AstTypeError<'tcx>> {
    // classify the scrutinee type up front — copy out of the borrow before
    // taking `&mut ctx` again
    enum ScrutKind<'tcx> {
        Enum(EnumRef<'tcx>),
        Tuple,
        Int,
        Bool,
        Other,
    }
    let kind = match scrutinee_ty.kind() {
        TyKind::Enum(er) => ScrutKind::Enum(*er),
        // a generic enum instantiation matches just like its base enum.
        TyKind::App(er, _) => ScrutKind::Enum(*er),
        TyKind::Tuple(_) => ScrutKind::Tuple,
        TyKind::Int => ScrutKind::Int,
        TyKind::Bool => ScrutKind::Bool,
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
        ScrutKind::Tuple | ScrutKind::Int | ScrutKind::Bool => {
            type_check_match_arms_inner(ctx, env, arms, scrutinee_ty, None, forced_expected, range)
        }
        ScrutKind::Other => Err(AstTypeError::MatchNonAggregateScrutinee {
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
fn type_check_match_arms_inner<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    env: &TypeEnv<'tcx>,
    arms: &[qhir::QMatchArm<'tcx>],
    scrutinee_ty: Ty<'tcx>,
    enum_ref: Option<EnumRef<'tcx>>,
    forced_expected: Option<Ty<'tcx>>,
    range: Range,
) -> Result<Vec<typed_hir::TypedMatchArm<'tcx>>, AstTypeError<'tcx>> {
    let mut covered_variants: std::collections::BTreeSet<usize> = Default::default();
    // duplicate-detection set for Int literal patterns (BTreeSet<i64> because
    // `covered_variants` is `usize` and can't represent negative integers).
    let mut covered_int_lits: std::collections::BTreeSet<i64> = Default::default();
    // Nested-enum exhaustiveness tracking: for outer variant arms whose payload is
    // a direct enum pattern (refutable), record which inner variant indices
    // have been covered (irrefutably).  After the arm loop we promote
    // fully-covered outer variants into `covered_variants`.
    // key   = outer variant_idx
    // value = (inner EnumRef, set of covered inner variant_idx)
    let mut nested_enum_coverage: std::collections::BTreeMap<
        usize,
        (EnumRef<'tcx>, std::collections::BTreeSet<usize>),
    > = Default::default();
    // an "irrefutable pattern" is one that is statically guaranteed to match
    // any value of the scrutinee's type — `Wildcard`, `Binding`, or (for
    // tuple scrutinees) `Tuple` patterns whose element patterns are all
    // irrefutable too (guaranteed by D1: only bindings/wildcards/tuples are
    // allowed in sub-pattern position, and tuples are product types with no
    // internal refutability). seeing one means every subsequent arm is dead.
    let mut irrefutable_seen = false;
    let mut typed_arms: Vec<typed_hir::TypedMatchArm> = Vec::with_capacity(arms.len());
    // the type all arm bodies must produce
    let mut result_ty: Option<Ty<'tcx>> = forced_expected;
    // substitution from the scrutinee's instantiation (empty for plain enums),
    // applied to variant payload types so bindings get concrete types.
    let inst: Subst<'tcx> = enum_instantiation(ctx, scrutinee_ty)
        .map(|(_, m)| m)
        .unwrap_or_default();

    for arm in arms {
        if irrefutable_seen {
            return Err(AstTypeError::UnreachableMatchArm { range: arm.range });
        }

        let mut bindings: PatternBindings<'tcx> = Vec::new();

        // validate & translate the pattern (always at "top level" — the
        // pattern is being matched directly against the scrutinee)
        let match_pattern = match &arm.pattern {
            qhir::QPattern::Variant {
                enum_ref: pat_er,
                variant_idx,
                payload,
            } => {
                let Some(enum_ref) = enum_ref else {
                    return Err(AstTypeError::MatchNonAggregateScrutinee {
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
                // Only register this arm as "covering" the outer variant when the
                // payload sub-pattern is irrefutable.  A refutable inner pattern
                // (e.g. `E#A(E#B)`) does not fully cover the outer variant — the
                // caller still needs a wildcard/binding catch-all, and multiple arms
                // can share the same outer variant with different inner patterns.
                if !qpattern_payload_is_refutable(payload.as_deref()) {
                    if !covered_variants.insert(*variant_idx) {
                        let variant_name =
                            ctx.get_enum(enum_ref).variants[*variant_idx].name.clone();
                        return Err(AstTypeError::DuplicateMatchPattern {
                            pattern: variant_name,
                            range: arm.range,
                        });
                    }
                } else {
                    // Payload is refutable.  Try to track which inner-enum variant this
                    // arm covers, so that multiple arms can collectively exhaust an inner
                    // enum and together count as covering the outer variant.
                    record_nested_enum_coverage(
                        ctx,
                        enum_ref,
                        *variant_idx,
                        payload.as_deref(),
                        arm.range,
                        &mut nested_enum_coverage,
                    )?;
                }
                let typed_payload = check_variant_payload_pattern(
                    ctx,
                    *pat_er,
                    *variant_idx,
                    payload.as_deref(),
                    &inst,
                    arm.range,
                    &mut bindings,
                )?;
                typed_hir::MatchPattern::Variant {
                    ty: scrutinee_ty,
                    enum_ref: *pat_er,
                    variant_idx: *variant_idx,
                    payload: typed_payload,
                }
            }
            qhir::QPattern::Tag { variant, payload } => {
                let Some(enum_ref) = enum_ref else {
                    return Err(AstTypeError::MatchNonAggregateScrutinee {
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
                // Same coverage logic: only insert if the inner pattern is irrefutable.
                if !qpattern_payload_is_refutable(payload.as_deref()) {
                    if !covered_variants.insert(idx) {
                        return Err(AstTypeError::DuplicateMatchPattern {
                            pattern: variant.clone(),
                            range: arm.range,
                        });
                    }
                } else {
                    record_nested_enum_coverage(
                        ctx,
                        enum_ref,
                        idx,
                        payload.as_deref(),
                        arm.range,
                        &mut nested_enum_coverage,
                    )?;
                }
                let typed_payload = check_variant_payload_pattern(
                    ctx,
                    enum_ref,
                    idx,
                    payload.as_deref(),
                    &inst,
                    arm.range,
                    &mut bindings,
                )?;
                typed_hir::MatchPattern::Variant {
                    ty: scrutinee_ty,
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
            qhir::QPattern::IntLit(n) => {
                // Int literal patterns are only valid against an Int scrutinee.
                if !matches!(scrutinee_ty.kind(), TyKind::Int) {
                    return Err(AstTypeError::PatternTypeMismatch {
                        message: format!(
                            "integer literal pattern used against non-Int type {}",
                            ctx.display_ty(scrutinee_ty)
                        ),
                        range: arm.range,
                    });
                }
                // duplicate detection
                if !covered_int_lits.insert(*n) {
                    return Err(AstTypeError::DuplicateMatchPattern {
                        pattern: n.to_string(),
                        range: arm.range,
                    });
                }
                typed_hir::MatchPattern::IntLit(*n)
                // note: NOT setting irrefutable_seen — Int literals are
                // refutable
            }
            qhir::QPattern::BoolLit(b) => {
                // Bool literal patterns are only valid against a Bool scrutinee.
                if !matches!(scrutinee_ty.kind(), TyKind::Bool) {
                    return Err(AstTypeError::PatternTypeMismatch {
                        message: format!(
                            "boolean literal pattern used against non-Bool type {}",
                            ctx.display_ty(scrutinee_ty)
                        ),
                        range: arm.range,
                    });
                }
                // track coverage: false = 0, true = 1
                let idx = *b as usize;
                if !covered_variants.insert(idx) {
                    return Err(AstTypeError::DuplicateMatchPattern {
                        pattern: b.to_string(),
                        range: arm.range,
                    });
                }
                typed_hir::MatchPattern::BoolLit(*b)
                // note: NOT setting irrefutable_seen — Bool literals are
                // refutable
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
            arm_env.insert(*var, (*ty, Kind::Owned, false));
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

    // Nested-enum promotion: if all inner variants for a given outer variant are
    // covered by arms with refutable payloads, count the outer variant as covered.
    for (&outer_vi, (inner_er, inner_covered)) in &nested_enum_coverage {
        if !covered_variants.contains(&outer_vi) {
            let num_inner = ctx.get_enum(*inner_er).variants.len();
            if inner_covered.len() == num_inner {
                covered_variants.insert(outer_vi);
            }
        }
    }

    // exhaustiveness:
    //  - enum scrutinees: require every variant to be covered, unless an
    //    irrefutable pattern (wildcard/binding) was seen
    //  - tuple scrutinees: every legal pattern is irrefutable (D1), so a single arm
    //    is always exhaustive — `irrefutable_seen` is guaranteed `true` by the time
    //    we get here (every branch above that can apply to a tuple scrutinee sets
    //    it), so there is nothing further to check.
    check_exhaustiveness(
        ctx,
        scrutinee_ty,
        enum_ref,
        &covered_variants,
        irrefutable_seen,
        range,
    )?;

    Ok(typed_arms)
}

/// When an outer variant arm has a **refutable** payload, attempt to record the
/// specific inner-enum variant that this arm covers, so that multiple arms that
/// together exhaust an inner enum can collectively count as covering the outer
/// variant (nested-enum exhaustiveness, todo 7).
///
/// Only fires when:
/// - the outer variant's declared payload type is a direct enum (not wrapped in
///   a tuple)
/// - the payload pattern is a `Variant` or `Tag` whose own payload is
///   irrefutable
///
/// On a duplicate inner variant (same (outer_vi, inner_vi) pair already
/// recorded), returns an error.
fn record_nested_enum_coverage<'tcx>(
    ctx: &CompileCtx<'tcx>,
    outer_er: EnumRef<'tcx>,
    outer_vi: usize,
    payload: Option<&qhir::QPattern<'tcx>>,
    arm_range: Range,
    nested: &mut std::collections::BTreeMap<
        usize,
        (EnumRef<'tcx>, std::collections::BTreeSet<usize>),
    >,
) -> Result<(), AstTypeError<'tcx>> {
    // The outer variant's declared payload type must be a direct enum.
    let outer_payload_ty = ctx.get_enum(outer_er).variants[outer_vi].payload.get();
    let Some(payload_ty) = outer_payload_ty else {
        return Ok(());
    };
    let inner_er = match payload_ty.kind() {
        TyKind::Enum(er) => *er,
        _ => return Ok(()), // payload is not an enum — nothing to track
    };
    // Resolve the inner variant index from the payload pattern.
    let inner_vi = match payload {
        Some(qhir::QPattern::Variant {
            variant_idx: vi,
            payload: inner_p,
            ..
        }) => {
            if qpattern_payload_is_refutable(inner_p.as_deref()) {
                return Ok(()); // 3+ level nesting: skip for now
            }
            *vi
        }
        Some(qhir::QPattern::Tag {
            variant: name,
            payload: inner_p,
        }) => {
            if qpattern_payload_is_refutable(inner_p.as_deref()) {
                return Ok(()); // 3+ level nesting: skip for now
            }
            match ctx.lookup_variant(inner_er, name) {
                Some(vi) => vi,
                None => return Ok(()), // unknown tag — type error reported elsewhere
            }
        }
        _ => return Ok(()), // not a direct variant/tag payload
    };
    // Record the inner variant.  Duplicate = same inner variant seen twice.
    let entry = nested
        .entry(outer_vi)
        .or_insert_with(|| (inner_er, std::collections::BTreeSet::new()));
    if !entry.1.insert(inner_vi) {
        let outer_name = ctx.get_enum(outer_er).variants[outer_vi].name.clone();
        let inner_name = ctx.get_enum(inner_er).variants[inner_vi].name.clone();
        return Err(AstTypeError::DuplicateMatchPattern {
            pattern: format!("{}({})", outer_name, inner_name),
            range: arm_range,
        });
    }
    Ok(())
}

/// Verify that a match expression's arm set is exhaustive.
///
/// - `enum_ref = Some(er)`: scrutinee is an enum; every variant index in
///   `0..num_variants` must appear in `covered` unless `has_irrefutable` is
///   true (a wildcard/binding arm matches everything remaining).
/// - `enum_ref = None`, `scrutinee_ty = Bool`: exhaustive iff `has_irrefutable`
///   or both `false` (0) and `true` (1) appear in `covered`.
/// - `enum_ref = None`, `scrutinee_ty = Int`: exhaustive iff `has_irrefutable`
///   (Int is unbounded; literal arms alone can never cover all values).
/// - `enum_ref = None`, otherwise (Tuple etc.): trivially exhaustive.
fn check_exhaustiveness<'tcx>(
    ctx: &CompileCtx<'tcx>,
    scrutinee_ty: Ty<'tcx>,
    enum_ref: Option<EnumRef<'tcx>>,
    covered: &std::collections::BTreeSet<usize>,
    has_irrefutable: bool,
    range: Range,
) -> Result<(), AstTypeError<'tcx>> {
    if has_irrefutable {
        return Ok(());
    }
    if let Some(enum_ref) = enum_ref {
        let enum_def = ctx.get_enum(enum_ref);
        let num_variants = enum_def.variants.len();
        if covered.len() < num_variants {
            let uncovered: Vec<String> = (0..num_variants)
                .filter(|i| !covered.contains(i))
                .map(|i| enum_def.variants[i].name.clone())
                .collect();
            return Err(AstTypeError::NonExhaustiveMatch {
                enum_name: enum_def.name.clone(),
                uncovered,
                range,
            });
        }
        return Ok(());
    }
    // Tuple (or unit) — trivially exhaustive (the single irrefutable arm was
    // required). Int — literal arms alone cannot be exhaustive; a wildcard or
    // binding is required. Bool — exhaustive iff both false (0) and true (1)
    // are covered.
    match scrutinee_ty.kind() {
        TyKind::Bool => {
            if covered.contains(&0) && covered.contains(&1) {
                Ok(())
            } else {
                let uncovered: Vec<&str> = [(0usize, "false"), (1, "true")]
                    .iter()
                    .filter(|(i, _)| !covered.contains(i))
                    .map(|(_, name)| *name)
                    .collect();
                Err(AstTypeError::NonExhaustiveMatch {
                    enum_name: "Bool".to_string(),
                    uncovered: uncovered.into_iter().map(str::to_string).collect(),
                    range,
                })
            }
        }
        TyKind::Int => {
            // Int is unbounded; literal arms alone cannot be exhaustive.
            Err(AstTypeError::NonExhaustiveMatch {
                enum_name: "Int".to_string(),
                uncovered: vec!["(all other integers)".to_string()],
                range,
            })
        }
        _ => {
            // Tuple / Unit / etc. — trivially exhaustive once any arm is present.
            Ok(())
        }
    }
}

/// validate & translate an (optional) payload sub-pattern attached to a
/// `Variant`/`Tag` pattern, against the variant's declared payload type.
/// returns the typed sub-pattern (or `None` for nullary variants / patterns
/// that don't destructure).
fn check_variant_payload_pattern<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    enum_ref: EnumRef<'tcx>,
    variant_idx: usize,
    payload_pattern: Option<&qhir::QPattern<'tcx>>,
    inst: &Subst<'tcx>,
    arm_range: Range,
    bindings: &mut PatternBindings<'tcx>,
) -> Result<Option<(Ty<'tcx>, Box<typed_hir::MatchPattern<'tcx>>)>, AstTypeError<'tcx>> {
    // Substitute the scrutinee's type arguments into the declared payload so a
    // pattern on `Option<Int>#Some(x)` binds `x : Int`, not the parameter `T`.
    let raw_payload = ctx.get_enum(enum_ref).variants[variant_idx].payload.get();
    let declared_payload = raw_payload.map(|p| subst(ctx, p, inst));
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
fn check_tuple_pattern<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    sub_patterns: &[qhir::QPattern<'tcx>],
    scrutinee_ty: Ty<'tcx>,
    arm_range: Range,
    bindings: &mut PatternBindings<'tcx>,
) -> Result<typed_hir::MatchPattern<'tcx>, AstTypeError<'tcx>> {
    let elem_tys = match scrutinee_ty.kind() {
        TyKind::Tuple(tys) => *tys,
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

/// Returns `true` if `pattern` can fail to match some values of its type —
/// i.e. it is **refutable**.  `Wildcard` and `Binding` are irrefutable;
/// `Variant`, `Tag`, `IntLit`, `BoolLit` are refutable.  `Tuple` is refutable
/// iff *any* of its elements is refutable (recursive).
fn qpattern_is_refutable<'tcx>(pattern: &qhir::QPattern<'tcx>) -> bool {
    match pattern {
        qhir::QPattern::Variant { .. }
        | qhir::QPattern::Tag { .. }
        | qhir::QPattern::IntLit(_)
        | qhir::QPattern::BoolLit(_) => true,
        qhir::QPattern::Tuple(elems) => elems.iter().any(qpattern_is_refutable),
        qhir::QPattern::Binding { .. } | qhir::QPattern::Wildcard => false,
    }
}

/// Convenience wrapper: is the *payload* of a variant pattern refutable?
fn qpattern_payload_is_refutable<'tcx>(payload: Option<&qhir::QPattern<'tcx>>) -> bool {
    payload.is_some_and(qpattern_is_refutable)
}

/// validate & translate a pattern that appears in a *nested* (sub-pattern)
/// position — inside a payload or a tuple element.
///
/// Supported here:
/// - bindings, wildcards — irrefutable
/// - tuple destructuring — recursively irrefutable
/// - enum variant patterns (`E#V(p)` or `#V(p)`) — refutable but well-typed;
///   exhaustiveness in nested position is **not** checked (noted as a future
///   todo); the check chain in MIR lowering handles the runtime test
///
/// Still rejected: integer/boolean literal patterns (`IntLit`, `BoolLit`) —
/// these cannot be nested because Int is unbounded and exhaustiveness for Bool
/// at an arbitrary nesting depth is not yet tracked.
fn check_subpattern<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    pattern: &qhir::QPattern<'tcx>,
    expected_ty: Ty<'tcx>,
    arm_range: Range,
    bindings: &mut PatternBindings<'tcx>,
) -> Result<typed_hir::MatchPattern<'tcx>, AstTypeError<'tcx>> {
    match pattern {
        qhir::QPattern::Variant {
            enum_ref: pat_er,
            variant_idx,
            payload,
        } => {
            // The expected type must be the same enum that the pattern names.
            let inst = match enum_instantiation(ctx, expected_ty) {
                Some((er, inst)) if er == *pat_er => inst,
                _ => {
                    return Err(AstTypeError::PatternTypeMismatch {
                        message: format!(
                            "enum variant pattern '{}' used against type {}",
                            ctx.get_enum(*pat_er).variants[*variant_idx].name,
                            ctx.display_ty(expected_ty)
                        ),
                        range: arm_range,
                    });
                }
            };
            let typed_payload = check_variant_payload_pattern(
                ctx,
                *pat_er,
                *variant_idx,
                payload.as_deref(),
                &inst,
                arm_range,
                bindings,
            )?;
            Ok(typed_hir::MatchPattern::Variant {
                ty: expected_ty,
                enum_ref: *pat_er,
                variant_idx: *variant_idx,
                payload: typed_payload,
            })
        }
        qhir::QPattern::Tag { variant, payload } => {
            // Resolve the expected type to an enum, then look up the variant.
            let (enum_ref, inst) = match enum_instantiation(ctx, expected_ty) {
                Some(pair) => pair,
                None => {
                    return Err(AstTypeError::PatternTypeMismatch {
                        message: format!(
                            "bare tag pattern '#{variant}' used against non-enum type {}",
                            ctx.display_ty(expected_ty)
                        ),
                        range: arm_range,
                    });
                }
            };
            let idx = ctx.lookup_variant(enum_ref, variant).ok_or_else(|| {
                AstTypeError::UnknownTagVariant {
                    variant: variant.clone(),
                    enum_name: ctx.get_enum(enum_ref).name.clone(),
                    range: arm_range,
                }
            })?;
            let typed_payload = check_variant_payload_pattern(
                ctx,
                enum_ref,
                idx,
                payload.as_deref(),
                &inst,
                arm_range,
                bindings,
            )?;
            Ok(typed_hir::MatchPattern::Variant {
                ty: expected_ty,
                enum_ref,
                variant_idx: idx,
                payload: typed_payload,
            })
        }
        qhir::QPattern::IntLit(n) => Err(AstTypeError::RefutableNestedPattern {
            enum_name: "Int".to_string(),
            variant: n.to_string(),
            range: arm_range,
        }),
        qhir::QPattern::BoolLit(b) => Err(AstTypeError::RefutableNestedPattern {
            enum_name: "Bool".to_string(),
            variant: b.to_string(),
            range: arm_range,
        }),
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
pub(super) fn check<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    env: &TypeEnv<'tcx>,
    expr: &qhir::Expr<'tcx>,
    expected: Ty<'tcx>,
) -> Result<typed_hir::Expr<'tcx>, AstTypeError<'tcx>> {
    match &expr.expr {
        // Resolve a bare #Tag (optionally with payload) against the expected enum type.
        qhir::Expression::Tag { variant, payload } => {
            let er = match expected.kind() {
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
            // `declared_payload_ty` is `Copy` so the immutable borrow of ctx ends here.
            let declared_payload_ty = ctx.get_enum(er).variants[variant_idx].payload.get();
            let typed_payload = match (payload.as_deref(), declared_payload_ty) {
                (None, None) => None,
                (None, Some(_)) => {
                    return Err(AstTypeError::TagMissingPayload {
                        variant: variant.clone(),
                        range: expr.range,
                    });
                }
                (Some(_), None) => {
                    return Err(AstTypeError::TagPayloadOnNullaryVariant {
                        variant: variant.clone(),
                        range: expr.range,
                    });
                }
                (Some(p), Some(declared_ty)) => {
                    let typed = check(ctx, env, p, declared_ty)?;
                    Some(Box::new(typed))
                }
            };
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Constructor {
                    enum_ref: er,
                    variant_idx,
                    payload: typed_payload,
                },
                ty: expected,
                range: expr.range,
                kind: Kind::Owned,
            })
        }

        // Resolve a generic enum constructor against the expected instantiation,
        // e.g. `let x: Option<Int> = Option#None` solves `T = Int` from `expected`.
        qhir::Expression::Constructor {
            enum_ref,
            variant_idx,
            payload,
        } => {
            let e = infer_constructor(
                ctx,
                env,
                expr,
                *enum_ref,
                *variant_idx,
                payload.as_deref(),
                Some(expected),
            )?;
            if e.ty.type_neq(expected) {
                return Err(AstTypeError::TypeError {
                    message: format!("expected type {} but found {}", expected, e.ty),
                    expected,
                    found: e.ty,
                    range: expr.range,
                });
            }
            Ok(e)
        }

        // Propagate check mode into both branches of an if-else.
        qhir::Expression::If {
            cond,
            t,
            f: Some(f),
        } => {
            let cond_expr = infer(ctx, env, cond)?;
            if cond_expr.ty != ctx.types.bool {
                return Err(AstTypeError::TypeError {
                    message: format!("condition of 'if' must be Bool, found {}", cond_expr.ty),
                    expected: ctx.types.bool,
                    found: cond_expr.ty,
                    range: cond.range,
                });
            }
            let t_expr = check(ctx, env, t, expected)?;
            let f_expr = check(ctx, env, f, expected)?;
            let kind = t_expr.kind.join(f_expr.kind);
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::If {
                    cond: Box::new(cond_expr),
                    t: Box::new(t_expr),
                    f: Box::new(f_expr),
                },
                ty: expected,
                range: expr.range,
                kind,
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
            let kind = typed_ret.kind;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Block {
                    statements: typed_statements,
                    expr: Some(Box::new(typed_ret)),
                },
                ty: expected,
                range: expr.range,
                kind,
            })
        }

        // propagate check mode into all arms of a match expression
        qhir::Expression::Match { scrutinee, arms } => {
            let scrut_expr = infer(ctx, env, scrutinee)?;
            let typed_arms =
                type_check_match_arms(ctx, env, arms, scrut_expr.ty, Some(expected), expr.range)?;
            let kind = typed_arms
                .iter()
                .map(|a| a.body.kind)
                .fold(Kind::Never, Kind::join);
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Match {
                    scrutinee: Box::new(scrut_expr),
                    arms: typed_arms,
                },
                ty: expected,
                range: expr.range,
                kind,
            })
        }

        // Push the expected element types down into a tuple literal so that
        // bare tags inside it (`(#red, 5)`) resolve against the declared
        // element types, the same way Tag resolution works at top level.
        qhir::Expression::Tuple(elems) => {
            if let TyKind::Tuple(expected_tys) = expected.kind()
                && expected_tys.len() == elems.len()
            {
                let expected_tys = *expected_tys;
                let typed_elems = elems
                    .iter()
                    .zip(expected_tys.iter())
                    .map(|(e, ety)| check(ctx, env, e, *ety))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(typed_hir::Expr {
                    expr: typed_hir::Expression::Tuple(typed_elems),
                    ty: expected,
                    range: expr.range,
                    kind: Kind::Owned,
                });
            }
            let e = infer(ctx, env, expr)?;
            if let Some(coerced) = coerce_never(&e, expected) {
                return Ok(coerced);
            }
            if e.ty.type_neq(expected) {
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
            // A diverging expression (kind `Never`) inhabits any type, so it
            // satisfies the expected type regardless and is re-typed to it.
            if let Some(coerced) = coerce_never(&e, expected) {
                return Ok(coerced);
            }
            if e.ty.type_neq(expected) {
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

// ── let-pattern ────────────────────────────────────────────────────────

/// Validate and translate a `let E#V(payload) = expr` LHS pattern.
///
/// Returns the typed `MatchPattern` and the list of bindings it introduces
/// (`(UniqVar, Ty, Range)`).
///
/// Errors:
/// - `PatternTypeMismatch` if `scrutinee_ty` is not the enum named in the
///   pattern
/// - `NestedVariantInLetPattern` if the sub-pattern contains a refutable
///   variant
/// - `PatternPayloadMismatch` / `PatternArityMismatch` if arity doesn't match
pub(super) fn check_let_pattern<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    pattern: &qhir::QPattern<'tcx>,
    scrutinee_ty: Ty<'tcx>,
    range: Range,
) -> Result<(typed_hir::MatchPattern<'tcx>, PatternBindings<'tcx>), AstTypeError<'tcx>> {
    let mut bindings: PatternBindings<'tcx> = Vec::new();
    let typed_pattern = check_let_pattern_inner(ctx, pattern, scrutinee_ty, range, &mut bindings)?;
    Ok((typed_pattern, bindings))
}

fn check_let_pattern_inner<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    pattern: &qhir::QPattern<'tcx>,
    expected_ty: Ty<'tcx>,
    range: Range,
    bindings: &mut PatternBindings<'tcx>,
) -> Result<typed_hir::MatchPattern<'tcx>, AstTypeError<'tcx>> {
    match pattern {
        qhir::QPattern::Variant {
            enum_ref,
            variant_idx,
            payload,
        } => {
            // The expected type must be this enum.
            let inst = match enum_instantiation(ctx, expected_ty) {
                Some((er, inst)) if er == *enum_ref => inst,
                _ => {
                    return Err(AstTypeError::PatternTypeMismatch {
                        message: format!(
                            "let-pattern '{}#{}' cannot match value of type {}",
                            ctx.get_enum(*enum_ref).name,
                            ctx.get_enum(*enum_ref).variants[*variant_idx].name,
                            ctx.display_ty(expected_ty)
                        ),
                        range,
                    });
                }
            };
            // The sub-pattern must be irrefutable (no nested variant/tag/literal).
            if let Some(sub) = payload.as_deref()
                && matches!(
                    sub,
                    qhir::QPattern::Variant { .. }
                        | qhir::QPattern::Tag { .. }
                        | qhir::QPattern::IntLit(_)
                        | qhir::QPattern::BoolLit(_)
                )
            {
                return Err(AstTypeError::NestedVariantInLetPattern { range });
            }
            let typed_payload = check_variant_payload_pattern(
                ctx,
                *enum_ref,
                *variant_idx,
                payload.as_deref(),
                &inst,
                range,
                bindings,
            )?;
            Ok(typed_hir::MatchPattern::Variant {
                ty: expected_ty,
                enum_ref: *enum_ref,
                variant_idx: *variant_idx,
                payload: typed_payload,
            })
        }
        _ => Err(AstTypeError::PatternTypeMismatch {
            message:
                "let-pattern LHS must be a constructor pattern `E#V(...)` — use `match` for other patterns"
                    .to_string(),
            range,
        }),
    }
}
