//! check the type of a subexpression against an expected type

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Map;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::lang::types::AdtRef;
use crate::lang::types::Kind;
use crate::lang::types::Region;
use crate::lang::types::RegionVar;
use crate::lang::types::Ty;
use crate::lang::types::TyKind;
use crate::passes::type_ast::TypeEnv;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::generics::Subst;
use crate::passes::type_ast::generics::subst;
use crate::passes::type_ast::infer::escape_check;
use crate::passes::type_ast::infer::infer;
use crate::passes::type_ast::infer::infer_call;
use crate::passes::type_ast::infer::infer_constructor;
use crate::passes::type_ast::infer::infer_method_call;
use crate::passes::type_ast::infer::infer_ptr_op;
use crate::passes::type_ast::infer::infer_statements_recovering;
use crate::passes::type_ast::infer::join_region_ty;

/// Variable bindings introduced by a pattern: each is the uniquified variable,
/// the type bound to it, and its declaration range.
type PatternBindings<'tcx> = Vec<(UniqVar<'tcx>, Ty<'tcx>, Range)>;

/// If `e` diverges (kind `Never`), re-type it to `expected` and return it:
/// a diverging expression inhabits every type (Calculus: Subsumption, `Never <:
/// k`), so checking it against any `expected` succeeds. Returns `None`
/// otherwise.
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

/// A scrutinee instantiation's region arguments, mapping the enum's region
/// parameters to the concrete regions the matched value borrows from.
type RegionSubst = Map<RegionVar, Region>;

/// View a type as an enum scrutinee: the base enum, a substitution mapping its
/// type parameters to the instantiation's type arguments, and a map from its
/// region parameters to the instantiation's region arguments (both empty for a
/// plain, non-generic `Enum`). Returns `None` for non-enum types.
fn enum_instantiation<'tcx>(
    ctx: &CompileCtx<'tcx>,
    ty: Ty<'tcx>,
) -> Option<(AdtRef<'tcx>, Subst<'tcx>, RegionSubst)> {
    match ty.kind() {
        TyKind::Enum(er) => Some((*er, Subst::new(), RegionSubst::new())),
        TyKind::App(er, args, regions) => {
            let def = ctx.get_enum(*er);
            let mapping = def
                .type_params
                .iter()
                .map(|p| p.id)
                .zip(args.iter().copied())
                .collect();
            let region_mapping = def
                .region_params
                .iter()
                .map(|p| p.region)
                .zip(regions.iter().copied())
                .collect();
            Some((*er, mapping, region_mapping))
        }
        _ => None,
    }
}

