#![allow(clippy::result_large_err)]

use pest::iterators::Pair;

use super::*;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Derivable;
use crate::compiler::structure::HeapedStrategy;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::TypeHead;
use crate::passes::parse::Rule;

/// Register one `type` declaration's skeleton (name, type/region params,
/// variant names) and stash its raw payload pairs for phase 1b.
pub(crate) fn collect_enum_skeleton<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    child: &Pair<'i, Rule>,
    cur_mod: ModuleRef<'run>,
    pending_payloads: &mut Vec<(AdtRef<'run>, usize, Vec<Pair<'i, Rule>>)>,
    generic_enums: &mut Vec<AdtRef<'run>>,
) -> Result<(), AstError> {
    let range = Range::from(child);
    let mut inner = child.clone().into_inner();
    let enum_name = inner
        .next()
        .missing("enum name", range)?
        .as_str()
        .to_string();

    // optional type/region parameters: `type Ref<'r, T> = ...`. Allocate them
    // now so phase 1b can resolve `T` and `'r` in payloads.
    let (type_params, region_params) =
        if inner.peek().map(|p| p.as_rule()) == Some(Rule::type_params) {
            let tp_pair = inner.next().missing("type parameters", range)?;
            let specs = collect_type_params(ctx, tp_pair.clone());
            let type_params = ctx.begin_type_params(&specs);
            let region_params = ctx.begin_region_params(&collect_region_params(tp_pair));
            (type_params, region_params)
        } else {
            (Vec::new(), Vec::new())
        };

    // enum_variant = { identifier ~ ("(" ~ type_ ~ ")")? }, optionally followed
    // by a `deriving C1, C2, ...` clause.
    let mut variant_names = Vec::new();
    let mut variant_payloads = Vec::new();
    let mut derives: Vec<Derivable> = Vec::new();
    for pair in inner {
        match pair.as_rule() {
            Rule::enum_variant => {
                let v_range = Range::from(&pair);
                let mut v_inner = pair.into_inner();
                let v_name = v_inner
                    .next()
                    .missing("variant name", v_range)?
                    .as_str()
                    .to_string();
                variant_names.push(v_name);
                // Remaining children are the payload type(s); >1 desugar to a
                // tuple payload when the payloads are resolved.
                variant_payloads.push(v_inner.collect::<Vec<_>>());
            }
            Rule::deriving_clause => derives = parse_deriving_clause(&pair)?,
            other => {
                return Err(AstError::UnexpectedRule {
                    expected: "enum variant or deriving clause",
                    got: other,
                    range: Range::from(&pair),
                });
            }
        }
    }

    let is_generic = !type_params.is_empty();
    let er = ctx.register_enum(
        &enum_name,
        variant_names,
        type_params,
        region_params,
        range,
        cur_mod,
        derives,
    )?;
    if is_generic {
        generic_enums.push(er);
    }
    for (idx, payload_pairs) in variant_payloads.into_iter().enumerate() {
        if !payload_pairs.is_empty() {
            pending_payloads.push((er, idx, payload_pairs));
        }
    }
    Ok(())
}

/// Dispatch a `deriving C1, C2, ...` clause. A general mechanism: each
/// derivable name maps to an action. Only `HeapedUnique` is registered today
/// (it sets the type's heap strategy); future derivables (`HeapedShared`, `Eq`,
/// `Clone`, ...) add arms here. An unknown name is a `NotDerivable` error.
/// Returns the derived heap strategy, if any.
pub(crate) fn parse_deriving_clause(pair: &Pair<Rule>) -> Result<Vec<Derivable>, AstError> {
    assert_eq!(pair.as_rule(), Rule::deriving_clause);
    let mut derives: Vec<Derivable> = Vec::new();
    for ident in pair.clone().into_inner() {
        let range = Range::from(&ident);
        let derivable = match ident.as_str() {
            // `Heaped` = the base heap capability (alloc/borrow/release), backed
            // by the default unique strategy. `HeapedShared` (the refcount
            // strategy) is a planned addition here.
            "Heaped" => Derivable::Heaped(HeapedStrategy::Unique),
            other => {
                return Err(AstError::NotDerivable {
                    name: other.to_string(),
                    range,
                });
            }
        };
        if derives.contains(&derivable) {
            return Err(AstError::DuplicateDerive {
                name: ident.as_str().to_string(),
                range,
            });
        }
        derives.push(derivable);
    }
    Ok(derives)
}

