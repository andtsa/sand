#![allow(clippy::result_large_err)]

use pest::iterators::Pair;

use super::*;
use crate::compiler::context::CompileCtx;
use crate::compiler::context::DefTarget;
use crate::compiler::structure::Range;
use crate::compiler::structure::RegionParamSpec;
use crate::compiler::structure::TypeConstraint;
use crate::compiler::structure::TypeParamSpec;
use crate::passes::parse::Rule;

/// Extract a parameter's declared type without registering a variable (used for
/// typeclass method signatures, which have no bodies).
pub(crate) fn param_type<'run>(
    ctx: &mut CompileCtx<'run>,
    p: Pair<Rule>,
) -> Result<Ty<'run>, AstError> {
    let range = Range::from(&p);
    let ty_pair = p
        .into_inner()
        .find(|c| c.as_rule() == Rule::type_)
        .missing("parameter type", range)?;
    build_type(ctx, ty_pair)
}

/// Build the outlives constraints from a `where 'r >= 's, ...` clause. Both
/// lifetimes must already be in scope (declared region parameters or
/// `'static`).
#[allow(clippy::type_complexity)]
pub(crate) fn build_where_clause(
    ctx: &mut CompileCtx<'_>,
    pair: Pair<Rule>,
) -> Result<(Vec<RegionConstraint>, Vec<TypeConstraint>), AstError> {
    assert_eq!(pair.as_rule(), Rule::where_clause);
    let mut regions = Vec::new();
    let mut types = Vec::new();
    for wc in pair.into_inner() {
        // where_constraint = { (lifetime ">=" lifetime) | (identifier ":" identifier) }
        let range = Range::from(&wc);
        let mut parts = wc.into_inner();
        let first = parts.next().missing("where constraint", range)?;
        match first.as_rule() {
            Rule::lifetime => {
                let longer = resolve_lifetime(ctx, &first)?;
                let shorter = resolve_lifetime(ctx, &parts.next().missing("lifetime", range)?)?;
                regions.push(RegionConstraint { longer, shorter });
            }
            _ => {
                // `T : Class`: `T` must be a declared type parameter and `Class`
                // a known typeclass.
                let pname = first.as_str();
                let param = ctx
                    .lookup_type_param(pname)
                    .ok_or_else(|| AstError::UnknownType {
                        name: pname.to_string(),
                        range,
                    })?;
                let cpair = parts.next().missing("typeclass name", range)?;
                let crange = Range::from(&cpair);
                let class = ctx.lookup_typeclass(cpair.as_str()).ok_or_else(|| {
                    AstError::UnknownTypeclass {
                        name: cpair.as_str().to_string(),
                        range,
                    }
                })?;
                ctx.record_type_ref(crange, DefTarget::Typeclass(class));
                types.push(TypeConstraint { param, class });
            }
        }
    }
    Ok((regions, types))
}

/// Parse each `type_param` in a `type_params` pair, applying the default
/// variance (`Covariant`) and kind (`Owned`) when their annotations are absent.
/// Region parameters in the same `<...>` list are handled by
/// [`collect_region_params`] and skipped here.
pub(crate) fn collect_type_params(
    ctx: &mut CompileCtx<'_>,
    pair: Pair<Rule>,
) -> Vec<TypeParamSpec> {
    assert_eq!(pair.as_rule(), Rule::type_params);
    pair.into_inner()
        .filter(|p| p.as_rule() == Rule::type_param)
        .map(|tp| {
            // type_param = { variance_ann? ~ identifier ~ (":" ~ kind_ann)? }
            let range = Range::from(&tp);
            let mut variance = Variance::Covariant;
            let mut explicit_variance = false;
            let mut kind = Kind::Owned;
            let mut name = String::new();
            for part in tp.into_inner() {
                match part.as_rule() {
                    Rule::variance_ann => {
                        explicit_variance = true;
                        variance = match part.as_str() {
                            "+" => Variance::Covariant,
                            "-" => Variance::Contravariant,
                            _ => Variance::Invariant,
                        };
                    }
                    Rule::identifier => name = part.as_str().to_string(),
                    Rule::kind_ann => kind = build_kind(ctx, part),
                    _ => {}
                }
            }
            TypeParamSpec {
                name,
                range,
                variance,
                explicit_variance,
                kind,
            }
        })
        .collect()
}