/// type-check a list of match arms against a scrutinee of type `scrutinee_ty`.
///
/// `scrutinee_ty` may be an enum type (tag-coverage exhaustiveness, the
/// classic case) or a tuple type (every legal pattern is irrefutable, so
/// exhaustiveness reduces to "the first arm's pattern always matches"). Any
/// other scrutinee type is a `MatchNonAggregateScrutinee` error.
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
    // A `match` on a *reference* destructures through the borrow: it matches the
    // pointee's shape and binds each payload field as a `&'r` (shared) or `&'r
    // mut` (exclusive) borrow (see `type_check_match_arms_inner`). We classify on
    // the **pointee** type and thread the reference's region + capability so
    // binding leaves can be wrapped. The shared form is what makes `Clone`
    // implementable for non-`Copy` aggregates; the mutable form enables in-place
    // field mutation.
    let (effective_ty, borrow) = match scrutinee_ty.kind() {
        TyKind::Ref(r, inner) | TyKind::RefMut(r, inner) => {
            let mutable = matches!(scrutinee_ty.kind(), TyKind::RefMut(..));
            // Heaped values are `Unique<Node>` handles; reading their fields
            // through a borrow needs a `unique_borrow` indirection that the heap
            // lowering does not yet provide, so reject for now (would miscompile).
            if let TyKind::Enum(er) | TyKind::App(er, _, _) = inner.kind()
                && ctx.get_enum(*er).heaped_strategy().is_some()
            {
                return Err(AstTypeError::BorrowMatchUnsupported {
                    ty: scrutinee_ty,
                    reason: "destructuring a heaped (`deriving Heaped`) type through a \
                             reference is not yet supported",
                    range,
                });
            }
            (*inner, Some((*r, mutable)))
        }
        _ => (scrutinee_ty, None),
    };

    // classify the (pointee) scrutinee type up front, copying out of the borrow
    // before taking `&mut ctx` again
    enum ScrutKind<'tcx> {
        Enum(AdtRef<'tcx>),
        Tuple,
        Int,
        Bool,
        Other,
    }
    let kind = match effective_ty.kind() {
        TyKind::Enum(er) => ScrutKind::Enum(*er),
        // a generic enum instantiation matches just like its base enum.
        TyKind::App(er, _, _) => ScrutKind::Enum(*er),
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
            effective_ty,
            Some(enum_ref),
            borrow,
            forced_expected,
            range,
        ),
        ScrutKind::Tuple | ScrutKind::Int | ScrutKind::Bool => type_check_match_arms_inner(
            ctx,
            env,
            arms,
            effective_ty,
            None,
            borrow,
            forced_expected,
            range,
        ),
        // `Other` here means the scrutinee (or, for a borrowing match, its
        // pointee) is not a matchable aggregate. Report against the original type.
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
/// trivial).
#[allow(clippy::too_many_arguments)]
fn type_check_match_arms_inner<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    env: &TypeEnv<'tcx>,
    arms: &[qhir::QMatchArm<'tcx>],
    scrutinee_ty: Ty<'tcx>,
    enum_ref: Option<AdtRef<'tcx>>,
    // `Some(('r, mutable))` for a borrowing match (scrutinee was `&'r T` or `&'r
    // mut T`): each pattern binding is then a `&'r`/`&'r mut` borrow of its field
    // rather than an owned move. `scrutinee_ty` is already the pointee `T`.
    borrow: Option<(Region, bool)>,
    forced_expected: Option<Ty<'tcx>>,
    range: Range,
) -> Result<Vec<typed_hir::TypedMatchArm<'tcx>>, AstTypeError<'tcx>> {
    let mut typed_arms: Vec<typed_hir::TypedMatchArm> = Vec::with_capacity(arms.len());
    // Patterns translated so far, for the incremental reachability (usefulness)
    // check: an arm is reachable iff its pattern matches some value none of the
    // preceding arms' patterns do.
    let mut prior_patterns: Vec<typed_hir::MatchPattern<'tcx>> = Vec::with_capacity(arms.len());
    // the type all arm bodies must produce
    let mut result_ty: Option<Ty<'tcx>> = forced_expected;
    // substitutions from the scrutinee's instantiation (empty for plain enums),
    // applied to variant payload types so bindings get concrete types + regions.
    let (inst, region_inst): (Subst<'tcx>, RegionSubst) = enum_instantiation(ctx, scrutinee_ty)
        .map(|(_, m, rm)| (m, rm))
        .unwrap_or_default();

    for arm in arms {
        let mut bindings: PatternBindings<'tcx> = Vec::new();

        // validate & translate the pattern (always at "top level": the
        // pattern is being matched directly against the scrutinee)
        let mut match_pattern = match &arm.pattern {
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
                let typed_payload = check_variant_payload_pattern(
                    ctx,
                    *pat_er,
                    *variant_idx,
                    payload.as_deref(),
                    &inst,
                    &region_inst,
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
                let typed_payload = check_variant_payload_pattern(
                    ctx,
                    enum_ref,
                    idx,
                    payload.as_deref(),
                    &inst,
                    &region_inst,
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
                check_tuple_pattern(ctx, sub_patterns, scrutinee_ty, arm.range, &mut bindings)?
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
                typed_hir::MatchPattern::IntLit(*n)
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
                typed_hir::MatchPattern::BoolLit(*b)
            }
            qhir::QPattern::Binding { var, range: brange } => {
                bindings.push((*var, scrutinee_ty, *brange));
                typed_hir::MatchPattern::Binding {
                    var: *var,
                    ty: scrutinee_ty,
                    range: *brange,
                }
            }
            qhir::QPattern::Wildcard => typed_hir::MatchPattern::Wildcard,
        };

        // Reachability: this arm is dead if its pattern matches no value the
        // preceding arms leave uncovered (catches both exact duplicates and
        // any arm shadowed by an earlier catch-all or constructor set). A dead
        // arm is a *warning*, not an error (matches Rust's `unreachable_patterns`
        // lint): its body is still type-checked below, but it is dropped from
        // the lowered program (it can never be selected).
        let reachable = arm_is_reachable(ctx, scrutinee_ty, &prior_patterns, &match_pattern);
        if !reachable {
            ctx.warn(
                arm.range,
                "unreachable match arm (appears after a wildcard or exhaustive pattern)",
            );
        }
        // Borrowing match: rewrite every binding leaf from an owned `T` to a
        // `&'r T` / `&'r mut T` borrow of the field (the field's region is the
        // scrutinee reference's region `'r`, its capability the scrutinee's).
        // Structural pattern node types stay unwrapped (pointee), so the decision
        // tree and codegen index the real layout.
        if let Some((r, mutable)) = borrow {
            for binding in bindings.iter_mut() {
                binding.1 = wrap_borrow(ctx, r, mutable, binding.1);
            }
            wrap_pattern_binding_tys(ctx, &mut match_pattern, r, mutable);
        }

        prior_patterns.push(match_pattern.clone());

        // extend the env with this arm's pattern bindings (immutable).
        // Bindings live in the current lexical scope (the enclosing block).
        let mut arm_env = env.clone();
        let home = ctx.current_scope_region();
        for (var, ty, _range) in &bindings {
            // A reference-typed binding (from a borrowing match) carries the
            // borrow capability; everything else is an owned value.
            let kind = match ty.kind() {
                TyKind::Ref(..) => Kind::Borrowed,
                TyKind::RefMut(..) => Kind::BorrowedMut,
                _ => Kind::Owned,
            };
            arm_env.insert(*var, (*ty, kind, false, home));
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

        // Keep only live arms in the lowered program; a dead arm was warned
        // about above and contributes nothing to the decision tree.
        if reachable {
            typed_arms.push(typed_hir::TypedMatchArm {
                pattern: match_pattern,
                body: typed_body,
                range: arm.range,
            });
        }
    }

    // Exhaustiveness: the match covers every value iff the all-wildcard row is
    // *not* useful against the arms (no uncovered witness exists). Works
    // uniformly for enums, bools, ints, tuples, and arbitrarily nested patterns.
    let witnesses = exhaustiveness_witnesses(ctx, scrutinee_ty, &prior_patterns);
    if !witnesses.is_empty() {
        return Err(AstTypeError::NonExhaustiveMatch {
            enum_name: enum_ref
                .map(|er| ctx.get_enum(er).name.clone())
                .unwrap_or_else(|| ctx.display_ty(scrutinee_ty).to_string()),
            uncovered: witnesses,
            range,
        });
    }

    Ok(typed_arms)
}

// --- Pattern usefulness (Maranget, ML'08) ---
//
// A single algorithm drives both reachability and exhaustiveness, over the same
// pattern matrix the decision-tree lowering uses. `useful(P, q)` answers: does
// the row `q` match some value that no row of the matrix `P` matches? Then:
//   - arm `i` is **reachable** iff its row is useful against arms `0..i`;
//   - the match is **exhaustive** iff the all-wildcard row is *not* useful
//     against every arm (an uncovered value would be a witness of usefulness).
// This subsumes the old ad-hoc coverage sets and correctly handles refutable
// patterns nested in tuples / at arbitrary depth.

/// A constructor identifying one "shape" a value of a column type can take.
#[derive(Clone, PartialEq)]
enum Ctor {
    /// enum variant by index.
    Variant(usize),
    Int(i64),
    Bool(bool),
    /// the sole constructor of a tuple type (arity = tuple width).
    Tuple,
}

/// A normalised pattern for the usefulness matrix: either a wildcard (covering
/// `Binding`/`Wildcard`) or a constructor applied to sub-patterns. Carries no
/// type information; column types are tracked alongside the matrix.
#[derive(Clone)]
enum Pat {
    Wild,
    Ctor { ctor: Ctor, args: Vec<Pat> },
}

/// Normalise a typed pattern into a [`Pat`] (dropping bindings to wildcards,
/// since bindings impose no test; variable extraction happens at the matched
/// arm).
fn to_pat(p: &typed_hir::MatchPattern<'_>) -> Pat {
    match p {
        typed_hir::MatchPattern::Wildcard | typed_hir::MatchPattern::Binding { .. } => Pat::Wild,
        typed_hir::MatchPattern::Variant {
            variant_idx,
            payload,
            ..
        } => Pat::Ctor {
            ctor: Ctor::Variant(*variant_idx),
            args: payload.iter().map(|(_, sub)| to_pat(sub)).collect(),
        },
        typed_hir::MatchPattern::Tuple { elems, .. } => Pat::Ctor {
            ctor: Ctor::Tuple,
            args: elems.iter().map(to_pat).collect(),
        },
        typed_hir::MatchPattern::IntLit(n) => Pat::Ctor {
            ctor: Ctor::Int(*n),
            args: Vec::new(),
        },
        typed_hir::MatchPattern::BoolLit(b) => Pat::Ctor {
            ctor: Ctor::Bool(*b),
            args: Vec::new(),
        },
    }
}

/// The complete set of constructors for `ty`, or `None` if the type has no
/// finite signature that literal patterns could exhaust (e.g. `Int`, or a type
/// that cannot be matched refutably at all). A `Some` signature means a match
/// is exhaustive once every listed constructor is covered.
fn type_signature<'tcx>(ctx: &CompileCtx<'tcx>, ty: Ty<'tcx>) -> Option<Vec<Ctor>> {
    if let Some((er, _, _)) = enum_instantiation(ctx, ty) {
        let n = ctx.get_enum(er).variants.len();
        return Some((0..n).map(Ctor::Variant).collect());
    }
    match ty.kind() {
        TyKind::Bool => Some(vec![Ctor::Bool(false), Ctor::Bool(true)]),
        TyKind::Tuple(_) => Some(vec![Ctor::Tuple]),
        _ => None,
    }
}

/// The field types introduced by specialising `ty`'s column on `ctor` (the
/// sub-occurrences). Empty for nullary constructors.
fn ctor_field_tys<'tcx>(ctx: &mut CompileCtx<'tcx>, ty: Ty<'tcx>, ctor: &Ctor) -> Vec<Ty<'tcx>> {
    match ctor {
        Ctor::Variant(vi) => match enum_instantiation(ctx, ty) {
            Some((er, inst, region_inst)) => match ctx.get_enum(er).variants[*vi].payload.get() {
                Some(p) => {
                    let p = subst(ctx, p, &inst);
                    vec![ctx.region_subst_ty(p, &region_inst)]
                }
                None => Vec::new(),
            },
            None => Vec::new(),
        },
        Ctor::Tuple => match ty.kind() {
            TyKind::Tuple(tys) => tys.to_vec(),
            _ => Vec::new(),
        },
        Ctor::Int(_) | Ctor::Bool(_) => Vec::new(),
    }
}

/// Render a constructor (with already-rendered argument strings) as a pattern,
/// for non-exhaustiveness witnesses.
fn render_ctor<'tcx>(ctx: &CompileCtx<'tcx>, ty: Ty<'tcx>, ctor: &Ctor, args: &[String]) -> String {
    match ctor {
        Ctor::Variant(vi) => {
            let name = match enum_instantiation(ctx, ty) {
                Some((er, _, _)) => ctx.get_enum(er).variants[*vi].name.clone(),
                None => format!("#{vi}"),
            };
            if args.is_empty() {
                name
            } else {
                format!("{name}({})", args.join(", "))
            }
        }
        Ctor::Tuple => format!("({})", args.join(", ")),
        Ctor::Bool(b) => b.to_string(),
        Ctor::Int(n) => n.to_string(),
    }
}

/// Distinct head constructors appearing in column 0 of the matrix.
fn head_ctors(matrix: &[Vec<Pat>]) -> Vec<Ctor> {
    let mut out: Vec<Ctor> = Vec::new();
    for row in matrix {
        if let Pat::Ctor { ctor, .. } = &row[0]
            && !out.contains(ctor)
        {
            out.push(ctor.clone());
        }
    }
    out
}

/// Specialise the matrix on `ctor`: keep rows whose head is `ctor` (expanding
/// its sub-patterns into the leading columns) or a wildcard (expanding to
/// wildcards), dropping rows headed by a different constructor.
fn specialize<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    matrix: &[Vec<Pat>],
    col_tys: &[Ty<'tcx>],
    ctor: &Ctor,
    head_ty: Ty<'tcx>,
) -> (Vec<Vec<Pat>>, Vec<Ty<'tcx>>) {
    let field_tys = ctor_field_tys(ctx, head_ty, ctor);
    let arity = field_tys.len();
    let mut new_tys = field_tys;
    new_tys.extend_from_slice(&col_tys[1..]);

    let mut rows = Vec::new();
    for row in matrix {
        let rest = &row[1..];
        match &row[0] {
            Pat::Ctor { ctor: c, args } if c == ctor => {
                let mut new_row = args.clone();
                new_row.extend_from_slice(rest);
                rows.push(new_row);
            }
            Pat::Wild => {
                let mut new_row = vec![Pat::Wild; arity];
                new_row.extend_from_slice(rest);
                rows.push(new_row);
            }
            _ => {} // different constructor: does not match
        }
    }
    (rows, new_tys)
}

/// The default matrix: rows headed by a wildcard, with column 0 dropped.
fn default_matrix<'tcx>(
    matrix: &[Vec<Pat>],
    col_tys: &[Ty<'tcx>],
) -> (Vec<Vec<Pat>>, Vec<Ty<'tcx>>) {
    let rows = matrix
        .iter()
        .filter(|row| matches!(row[0], Pat::Wild))
        .map(|row| row[1..].to_vec())
        .collect();
    (rows, col_tys[1..].to_vec())
}

/// `useful(P, q)`: `Some(witness)` if `q` matches a value no row of `P` does
/// (the witness is one such value, rendered per column), `None` otherwise.
fn useful<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    matrix: &[Vec<Pat>],
    col_tys: &[Ty<'tcx>],
    q: &[Pat],
) -> Option<Vec<String>> {
    // Base case: no columns. Useful iff the matrix has no rows.
    if col_tys.is_empty() {
        return matrix.is_empty().then(Vec::new);
    }
    let head_ty = col_tys[0];

    match &q[0] {
        Pat::Ctor { ctor, args } => {
            let (sp, sp_tys) = specialize(ctx, matrix, col_tys, ctor, head_ty);
            let mut q2 = args.clone();
            q2.extend_from_slice(&q[1..]);
            useful(ctx, &sp, &sp_tys, &q2).map(|w| wrap_witness(ctx, head_ty, ctor, w))
        }
        Pat::Wild => {
            let used = head_ctors(matrix);
            let sig = type_signature(ctx, head_ty);
            let complete = sig
                .as_ref()
                .is_some_and(|all| all.iter().all(|c| used.contains(c)));
            if complete {
                // Every constructor is present: q is useful iff it is useful for
                // at least one of them.
                for ctor in sig.unwrap() {
                    let arity = ctor_field_tys(ctx, head_ty, &ctor).len();
                    let (sp, sp_tys) = specialize(ctx, matrix, col_tys, &ctor, head_ty);
                    let mut q2 = vec![Pat::Wild; arity];
                    q2.extend_from_slice(&q[1..]);
                    if let Some(w) = useful(ctx, &sp, &sp_tys, &q2) {
                        return Some(wrap_witness(ctx, head_ty, &ctor, w));
                    }
                }
                None
            } else {
                // The column is not covered: recurse on the default matrix and
                // prepend a witness for a value the present constructors miss.
                let (dp, dp_tys) = default_matrix(matrix, col_tys);
                let w_rest = useful(ctx, &dp, &dp_tys, &q[1..])?;
                let head = match &sig {
                    Some(all) => match all.iter().find(|c| !used.contains(c)) {
                        Some(missing) => {
                            let arity = ctor_field_tys(ctx, head_ty, missing).len();
                            render_ctor(ctx, head_ty, missing, &vec!["_".to_string(); arity])
                        }
                        None => "_".to_string(),
                    },
                    // infinite / unmatchable type: any unlisted value works.
                    None => "_".to_string(),
                };
                let mut w = vec![head];
                w.extend(w_rest);
                Some(w)
            }
        }
    }
}

/// Wrap the leading `arity` witness columns into `ctor`, leaving the rest.
fn wrap_witness<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    head_ty: Ty<'tcx>,
    ctor: &Ctor,
    mut w: Vec<String>,
) -> Vec<String> {
    let arity = ctor_field_tys(ctx, head_ty, ctor).len();
    let rest = w.split_off(arity);
    let head = render_ctor(ctx, head_ty, ctor, &w);
    let mut out = vec![head];
    out.extend(rest);
    out
}