/// phase 1b, resolve each stashed variant payload type, now that every enum
/// skeleton exists. Each payload resolves with its enum's type/region
/// parameters in scope and its bare type names in the enum's own module; a
/// borrow in a payload must name a declared region parameter (or `'static`).
pub(crate) fn resolve_enum_payloads<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    pending_payloads: Vec<(AdtRef<'run>, usize, Vec<Pair<'i, Rule>>)>,
) -> Result<(), AstError> {
    for (er, idx, payload_pairs) in pending_payloads {
        let params = ctx.get_enum(er).type_params.clone();
        let region_params = ctx.get_enum(er).region_params.clone();
        ctx.set_build_module(ctx.get_enum(er).src_module);
        ctx.enter_type_param_scope(&params);
        ctx.enter_region_param_scope(&region_params);
        let payload_range = Range::from(&payload_pairs[0]);
        // Multiple payload types desugar to a single tuple payload:
        // `Cons(Int, List)` ≡ `Cons((Int, List))`.
        let payload_ty = if payload_pairs.len() == 1 {
            build_type(ctx, payload_pairs.into_iter().next().unwrap())?
        } else {
            let tys = payload_pairs
                .into_iter()
                .map(|p| build_type(ctx, p))
                .collect::<Result<Vec<_>, _>>()?;
            ctx.intern_tuple(tys)
        };
        let mut payload_regions = Vec::new();
        payload_ty.free_regions(&mut payload_regions);
        for r in payload_regions {
            let ok = match r {
                Region::Static => true,
                Region::Var(rv) => region_params.iter().any(|p| p.region == rv),
            };
            if !ok {
                return Err(AstError::PayloadBorrowNeedsLifetime {
                    name: ctx.get_enum(er).name.clone(),
                    range: payload_range,
                });
            }
        }
        ctx.set_variant_payload(er, idx, payload_ty);
    }
    ctx.end_type_params();
    Ok(())
}

/// phase 2, build every function body, grouped by the module it is declared in
/// (`module ...;` switches the current module). Enum / `use` declarations were
/// already handled in phase 1.
/// A display string for an instance head, used to mangle impl-method names.
pub(crate) fn head_name<'a>(ctx: &CompileCtx<'a>, head: TypeHead<'a>) -> String {
    match head {
        TypeHead::Int => "Int".to_string(),
        TypeHead::Bool => "Bool".to_string(),
        TypeHead::Unit => "Unit".to_string(),
        TypeHead::Enum(er) => ctx.get_enum(er).name.clone(),
    }
}

/// Heap-strategy legality (Calculus, `K-HeapedRec`): a (mutually) recursive
/// `type` *must* derive a heap strategy (`deriving HeapedUnique`); without it
/// its values would be infinite-sized and leak. A non-recursive type *may*
/// derive one (to opt a large value onto the heap) but need not.
pub(crate) fn check_heaped_legality(ctx: &CompileCtx<'_>) -> Result<(), AstError> {
    for er in ctx.all_enums().collect::<Vec<_>>() {
        let def = ctx.get_enum(er);
        if def.is_anonymous {
            continue;
        }
        if is_recursive_enum(ctx, er) && def.heaped_strategy().is_none() {
            return Err(AstError::RecursiveTypeNeedsHeaped {
                name: def.name.clone(),
                range: def.range,
            });
        }
    }
    Ok(())
}