/// Parse a `kind_ann` into a [`Kind`], interning arrow kinds.
/// `kind_ann = { kind_atom ~ ("->" ~ kind_atom)* }`; `->` is right-associative,
/// so `A -> B -> C` is `A -> (B -> C)`.
pub(crate) fn build_kind(ctx: &mut CompileCtx<'_>, pair: Pair<Rule>) -> Kind {
    assert_eq!(pair.as_rule(), Rule::kind_ann);
    let atoms: Vec<Kind> = pair
        .into_inner()
        .filter(|p| p.as_rule() == Rule::kind_atom)
        .map(|a| build_kind_atom(ctx, a))
        .collect();
    let mut it = atoms.into_iter().rev();
    let mut acc = it.next().expect("kind_ann has at least one atom");
    for from in it {
        acc = ctx.intern_kind(from, acc);
    }
    acc
}

/// `kind_atom = { "Owned" | "Never" | "(" ~ kind_ann ~ ")" }`.
pub(crate) fn build_kind_atom(ctx: &mut CompileCtx<'_>, pair: Pair<Rule>) -> Kind {
    assert_eq!(pair.as_rule(), Rule::kind_atom);
    match pair.clone().into_inner().next() {
        Some(inner) if inner.as_rule() == Rule::kind_ann => build_kind(ctx, inner),
        _ => match pair.as_str().trim() {
            "Never" => Kind::Never,
            _ => Kind::Owned,
        },
    }
}

/// Parse each `region_param` (`'r`) in a `type_params` pair. Type parameters in
/// the same `<...>` list are handled by [`collect_type_params`] and skipped.
pub(crate) fn collect_region_params(pair: Pair<Rule>) -> Vec<RegionParamSpec> {
    assert_eq!(pair.as_rule(), Rule::type_params);
    pair.into_inner()
        .filter(|p| p.as_rule() == Rule::region_param)
        .map(|rp| {
            // region_param = { lifetime }
            let range = Range::from(&rp);
            let lt = rp.into_inner().next();
            let name = lt
                .map(|l| l.as_str().trim_start_matches('\'').to_string())
                .unwrap_or_default();
            RegionParamSpec { name, range }
        })
        .collect()
}

pub(crate) fn build_parameter<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<Parameter<'run>, AstError> {
    let rule = pair.as_rule();
    assert_eq!(rule, Rule::parameter);
    // capture span before into_inner
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();
    let first = inner.next().missing("parameter name", range)?;
    let (is_mutable, name) = if first.as_rule() == Rule::mut_kw {
        (true, inner.next().missing("parameter name", range)?)
    } else {
        (false, first)
    };
    let ty_pair = inner.next().missing("parameter type", range)?;
    tracing::trace!("parameter : {} : {}", name.as_str(), ty_pair.as_str());
    let ty = build_type(ctx, ty_pair)?;
    let var = HirVar::Decl(ctx.new_original_variable(&name, rule)?);
    Ok(Parameter {
        name: var,
        ty,
        range,
        is_mutable,
    })
}

/// Resolve a `lifetime` token (`'r`) to its [`Region`]. The region must be in
/// scope. `'static` always is, any other name must be a declared region
/// parameter of the enclosing item (`def f<'r>(...)`).
pub(crate) fn resolve_lifetime(ctx: &CompileCtx<'_>, lt: &Pair<Rule>) -> Result<Region, AstError> {
    assert_eq!(lt.as_rule(), Rule::lifetime);
    let name = lt.as_str().trim_start_matches('\'');
    ctx.resolve_region(name)
        .ok_or_else(|| AstError::UnknownRegion {
            name: name.to_string(),
            range: Range::from(lt),
        })
}

/// Build a type, applying an optional `@ 'r` region ascription (Calculus:
/// Types). `type_ = { fn_type | core_type ~ ("@" ~ lifetime)? }`.
pub(crate) fn build_type<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<Ty<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::type_);
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();
    let first = inner.next().missing("type", range)?;
    if first.as_rule() == Rule::fn_type {
        return build_fn_type(ctx, first);
    }
    let mut ty = build_core_type(ctx, first)?;
    if let Some(lt) = inner.next() {
        let region = resolve_lifetime(ctx, &lt)?;
        ty = ctx.region_ty(ty, region);
    }
    Ok(ty)
}