/// Is `arm` reachable given the `prior` arms (i.e. useful against them)?
fn arm_is_reachable<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    scrutinee_ty: Ty<'tcx>,
    prior: &[typed_hir::MatchPattern<'tcx>],
    arm: &typed_hir::MatchPattern<'tcx>,
) -> bool {
    let matrix: Vec<Vec<Pat>> = prior.iter().map(|p| vec![to_pat(p)]).collect();
    let q = vec![to_pat(arm)];
    useful(ctx, &matrix, &[scrutinee_ty], &q).is_some()
}

/// Witnesses of non-exhaustiveness for a match over `scrutinee_ty` with the
/// given arm patterns; empty when the match is exhaustive.
fn exhaustiveness_witnesses<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    scrutinee_ty: Ty<'tcx>,
    arms: &[typed_hir::MatchPattern<'tcx>],
) -> Vec<String> {
    let matrix: Vec<Vec<Pat>> = arms.iter().map(|p| vec![to_pat(p)]).collect();
    let q = vec![Pat::Wild];
    useful(ctx, &matrix, &[scrutinee_ty], &q).unwrap_or_default()
}

/// validate & translate an (optional) payload sub-pattern attached to a
/// `Variant`/`Tag` pattern, against the variant's declared payload type.
/// returns the typed sub-pattern (or `None` for nullary variants / patterns
/// that don't destructure).
#[allow(clippy::too_many_arguments)]
fn check_variant_payload_pattern<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    enum_ref: AdtRef<'tcx>,
    variant_idx: usize,
    payload_pattern: Option<&qhir::QPattern<'tcx>>,
    inst: &Subst<'tcx>,
    region_inst: &RegionSubst,
    arm_range: Range,
    bindings: &mut PatternBindings<'tcx>,
) -> Result<Option<(Ty<'tcx>, Box<typed_hir::MatchPattern<'tcx>>)>, AstTypeError<'tcx>> {
    // Substitute the scrutinee's type arguments into the declared payload so a
    // pattern on `Option<Int>#Some(x)` binds `x : Int`, not the parameter `T`,
    //
    // this means that x also binds its region arguments:
    // `Holder<'a>#H(x)` binds `x : &'a Int`
    // a borrow matched out of the ADT keeps the instantiation's lifetime, so the
    // escape check catches it being returned.
    let raw_payload = ctx.get_enum(enum_ref).variants[variant_idx].payload.get();
    let declared_payload = raw_payload.map(|p| {
        let p = subst(ctx, p, inst);
        ctx.region_subst_ty(p, region_inst)
    });
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