/// The enums directly referenced in `er`'s variant payloads.
pub(crate) fn enum_successors<'tcx>(ctx: &CompileCtx<'tcx>, er: AdtRef<'tcx>) -> Vec<AdtRef<'tcx>> {
    let mut out = Vec::new();
    for v in &ctx.get_enum(er).variants {
        if let Some(ty) = v.payload.get() {
            collect_referenced_enums(ty, &mut out);
        }
    }
    out
}

/// Push every enum mentioned in `ty` (directly or nested) into `out`.
pub(crate) fn collect_referenced_enums<'tcx>(ty: Ty<'tcx>, out: &mut Vec<AdtRef<'tcx>>) {
    match ty.kind() {
        TyKind::Enum(er) => out.push(*er),
        TyKind::App(er, args, _) => {
            out.push(*er);
            for a in args.iter() {
                collect_referenced_enums(*a, out);
            }
        }
        TyKind::Tuple(elems) => {
            for e in elems.iter() {
                collect_referenced_enums(*e, out);
            }
        }
        TyKind::Region(t, _) | TyKind::Ref(_, t) | TyKind::RefMut(_, t) | TyKind::Ptr(t) => {
            collect_referenced_enums(*t, out);
        }
        TyKind::Fn(a, r, _) => {
            collect_referenced_enums(*a, out);
            collect_referenced_enums(*r, out);
        }
        _ => {}
    }
}

/// Whether `start` can reach itself through payload references; i.e. it is
/// (directly or mutually) recursive.
// `EnumRef` reaches an enum payload `Cell` (interior mutability), but the set
// keys hash/compare by arena-pointer identity that never reads the `Cell`, so
// the keys are stable, mirroring the suppression on `CompileCtx`'s maps.
#[allow(clippy::mutable_key_type)]
pub(crate) fn is_recursive_enum<'tcx>(ctx: &CompileCtx<'tcx>, start: AdtRef<'tcx>) -> bool {
    let mut stack = enum_successors(ctx, start);
    let mut visited: std::collections::BTreeSet<AdtRef<'tcx>> = std::collections::BTreeSet::new();
    while let Some(n) = stack.pop() {
        if n == start {
            return true;
        }
        if visited.insert(n) {
            stack.extend(enum_successors(ctx, n));
        }
    }
    false
}

/// An FFI boundary type must be `Int`, `Unit`, or `Ptr<T>`.
pub(crate) fn require_ffi_safe<'tcx>(
    ctx: &CompileCtx<'tcx>,
    ty: Ty<'tcx>,
    range: Range,
) -> Result<(), AstError> {
    let ok = matches!(ty.kind(), TyKind::Int | TyKind::Unit | TyKind::Ptr(_));
    if ok {
        Ok(())
    } else {
        Err(AstError::NonFfiSafeType {
            ty: ctx.display_ty(ty).to_string(),
            range,
        })
    }
}

/// Validate the declared variance of a generic enum's parameters against the
/// positions they occupy in its variant payloads (Calculus: Types, variance).
///
/// Each payload position carries a *polarity*: producer positions (enum
/// payloads, tuple elements, pointee of a reference/pointer, a function's
/// *result*) are covariant; a function's *argument* is the first consumer
/// (contravariant) position in the grammar, so descending into it flips
/// polarity. Generic applications `F<..>` compose: an argument under a
/// contravariant parameter of `F` flips, under an invariant one becomes
/// invariant. A parameter that occurs at both polarities is invariant.
///
/// An **explicit** annotation is checked against the inferred polarity:
/// `+a` requires no contravariant occurrence, `-a` no covariant occurrence,
/// `∅a` is always sound, and an unused parameter accepts anything. A parameter
/// with **no** annotation is inferred from its positions and so is never
/// rejected.
pub(crate) fn check_variance<'run>(
    ctx: &CompileCtx<'run>,
    er: AdtRef<'run>,
) -> Result<(), AstError> {
    let def = ctx.get_enum(er);
    for param in &def.type_params {
        let mut occ = Occurrence::default();
        for ty in def.variants.iter().filter_map(|v| v.payload.get()) {
            param_polarity(ctx, ty, param.id, Sign::Pos, &mut occ);
        }
        // An absent annotation is inferred (always sound); only an explicit one
        // can contradict the positions.
        let sound = match param.variance {
            _ if !param.explicit_variance => true,
            Variance::Invariant => true,
            Variance::Covariant => !occ.neg,
            Variance::Contravariant => !occ.pos,
        };
        if !sound {
            return Err(AstError::UnsoundVariance {
                type_name: def.name.clone(),
                param: param.name.clone(),
                range: param.range,
            });
        }
    }
    Ok(())
}