/// Build a function type `A -> B` / `A -[k]> B`.
/// `fn_type = { core_type ~ fn_arrow ~ type_ }`, right-associative via the
/// codomain.
pub(crate) fn build_fn_type<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<Ty<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::fn_type);
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();
    let dom_pair = inner.next().missing("function domain type", range)?;
    let arrow = inner.next().missing("function arrow", range)?;
    let cod_pair = inner.next().missing("function codomain type", range)?;
    let mode = build_fn_arrow(&arrow);
    let dom = build_core_type(ctx, dom_pair)?;
    let cod = build_type(ctx, cod_pair)?;
    Ok(ctx.fn_ty(dom, cod, mode))
}

/// Parse a function arrow's calling mode.
/// `fn_arrow = { ("-[" ~ arrow_kind ~ "]>") | "->" }`.
pub(crate) fn build_fn_arrow(pair: &Pair<Rule>) -> FnMode {
    assert_eq!(pair.as_rule(), Rule::fn_arrow);
    match pair.clone().into_inner().next() {
        Some(k) if k.as_rule() == Rule::arrow_kind => match k.as_str().trim() {
            "Owned" => FnMode::Consuming,
            "BorrowedMut" => FnMode::ReusableMut,
            _ => FnMode::Reusable,
        },
        _ => FnMode::Reusable,
    }
}