/// Wrap a field type as a `&'r` (shared) or `&'r mut` (exclusive) borrow.
fn wrap_borrow<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    region: Region,
    mutable: bool,
    ty: Ty<'tcx>,
) -> Ty<'tcx> {
    if mutable {
        ctx.ref_mut_ty(region, ty)
    } else {
        ctx.ref_ty(region, ty)
    }
}

/// Rewrite every `Binding` leaf's type in a pattern to a `&'r`/`&'r mut` borrow
/// of it (used for a borrowing match, destructuring through a reference). Only
/// the binding leaves change; structural node types (`Variant.ty`, `Tuple.ty`,
/// payload types) stay the bare pointee types so the decision tree / codegen
/// index the real layout.
fn wrap_pattern_binding_tys<'tcx>(
    ctx: &mut CompileCtx<'tcx>,
    pattern: &mut typed_hir::MatchPattern<'tcx>,
    region: Region,
    mutable: bool,
) {
    match pattern {
        typed_hir::MatchPattern::Binding { ty, .. } => {
            *ty = wrap_borrow(ctx, region, mutable, *ty);
        }
        typed_hir::MatchPattern::Tuple { elems, .. } => {
            for e in elems.iter_mut() {
                wrap_pattern_binding_tys(ctx, e, region, mutable);
            }
        }
        typed_hir::MatchPattern::Variant { payload, .. } => {
            if let Some((_, sub)) = payload {
                wrap_pattern_binding_tys(ctx, sub, region, mutable);
            }
        }
        typed_hir::MatchPattern::Wildcard
        | typed_hir::MatchPattern::IntLit(_)
        | typed_hir::MatchPattern::BoolLit(_) => {}
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
/// validate & translate a pattern that appears in a *nested* (sub-pattern)
/// position: inside a payload or a tuple element.
///
/// Supported here:
/// - bindings, wildcards: irrefutable
/// - tuple destructuring: recursively irrefutable
/// - enum variant patterns (`E#V(p)` or `#V(p)`): refutable but well-typed
/// - integer / boolean literal patterns (`IntLit`, `BoolLit`): refutable; the
///   usefulness checker tracks their coverage at any depth
///
/// Refutability at any depth is fine: the usefulness checker decides
/// exhaustiveness over the whole pattern matrix and the decision tree emits the
/// runtime test against the extracted sub-occurrence.
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
            let (inst, region_inst) = match enum_instantiation(ctx, expected_ty) {
                Some((er, inst, region_inst)) if er == *pat_er => (inst, region_inst),
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
                &region_inst,
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
            let (enum_ref, inst, region_inst) = match enum_instantiation(ctx, expected_ty) {
                Some(triple) => triple,
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
                &region_inst,
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
        // Literal sub-patterns are refutable but well-typed: the usefulness
        // checker tracks their coverage at any depth and the decision tree tests
        // them against the (extracted) sub-occurrence.
        qhir::QPattern::IntLit(n) => {
            if !matches!(expected_ty.kind(), TyKind::Int) {
                return Err(AstTypeError::PatternTypeMismatch {
                    message: format!(
                        "integer literal pattern used against non-Int type {}",
                        ctx.display_ty(expected_ty)
                    ),
                    range: arm_range,
                });
            }
            Ok(typed_hir::MatchPattern::IntLit(*n))
        }
        qhir::QPattern::BoolLit(b) => {
            if !matches!(expected_ty.kind(), TyKind::Bool) {
                return Err(AstTypeError::PatternTypeMismatch {
                    message: format!(
                        "boolean literal pattern used against non-Bool type {}",
                        ctx.display_ty(expected_ty)
                    ),
                    range: arm_range,
                });
            }
            Ok(typed_hir::MatchPattern::BoolLit(*b))
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
            if !e.ty.eq_modulo_regions(expected) {
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
            // `check` ignores regions (`eq_modulo_regions`), so the branches may
            // each carry a different real region; stamp the join with their meet
            // so a borrow escaping through either branch is caught.
            let ty = join_region_ty(
                ctx,
                expected,
                &[(t_expr.ty, t_expr.kind), (f_expr.ty, f_expr.kind)],
            );
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::If {
                    cond: Box::new(cond_expr),
                    t: Box::new(t_expr),
                    f: Box::new(f_expr),
                },
                ty,
                range: expr.range,
                kind,
            })
        }

        // Propagate check mode into the trailing expression of a block.
        qhir::Expression::Block {
            statements,
            expr: Some(ret),
        } => {
            // A block opens a fresh lexical region scope; the trailing
            // expression may not yield a borrow of a local
            // (Calculus: The Escape Check).
            let block_region = ctx.enter_region_scope();
            let block_depth = ctx.region_depth(block_region);

            let computed = (|| {
                let (typed_statements, final_env) =
                    infer_statements_recovering(ctx, env, statements);
                let typed_ret = check(ctx, &final_env, ret, expected)?;
                Ok::<_, AstTypeError<'tcx>>((typed_statements, typed_ret))
            })();
            ctx.exit_region_scope();

            let (typed_statements, typed_ret) = computed?;
            let kind = typed_ret.kind;
            // Carry the *actual* result type (region-blind-equal to `expected`, but
            // keeping its real regions) so a nested block's borrow region survives
            // to the enclosing block's escape check; regions live on the type.
            let ret_ty = typed_ret.ty;
            escape_check(ctx, ret_ty, block_depth, expr.range)?;
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Block {
                    statements: typed_statements,
                    expr: Some(Box::new(typed_ret)),
                    // Drops are inserted by the ownership pass.
                    drops: Vec::new(),
                },
                ty: ret_ty,
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
            // Stamp the join with the meet of the arm regions so a borrow
            // escaping through *any* arm is caught (see `join_region_ty`).
            let branches: Vec<(Ty<'tcx>, Kind)> = typed_arms
                .iter()
                .map(|a| (a.body.ty, a.body.kind))
                .collect();
            let ty = join_region_ty(ctx, expected, &branches);
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Match {
                    scrutinee: Box::new(scrut_expr),
                    arms: typed_arms,
                },
                ty,
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
                // Carry the *actual* element types (region-blind-equal to
                // `expected`, but keeping their real regions) so a borrow of a
                // local stored in the tuple survives to the escape check; regions
                // live on the type (cf. the Block / if / match arms).
                let elem_tys: Vec<Ty<'tcx>> = typed_elems.iter().map(|e| e.ty).collect();
                let ty = ctx.intern_tuple(elem_tys);
                return Ok(typed_hir::Expr {
                    expr: typed_hir::Expression::Tuple(typed_elems),
                    ty,
                    range: expr.range,
                    kind: Kind::Owned,
                });
            }
            let e = infer(ctx, env, expr)?;
            if let Some(coerced) = coerce_never(&e, expected) {
                return Ok(coerced);
            }
            if !e.ty.eq_modulo_regions(expected) {
                return Err(AstTypeError::TypeError {
                    message: format!("expected type {} but found {}", expected, e.ty),
                    expected,
                    found: e.ty,
                    range: expr.range,
                });
            }
            Ok(e)
        }

        // Raw-pointer ops are generic; push the expected type
        // down so `__ptr_cast` can take its target type from the context.
        qhir::Expression::IntrinsicCall { fn_name, args, .. } if fn_name.is_ptr_op() => {
            let e = infer_ptr_op(ctx, env, *fn_name, args, Some(expected), expr.range)?;
            if !e.ty.eq_modulo_regions(expected) {
                return Err(AstTypeError::TypeError {
                    message: format!("expected type {} but found {}", expected, e.ty),
                    expected,
                    found: e.ty,
                    range: expr.range,
                });
            }
            Ok(e)
        }

        // Push the expected type into a direct function call so a type parameter
        // that appears only in the return (`fail<A>(c: Int): Check<A>`) is solved
        // from it rather than left ambiguous.
        qhir::Expression::Call { fn_name, args } => {
            let e = infer_call(ctx, env, expr, *fn_name, args, Some(expected))?;
            if let Some(coerced) = coerce_never(&e, expected) {
                return Ok(coerced);
            }
            if !e.ty.eq_modulo_regions(expected) {
                return Err(AstTypeError::TypeError {
                    message: format!("expected type {} but found {}", expected, e.ty),
                    expected,
                    found: e.ty,
                    range: expr.range,
                });
            }
            Ok(e)
        }

        // Push the expected type into a typeclass method call so a receiver that
        // appears only in the result (`pure<A>(x: A): F<A>`) can be solved from
        // it. `infer_method_call` then verifies the result matches.
        qhir::Expression::MethodCall {
            class,
            method,
            args,
        } => {
            let e = infer_method_call(ctx, env, expr, *class, method, args, Some(expected))?;
            if let Some(coerced) = coerce_never(&e, expected) {
                return Ok(coerced);
            }
            if !e.ty.eq_modulo_regions(expected) {
                return Err(AstTypeError::TypeError {
                    message: format!("expected type {} but found {}", expected, e.ty),
                    expected,
                    found: e.ty,
                    range: expr.range,
                });
            }
            Ok(e)
        }

        // Check a lambda against an expected function type, pushing the
        // expected *return* type into the body. Without this, a lambda is always
        // inferred, so a body whose type isn't fully determined locally (e.g. a
        // generic constructor `Res#Err(x)` whose parameter only appears in
        // another variant) can't be resolved even when the surrounding arrow
        // type pins it. The param keeps its annotation, which must match the
        // expected domain.
        qhir::Expression::Lambda { param, body, mode }
            if matches!(expected.kind(), TyKind::Fn(_, _, _, _)) =>
        {
            let TyKind::Fn(arg_ty, ret_ty, exp_mode, _) = expected.kind() else {
                unreachable!("guarded by the match arm condition")
            };
            if !param.ty.eq_modulo_regions(*arg_ty) {
                return Err(AstTypeError::TypeError {
                    message: format!(
                        "lambda parameter has type {} but {} is expected here",
                        param.ty, arg_ty
                    ),
                    expected: *arg_ty,
                    found: param.ty,
                    range: expr.range,
                });
            }
            let home = ctx.current_scope_region();
            let mut body_env = env.clone();
            body_env.insert(param.name, (param.ty, Kind::Owned, param.is_mutable, home));
            let body_typed = check(ctx, &body_env, body, *ret_ty)?;

            let mut referenced = std::collections::HashSet::new();
            crate::analysis::annotate::collect_dependencies(&body_typed.expr, &mut referenced);
            let mut captures: Vec<(UniqVar<'tcx>, Ty<'tcx>)> = referenced
                .into_iter()
                .filter(|v| *v != param.name)
                .filter_map(|v| env.get(&v).map(|binding| (v, binding.0)))
                .collect();
            captures.sort_by_key(|(v, _)| *v);

            // Infer the calling mode from capture usage (see the infer path).
            let assumptions = ctx.type_assumptions().to_vec();
            let inferred_mode =
                crate::analysis::annotate::closure_mode_from_body(&body_typed, &captures, &|t| {
                    ctx.is_copy_under(t, &assumptions)
                });
            let mode = (*mode).max(inferred_mode);

            // The (inferred) mode must be usable where the expected arrow mode is
            // required — e.g. a closure that consumes a capture (`FnOnce`) cannot
            // be supplied where a reusable (`Fn`) closure is expected.
            if !mode.usable_as(*exp_mode) {
                return Err(AstTypeError::TypeError {
                    message: format!("a {expected} closure is required here"),
                    expected,
                    found: ctx.fn_ty(param.ty, body_typed.ty, mode),
                    range: expr.range,
                });
            }

            // Carry the closure's concrete environment type (matching the infer
            // path), so a checked lambda retains its captures even against an
            // abstract expected arrow — this is what lets the function-return
            // escape check see a captured local borrow's region.
            let env_ty = match captures.as_slice() {
                [] => ctx.types.unit,
                [(_, t)] => *t,
                many => ctx.intern_tuple(many.iter().map(|(_, t)| *t).collect()),
            };
            let fn_ty = ctx.closure_ty(param.ty, body_typed.ty, mode, env_ty);
            Ok(typed_hir::Expr {
                expr: typed_hir::Expression::Lambda {
                    param: param.clone(),
                    body: Box::new(body_typed),
                    captures,
                },
                range: expr.range,
                ty: fn_ty,
                kind: Kind::Owned,
            })
        }

        // everything else: infer, then verify type matches expected.
        _ => {
            let e = infer(ctx, env, expr)?;
            // A diverging expression (kind `Never`) inhabits any type, so it
            // satisfies the expected type regardless and is re-typed to it.
            if let Some(coerced) = coerce_never(&e, expected) {
                return Ok(coerced);
            }
            if !e.ty.eq_modulo_regions(expected) {
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

// --- let-pattern ---
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
            let (inst, region_inst) = match enum_instantiation(ctx, expected_ty) {
                Some((er, inst, region_inst)) if er == *enum_ref => (inst, region_inst),
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
                &region_inst,
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
                "let-pattern LHS must be a constructor pattern `E#V(...)`. use `match` for other patterns"
                    .to_string(),
            range,
        }),
    }
}