/// The polarities at which a type parameter occurs in a type.
#[derive(Default, Clone, Copy)]
pub(crate) struct Occurrence {
    /// occurs in a covariant (producer) position.
    pos: bool,
    /// occurs in a contravariant (consumer) position.
    neg: bool,
}

/// Polarity of the position currently being descended into.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Sign {
    Pos,
    Neg,
    /// invariant: both producer and consumer (e.g. under an invariant
    /// constructor parameter); an occurrence here counts as both.
    Inv,
}

impl Sign {
    /// Flip producer <-> consumer (invariant is its own dual).
    fn flip(self) -> Sign {
        match self {
            Sign::Pos => Sign::Neg,
            Sign::Neg => Sign::Pos,
            Sign::Inv => Sign::Inv,
        }
    }

    /// Compose this outer polarity with the declared `variance` of the
    /// constructor parameter being descended through.
    fn compose(self, variance: Variance) -> Sign {
        match variance {
            Variance::Covariant => self,
            Variance::Contravariant => self.flip(),
            Variance::Invariant => Sign::Inv,
        }
    }
}

/// Accumulate the polarities at which `id` occurs in `ty`, given the polarity
/// `sign` of `ty`'s own position.
pub(crate) fn param_polarity<'run>(
    ctx: &CompileCtx<'run>,
    ty: Ty<'run>,
    id: TypeParamId,
    sign: Sign,
    occ: &mut Occurrence,
) {
    let record = |occ: &mut Occurrence| match sign {
        Sign::Pos => occ.pos = true,
        Sign::Neg => occ.neg = true,
        Sign::Inv => {
            occ.pos = true;
            occ.neg = true;
        }
    };
    match ty.kind() {
        TyKind::Param(p) => {
            if *p == id {
                record(occ);
            }
        }
        // A higher-kinded use `f<..>`: a `f == id` head occurrence counts at the
        // current polarity; its arguments' variance is unknown (the bound
        // constructor is not yet fixed), so they are treated invariantly.
        TyKind::ParamApp(p, args) => {
            if *p == id {
                record(occ);
            }
            for a in args.iter() {
                param_polarity(ctx, *a, id, Sign::Inv, occ);
            }
        }
        TyKind::Tuple(elems) => {
            for e in elems.iter() {
                param_polarity(ctx, *e, id, sign, occ);
            }
        }
        // `F<..>`: compose the current polarity with each of `F`'s declared
        // parameter variances (Calculus: Types, variance; nested composition).
        TyKind::App(er, args, _) => {
            let params = &ctx.get_enum(*er).type_params;
            for (i, a) in args.iter().enumerate() {
                let v = params
                    .get(i)
                    .map(|p| p.variance)
                    .unwrap_or(Variance::Covariant);
                param_polarity(ctx, *a, id, sign.compose(v), occ);
            }
        }
        // References / pointers are covariant in their pointee.
        TyKind::Region(inner, _)
        | TyKind::Ref(_, inner)
        | TyKind::RefMut(_, inner)
        | TyKind::Ptr(inner) => param_polarity(ctx, *inner, id, sign, occ),
        // A function is contravariant in its argument, covariant in its result.
        TyKind::Fn(a, r, _) => {
            param_polarity(ctx, *a, id, sign.flip(), occ);
            param_polarity(ctx, *r, id, sign, occ);
        }
        _ => {}
    }
}