pub(crate) fn build_core_type<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<Ty<'run>, AstError> {
    tracing::trace!("build_core_type called with {:?}", pair.as_str());
    assert_eq!(
        pair.as_rule(),
        Rule::core_type,
        "expected core type, got {:?}: {}",
        pair.as_rule(),
        pair.as_str()
    );
    let range = Range::from(&pair);

    // Check whether the inner token is a qualified_type, a tag_type, or a
    // plain identifier / built-in keyword.
    let inner_opt = pair.clone().into_inner().next();
    match inner_opt {
        Some(inner) if inner.as_rule() == Rule::borrow_type => {
            // borrow_type = { "&" ~ lifetime? ~ mut_kw? ~ core_type }
            let mut parts = inner.into_inner().peekable();
            let region = if parts.peek().map(|p| p.as_rule()) == Some(Rule::lifetime) {
                let lt = parts.next().missing("lifetime", range)?;
                resolve_lifetime(ctx, &lt)?
            } else {
                ctx.anon_region()
            };
            let mutable = if parts.peek().map(|p| p.as_rule()) == Some(Rule::mut_kw) {
                parts.next();
                true
            } else {
                false
            };
            let core_pair = parts.next().missing("borrow target type", range)?;
            let inner_ty = build_core_type(ctx, core_pair)?;
            Ok(if mutable {
                ctx.ref_mut_ty(region, inner_ty)
            } else {
                ctx.ref_ty(region, inner_ty)
            })
        }
        Some(inner) if inner.as_rule() == Rule::tuple_type => {
            // tuple_type = { "(" ~ type_ ~ ("," ~ type_)+ ~ ")" }
            let elem_tys = inner
                .into_inner()
                .map(|p| build_type(ctx, p))
                .collect::<Result<Vec<Ty<'run>>, _>>()?;
            Ok(ctx.intern_tuple(elem_tys))
        }
        Some(inner) if inner.as_rule() == Rule::tag_type => {
            // tag_type = { "#" ~ identifier ~ ("|" ~ "#" ~ identifier)* }
            // The "#" and "|" literals are anonymous; only `identifier` children are
            // captured.
            let tags: Vec<String> = inner.into_inner().map(|p| p.as_str().to_string()).collect();
            let er = ctx.register_or_get_anon_enum(tags, range);
            Ok(ctx.enum_ty(er))
        }
        Some(inner) if inner.as_rule() == Rule::type_application => {
            // type_application = { identifier ~ "<" ~ type_app_arg (~ "," ~ …)* ~ ">" }
            // type_app_arg     = { lifetime | type_ }   (lifetimes first)
            let app_range = Range::from(&inner);
            let mut parts = inner.into_inner();
            let name_pair = parts.next().missing("generic type name", app_range)?;
            let name_range = Range::from(&name_pair);
            let name = name_pair.as_str().to_string();

            // Split args into region args (lifetimes) and type args, enforcing
            // that all lifetimes precede the first type (the lifetimes-first
            // convention, mirroring the declaration order).
            let mut region_args: Vec<Region> = Vec::new();
            let mut arg_tys: Vec<Ty<'run>> = Vec::new();
            for arg in parts {
                // `arg` is a `type_app_arg`; its sole child is a lifetime or a type_.
                let child = arg
                    .into_inner()
                    .next()
                    .missing("type-application argument", range)?;
                match child.as_rule() {
                    Rule::lifetime => {
                        if !arg_tys.is_empty() {
                            return Err(AstError::RegionArgsNotFirst { name, range });
                        }
                        let lt = child.as_str().trim_start_matches('\'');
                        let region =
                            ctx.resolve_region(lt)
                                .ok_or_else(|| AstError::UnknownRegion {
                                    name: lt.to_string(),
                                    range,
                                })?;
                        region_args.push(region);
                    }
                    // A `_` hole is only meaningful in an `impl` head (built
                    // manually by `build_impl`); anywhere else it is an error.
                    Rule::hole => {
                        return Err(AstError::HoleOutsideImplHead {
                            range: Range::from(&child),
                        });
                    }
                    _ => arg_tys.push(build_type(ctx, child)?),
                }
            }

            // A higher-kinded type parameter applied: `F<A>` where `F` is a
            // type-constructor parameter in scope. Unlike a concrete
            // enum application this produces a `ParamApp`, opaque until
            // monomorphisation binds `F` to a concrete constructor.
            if let Some(id) = ctx.lookup_type_param(&name) {
                if !region_args.is_empty() {
                    return Err(AstError::RegionArgArityMismatch {
                        name: name.clone(),
                        expected: 0,
                        found: region_args.len(),
                        range,
                    });
                }
                // Unfold the constructor's arrow kind into its expected argument
                // kinds (currying) and final result kind.
                let mut cur = ctx.type_param_kind(id);
                let mut domains: Vec<Kind> = Vec::new();
                while let Kind::Arrow(aid) = cur {
                    let (from, to) = ctx.kind_arrow(aid);
                    domains.push(from);
                    cur = to;
                }
                if domains.is_empty() {
                    return Err(AstError::NotATypeConstructor {
                        name: name.clone(),
                        range,
                    });
                }
                if domains.len() != arg_tys.len() {
                    return Err(AstError::TypeArgArityMismatch {
                        name: name.clone(),
                        expected: domains.len(),
                        found: arg_tys.len(),
                        range,
                    });
                }
                for (dom, &arg) in domains.iter().zip(&arg_tys) {
                    let arg_kind = ctx.kind_of(arg);
                    if !arg_kind.is_subkind(*dom) {
                        return Err(AstError::KindArgMismatch {
                            type_name: name.clone(),
                            param: "<argument>".to_string(),
                            expected: *dom,
                            found: arg_kind,
                            range,
                        });
                    }
                }
                return Ok(ctx.param_app_ty(id, arg_tys));
            }

            // `Ptr<T>` is a built-in generic primitive, not a user enum:
            // exactly one type argument, no region arguments (a raw
            // pointer is outside the region discipline).
            if name == "Ptr" {
                if !region_args.is_empty() {
                    return Err(AstError::RegionArgArityMismatch {
                        name,
                        expected: 0,
                        found: region_args.len(),
                        range,
                    });
                }
                if arg_tys.len() != 1 {
                    return Err(AstError::TypeArgArityMismatch {
                        name,
                        expected: 1,
                        found: arg_tys.len(),
                        range,
                    });
                }
                return Ok(ctx.ptr_ty(arg_tys[0]));
            }

            let er = ctx
                .lookup_enum_current(&name)
                .ok_or_else(|| AstError::UnknownType {
                    name: name.clone(),
                    range,
                })?;
            ctx.record_type_ref(name_range, DefTarget::Adt(er));
            let params = ctx.get_enum(er).type_params.clone();
            let region_params = ctx.get_enum(er).region_params.clone();
            if params.len() != arg_tys.len() {
                return Err(AstError::TypeArgArityMismatch {
                    name,
                    expected: params.len(),
                    found: arg_tys.len(),
                    range,
                });
            }
            if region_params.len() != region_args.len() {
                return Err(AstError::RegionArgArityMismatch {
                    name,
                    expected: region_params.len(),
                    found: region_args.len(),
                    range,
                });
            }
            // `K-App` (Calculus: Kinding Rules): each argument's kind must
            // satisfy the declared parameter kind.
            for (param, &arg) in params.iter().zip(&arg_tys) {
                let arg_kind = ctx.kind_of(arg);
                if !arg_kind.is_subkind(param.kind) {
                    return Err(AstError::KindArgMismatch {
                        type_name: name,
                        param: param.name.clone(),
                        expected: param.kind,
                        found: arg_kind,
                        range,
                    });
                }
            }
            Ok(ctx.intern_app(er, arg_tys, region_args))
        }
        Some(inner) if inner.as_rule() == Rule::qualified_type => {
            // qualified_type = { identifier ~ "::" ~ identifier }
            let qrange = Range::from(&inner);
            let mut parts = inner.into_inner();
            let mod_name = parts
                .next()
                .missing("module name in qualified type", qrange)?
                .as_str();
            let type_name_pair = parts
                .next()
                .missing("type name in qualified type", qrange)?;
            let type_name_range = Range::from(&type_name_pair);
            let type_name = type_name_pair.as_str();
            let mod_ref = ctx
                .get_mod_by_name(mod_name)
                .ok_or_else(|| AstError::UnknownModule {
                    module: mod_name.to_string(),
                    range,
                })?;
            let er = ctx
                .lookup_enum_in_module(mod_ref, type_name)
                .ok_or_else(|| AstError::UnknownType {
                    name: format!("{mod_name}::{type_name}"),
                    range,
                })?;
            ctx.record_type_ref(type_name_range, DefTarget::Adt(er));
            Ok(ctx.enum_ty(er))
        }
        _ => {
            // Built-in keyword or plain identifier (user-defined enum in same file).
            let name = inner_opt
                .map(|p| p.as_str().to_string())
                .unwrap_or_else(|| pair.as_str().to_string());
            match name.as_str() {
                "Int" => Ok(ctx.types.int),
                "Bool" => Ok(ctx.types.bool),
                "Unit" => Ok(ctx.types.unit),
                // A type parameter in scope (e.g. `T` inside `def f<T>`)
                // shadows any same-named enum and resolves to `Ty::Param`.
                other if ctx.lookup_type_param(other).is_some() => {
                    let id = ctx.lookup_type_param(other).unwrap();
                    // A higher-kinded parameter is a constructor; it cannot stand
                    // alone as a type. It must be applied (`F<T>`).
                    if matches!(ctx.type_param_kind(id), Kind::Arrow(_)) {
                        return Err(AstError::TypeConstructorNotApplied {
                            name: other.to_string(),
                            range,
                        });
                    }
                    Ok(ctx.param_ty(id))
                }
                other => {
                    let er =
                        ctx.lookup_enum_current(other)
                            .ok_or_else(|| AstError::UnknownType {
                                name: other.to_string(),
                                range,
                            })?;
                    // For a bare type name the core_type span *is* the name span.
                    ctx.record_type_ref(range, DefTarget::Adt(er));
                    // A bare name for a *generic* enum is under-applied: it needs
                    // its type/region arguments (`List<T>`, not `List`). Reject it
                    // here with a clear arity error rather than silently producing
                    // a malformed un-applied `Enum` type (which later fails to
                    // unify with the applied form behind a confusing message).
                    let def = ctx.get_enum(er);
                    let tp = def.type_params.len();
                    let rp = def.region_params.len();
                    if tp > 0 {
                        return Err(AstError::TypeArgArityMismatch {
                            name: other.to_string(),
                            expected: tp,
                            found: 0,
                            range,
                        });
                    }
                    if rp > 0 {
                        return Err(AstError::RegionArgArityMismatch {
                            name: other.to_string(),
                            expected: rp,
                            found: 0,
                            range,
                        });
                    }
                    Ok(ctx.enum_ty(er))
                }
            }
        }
    }
}

// === statements ===
