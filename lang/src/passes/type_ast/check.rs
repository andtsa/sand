//! check the type of a subexpression against an expected type

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Range;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::lang::types::EnumRef;
use crate::lang::types::Ty;
use crate::passes::type_ast::TypeEnv;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::infer::infer;
use crate::passes::type_ast::infer::infer_statement;

/// type-check a list of match arms against a known enum scrutinee
///
/// if `forced_expected` is `Some(ty)`, all arm bodies are checked against that
/// type (check mode, enables bare-tag resolution in bodies).  if `None`, the
/// type is inferred from the first arm and subsequent arms are checked against
/// it
///
/// returns the list of typed match arms on success
pub(super) fn type_check_match_arms(
    ctx: &CompileCtx,
    env: &TypeEnv,
    arms: &[qhir::QMatchArm],
    enum_ref: EnumRef,
    forced_expected: Option<Ty>,
    range: Range,
) -> Result<Vec<typed_hir::TypedMatchArm>, AstTypeError> {
    let mut covered_variants: std::collections::BTreeSet<usize> = Default::default();
    let mut wildcard_seen = false;
    let mut typed_arms: Vec<typed_hir::TypedMatchArm> = Vec::with_capacity(arms.len());
    // the type all arm bodies must produce
    let mut result_ty: Option<Ty> = forced_expected;

    for arm in arms {
        if wildcard_seen {
            return Err(AstTypeError::UnreachableMatchArm { range: arm.range });
        }

        // validate & translate the pattern
        let match_pattern = match &arm.pattern {
            qhir::QPattern::Variant {
                enum_ref: pat_er,
                variant_idx,
            } => {
                if *pat_er != enum_ref {
                    return Err(AstTypeError::MatchWrongEnumType {
                        expected_enum: ctx.get_enum(enum_ref).name.clone(),
                        found_enum: ctx.get_enum(*pat_er).name.clone(),
                        range: arm.range,
                    });
                }
                if !covered_variants.insert(*variant_idx) {
                    let variant_name = ctx.get_enum(enum_ref).variants[*variant_idx].clone();
                    return Err(AstTypeError::DuplicateMatchPattern {
                        pattern: variant_name,
                        range: arm.range,
                    });
                }
                typed_hir::MatchPattern::Variant {
                    enum_ref: *pat_er,
                    variant_idx: *variant_idx,
                }
            }
            qhir::QPattern::Tag { variant } => {
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
                typed_hir::MatchPattern::Variant {
                    enum_ref,
                    variant_idx: idx,
                }
            }
            qhir::QPattern::Wildcard => {
                wildcard_seen = true;
                typed_hir::MatchPattern::Wildcard
            }
        };

        // typecheck the arm body
        let typed_body = match result_ty {
            Some(ty) => check(ctx, env, &arm.body, ty)?,
            None => infer(ctx, env, &arm.body)?,
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

    // exhaustiveness: require every variant to be covered (unless wildcard was
    // seen)
    if !wildcard_seen {
        let enum_def = ctx.get_enum(enum_ref);
        let num_variants = enum_def.variants.len();
        if covered_variants.len() < num_variants {
            let uncovered: Vec<String> = (0..num_variants)
                .filter(|i| !covered_variants.contains(i))
                .map(|i| enum_def.variants[i].clone())
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

/// Bidirectional type checking: verify that `expr` has type `expected`,
/// propagating the expected type into sub-expressions where useful (primarily
/// bare `#Tag` expressions and the trailing expression of if-else / blocks).
pub(super) fn check(
    ctx: &CompileCtx,
    env: &TypeEnv,
    expr: &qhir::Expr,
    expected: Ty,
) -> Result<typed_hir::Expr, AstTypeError> {
    match &expr.expr {
        // Resolve a bare #Tag against the expected enum type.
        qhir::Expression::Tag { variant } => {
            let er = match expected {
                Ty::Enum(er) => er,
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
            if cond_expr.ty != Ty::Bool {
                return Err(AstTypeError::TypeError {
                    message: format!("condition of 'if' must be Bool, found {:?}", cond_expr.ty),
                    expected: Ty::Bool,
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
            let enum_ref = match scrut_expr.ty {
                Ty::Enum(er) => er,
                ty => {
                    return Err(AstTypeError::MatchNonEnumScrutinee {
                        ty,
                        range: scrutinee.range,
                    });
                }
            };
            let typed_arms =
                type_check_match_arms(ctx, env, arms, enum_ref, Some(expected), expr.range)?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Match {
                    scrutinee: Box::new(scrut_expr),
                    arms: typed_arms,
                },
                ty: expected,
                range: expr.range,
            })
        }

        // everything else: infer, then verify type matches expected.
        _ => {
            let e = infer(ctx, env, expr)?;
            if e.ty.type_neq(&expected) {
                return Err(AstTypeError::TypeError {
                    message: format!("expected type {:?} but found {:?}", expected, e.ty),
                    expected,
                    found: e.ty,
                    range: expr.range,
                });
            }
            Ok(e)
        }
    }
}
