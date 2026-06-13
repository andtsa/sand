//! build an AST from the Pair tree
#![allow(clippy::result_large_err)]

use std::num::ParseIntError;

use pest::Parser;
use pest::error::Error;
use pest::iterators::Pair;
use thiserror::Error;

use crate::compiler::context::CompileCtx;
use crate::compiler::context::ContextError;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::RegionParamSpec;
use crate::compiler::structure::TypeParamSpec;
use crate::compiler::structure::UriError;
use crate::internal_bug;
use crate::ir_types::hhir::*;
use crate::lang::intrinsics;
use crate::lang::ops::*;
use crate::lang::types::*;
use crate::passes::parse::LangParser;
use crate::passes::parse::Rule;

#[derive(Debug, Error)]
pub enum AstError {
    #[error("parse error: {0}")]
    Pest(#[from] Box<Error<Rule>>),

    #[error("unexpected rule: expected {expected:?}, got {got:?} at {range}")]
    UnexpectedRule {
        expected: &'static str,
        got: Rule,
        range: Range,
    },

    #[error("missing {expected:?} at {range}")]
    Missing {
        expected: &'static str,
        range: Range,
    },

    #[error("invalid integer literal: {got} at {range} ({source})")]
    InvalidInteger {
        got: String,
        range: Range,
        source: ParseIntError,
    },

    #[error("invalid name: {got} at {range}")]
    InvalidName { got: String, range: Range },

    #[error("context error: {0}")]
    ContextError(#[from] ContextError),

    #[error(transparent)]
    UriError(#[from] UriError),

    #[error("unknown type '{name}' at {range}")]
    UnknownType { name: String, range: Range },

    #[error("unknown module '{module}' at {range}")]
    UnknownModule { module: String, range: Range },

    #[error(
        "unknown lifetime '{name}' at {range}: declare it as a region parameter, e.g. `<'{name}>`"
    )]
    UnknownRegion { name: String, range: Range },

    #[error(
        "generic type '{name}' expects {expected} type argument(s) but {found} were given at {range}"
    )]
    TypeArgArityMismatch {
        name: String,
        expected: usize,
        found: usize,
        range: Range,
    },

    #[error(
        "type argument for parameter '{param}' of '{type_name}' has kind {found:?}, but kind {expected:?} is required at {range}"
    )]
    KindArgMismatch {
        type_name: String,
        param: String,
        expected: Kind,
        found: Kind,
        range: Range,
    },

    #[error(
        "parameter '{param}' of '{type_name}' is declared contravariant but appears in a covariant (producer) position at {range}"
    )]
    UnsoundVariance {
        type_name: String,
        param: String,
        range: Range,
    },
}

trait AstExt<T> {
    /// If the Option is None, produce a `Missing` error located at
    /// `start..end`.
    fn missing(self, expecting: &'static str, range: Range) -> Result<T, AstError>
    where
        Self: Sized;
}

impl<T> AstExt<T> for Option<T> {
    fn missing(self, expecting: &'static str, range: Range) -> Result<T, AstError>
    where
        Self: Sized,
    {
        match self {
            Some(v) => Ok(v),
            None => Err(AstError::Missing {
                expected: expecting,
                range,
            }),
        }
    }
}

impl<'run> ProgramModule<'run> {
    pub fn parse_source_file(
        ctx: &mut CompileCtx<'run>,
        src: &str,
        file: FileRef,
    ) -> Result<Vec<ProgramModule<'run>>, AstError> {
        let mut pairs = LangParser::parse(Rule::program, src).map_err(Box::new)?;

        let program_pair = match pairs.next() {
            Some(p) => p,
            None => {
                return Err(AstError::Missing {
                    expected: "root node",
                    range: Range::new(1, 1, 1, 1),
                });
            }
        };

        let dm = ctx.default_module(file);

        let map = build_program(ctx, program_pair, src, dm, file)?;
        Ok(map
            .into_iter()
            .map(|(module_name, functions)| ProgramModule {
                functions,
                module_name,
            })
            .collect::<Vec<_>>())
    }

    pub fn parse_stub(ctx: &mut CompileCtx<'run>, src: &str) -> Result<Self, AstError> {
        let fr = ctx.stub_file();
        let modules = Self::parse_source_file(ctx, src, fr)?;
        if modules.len() == 1 {
            Ok(modules.into_iter().next().unwrap())
        } else {
            Err(AstError::UnexpectedRule {
                expected: "exactly one module",
                got: Rule::program,
                range: Range::new(1, 1, 1, 1),
            })
        }
    }
}

// ============== top level ==============

pub fn build_program<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
    default_module: ModuleRef<'run>,
    file: FileRef,
) -> Result<Map<ModuleRef<'run>, Vec<Function<'run>>>, AstError> {
    assert_eq!(pair.as_rule(), Rule::program);

    // Collect all top-level children so we can do two passes.
    let children: Vec<Pair<Rule>> = pair.into_inner().collect();

    // first pass: register enum type declarations (phase 1 — names only).
    // we also track the current module as module declarations are encountered,
    // so that enum defs are attributed to the correct module.
    //
    // phase 1 deliberately allocates every `EnumRef` and variant name (with
    // `payload: None`) before any payload type annotation is resolved: a
    // payload may reference another enum — including forward or recursive
    // references (`type Tree = Leaf | Node((Tree, Tree))`) — so every
    // `EnumRef` must exist before `build_type` runs on any payload. We stash
    // the raw payload `type_` pairs (cloned — they borrow from `src`) here
    // and resolve them in phase 1.5 below, once every enum skeleton exists.
    let mut pending_payloads: Vec<(EnumRef, usize, Pair<Rule>)> = Vec::new();
    let mut generic_enums: Vec<EnumRef> = Vec::new();
    {
        let mut cur_mod = default_module;
        for child in &children {
            match child.as_rule() {
                Rule::module => {
                    let child_span = child.as_span();
                    let modname_pair = child
                        .clone()
                        .into_inner()
                        .next()
                        .missing("module name", Range::from(child_span))?;
                    cur_mod = ctx
                        .get_mod_by_name(modname_pair.as_str())
                        .unwrap_or_else(|| ctx.register_module(modname_pair.as_str(), file));
                }
                Rule::type_alias => {
                    let range = Range::from(child);
                    let mut inner = child.clone().into_inner();
                    let name_pair = inner.next().missing("enum name", range)?;
                    let enum_name = name_pair.as_str().to_string();

                    // optional type and region parameters: `type Ref<'r, T> =
                    // ...`. Allocate them now so phase 1.5 can resolve `T` and
                    // `'r` in payloads.
                    let (type_params, region_params) =
                        if inner.peek().map(|p| p.as_rule()) == Some(Rule::type_params) {
                            let tp_pair = inner.next().missing("type parameters", range)?;
                            let type_params =
                                ctx.begin_type_params(&collect_type_params(tp_pair.clone()));
                            let region_params =
                                ctx.begin_region_params(&collect_region_params(tp_pair));
                            (type_params, region_params)
                        } else {
                            (Vec::new(), Vec::new())
                        };

                    // enum_variant = { identifier ~ ("(" ~ type_ ~ ")")? }
                    let mut variant_names = Vec::new();
                    let mut variant_payloads = Vec::new();
                    for variant_pair in inner {
                        assert_eq!(variant_pair.as_rule(), Rule::enum_variant);
                        let v_range = Range::from(&variant_pair);
                        let mut v_inner = variant_pair.into_inner();
                        let v_name = v_inner
                            .next()
                            .missing("variant name", v_range)?
                            .as_str()
                            .to_string();
                        let payload_pair = v_inner.next();
                        variant_names.push(v_name);
                        variant_payloads.push(payload_pair);
                    }

                    let is_generic = !type_params.is_empty();
                    let er = ctx.register_enum(
                        &enum_name,
                        variant_names,
                        type_params,
                        region_params,
                        range,
                        cur_mod,
                    )?;
                    if is_generic {
                        generic_enums.push(er);
                    }
                    for (idx, payload_pair) in variant_payloads.into_iter().enumerate() {
                        if let Some(p) = payload_pair {
                            pending_payloads.push((er, idx, p));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // phase 1.5: resolve every variant's payload type annotation now that all
    // `EnumRef`s exist (so forward/recursive payload types resolve correctly).
    // Each payload is resolved with its own enum's type parameters in scope, so
    // a `T` in `type Option<T> = Some(T)` resolves to that enum's `Ty::Param`.
    for (er, idx, payload_pair) in pending_payloads {
        let params = ctx.get_enum(er).type_params.clone();
        let region_params = ctx.get_enum(er).region_params.clone();
        ctx.enter_type_param_scope(&params);
        ctx.enter_region_param_scope(&region_params);
        let payload_ty = build_type(ctx, payload_pair)?;
        ctx.set_variant_payload(er, idx, payload_ty);
    }
    ctx.end_type_params();

    // phase 1.6: validate declared variance against the positions each
    // parameter occupies in the enum's variant payloads (Calculus §2.1).
    for er in generic_enums {
        check_variance(ctx, er)?;
    }

    // second pass: build functions
    let mut mods: Map<ModuleRef, Vec<Function>> = Map::new();
    let mut funcs = Vec::new();
    let mut current_module = default_module;
    for child in children {
        match child.as_rule() {
            Rule::module => {
                let child_span = child.as_span();
                let modname_pair = child
                    .into_inner()
                    .next()
                    .missing("module name", Range::from(child_span))?;
                let mod_span = modname_pair.as_span();
                if modname_pair.as_rule() != Rule::identifier {
                    return Err(AstError::UnexpectedRule {
                        expected: "identifier",
                        got: modname_pair.as_rule(),
                        range: Range::from(&modname_pair),
                    });
                }
                // flush accumulated functions into the current module slot
                if !funcs.is_empty() {
                    mods.entry(current_module).or_default().append(&mut funcs);
                }
                current_module = ctx
                    .get_mod_by_name(mod_span.as_str())
                    .unwrap_or_else(|| ctx.register_module(mod_span.as_str(), file));
            }
            Rule::function => {
                funcs.push(build_function(ctx, child, src, &current_module)?);
            }
            // type_alias declarations were handled in the first pass
            Rule::type_alias => {}
            // ignore start/end markers
            Rule::EOI => continue,
            other => {
                let range = Range::from(child);
                eprintln!("parse error: unexpected top-level rule at {range} - got {other:?}");
                return Err(AstError::UnexpectedRule {
                    expected: "function or module declaration",
                    got: other,
                    range,
                });
            }
        }
    }

    if !funcs.is_empty() {
        mods.entry(current_module).or_default().append(&mut funcs);
    }

    Ok(mods)
}

fn build_function<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
    cur_module: &ModuleRef<'run>,
) -> Result<Function<'run>, AstError> {
    // keep the build-module hint up to date so that anonymous tag-union types
    // declared in `build_type` are attributed to the right module.
    ctx.set_build_module(*cur_module);
    let range = Range::from(&pair);
    if pair.as_rule() != Rule::function {
        return Err(AstError::UnexpectedRule {
            expected: "function",
            got: pair.as_rule(),
            range,
        });
    }

    let mut inner = pair.into_inner();

    // order in grammar: identifier, (parameter | parameters)? , type_, expression
    // first child must be identifier
    let name_pair = inner.next().missing("function name", range)?;
    let name_range = Range::from(&name_pair);
    if name_pair.as_rule() != Rule::identifier {
        return Err(AstError::UnexpectedRule {
            expected: "identifier",
            got: name_pair.as_rule(),
            range: name_range,
        });
    }
    let name = name_pair.as_str().to_string();

    // make sure we aren't redefining internal functions
    if !intrinsics::fn_name_allowed(&name) {
        return Err(AstError::InvalidName {
            got: name,
            range: name_range,
        });
    }

    // optional type and region parameters: `def f<'r, T, U>(...)`. Scoping them
    // here means `build_type` resolves `T`/`U` to `Ty::Param` and `'r` to its
    // region for the rest of this function's signature and body.
    let (type_params, region_params) =
        if inner.peek().map(|p| p.as_rule()) == Some(Rule::type_params) {
            let tp_pair = inner.next().missing("type parameters", range)?;
            let type_params = ctx.begin_type_params(&collect_type_params(tp_pair.clone()));
            let region_params = ctx.begin_region_params(&collect_region_params(tp_pair));
            (type_params, region_params)
        } else {
            (ctx.begin_type_params(&[]), ctx.begin_region_params(&[]))
        };

    // collect optional parameters (parameter or parameters)
    let mut parameters = Vec::new();
    loop {
        let peek = inner.peek().map(|p| p.as_rule());
        match peek {
            Some(Rule::parameter) => {
                let p = inner.next().missing("parameter", range)?;
                for pp in p.into_inner() {
                    parameters.push(build_parameter(ctx, pp)?);
                }
            }
            Some(Rule::parameters) => {
                let p = inner.next().missing("parameter", range)?;
                for pp in p.into_inner() {
                    parameters.push(build_parameter(ctx, pp)?);
                }
            }
            _ => break,
        }
    }

    // next should be type_
    let ty_pair = match inner.next() {
        Some(p) => {
            if p.as_rule() != Rule::type_ {
                return Err(AstError::UnexpectedRule {
                    expected: "type_",
                    got: p.as_rule(),
                    range: Range::from(&p),
                });
            }
            p
        }
        None => {
            return Err(AstError::UnexpectedRule {
                expected: "type_",
                got: Rule::program,
                range,
            });
        }
    };
    let ret_type = build_type(ctx, ty_pair)?;

    // optional `where 'r >= 's` outlives constraints (resolved while the
    // function's region parameters are still in scope).
    let where_constraints = if inner.peek().map(|p| p.as_rule()) == Some(Rule::where_clause) {
        let wc_pair = inner.next().missing("where clause", range)?;
        build_where_clause(ctx, wc_pair)?
    } else {
        Vec::new()
    };

    // final child is the function body expression
    let body_pair = inner.next().missing("function body expression", range)?;
    let body = build_expr(ctx, body_pair, src)?;

    let ofref = ctx.register_function(&name_pair, cur_module)?;
    ctx.end_type_params();

    Ok(Function {
        name: ofref,
        range: Range::from(name_pair),
        type_params,
        region_params,
        where_constraints,
        parameters,
        ret_type,
        body,
    })
}

/// Build the outlives constraints from a `where 'r >= 's, ...` clause. Both
/// lifetimes must already be in scope (declared region parameters or
/// `'static`).
fn build_where_clause(
    ctx: &CompileCtx<'_>,
    pair: Pair<Rule>,
) -> Result<Vec<RegionConstraint>, AstError> {
    assert_eq!(pair.as_rule(), Rule::where_clause);
    pair.into_inner()
        .map(|wc| {
            // where_constraint = { lifetime ~ ">=" ~ lifetime }
            let range = Range::from(&wc);
            let mut parts = wc.into_inner();
            let longer = resolve_lifetime(ctx, &parts.next().missing("lifetime", range)?)?;
            let shorter = resolve_lifetime(ctx, &parts.next().missing("lifetime", range)?)?;
            Ok(RegionConstraint { longer, shorter })
        })
        .collect()
}

/// Validate the declared variance of a generic enum's parameters against the
/// positions they occupy in its variant payloads (Calculus §2.1).
///
/// Every position in the current type grammar (enum payloads, tuples, generic
/// applications) is a *producer* (covariant) position — there are no consumer
/// positions until function types arrive. So a parameter that is used is
/// covariant, and the only unsound declaration is `Contravariant` on a used
/// parameter. `Covariant` and `Invariant` are always sound, and an unused
/// (phantom) parameter accepts any declared variance.
fn check_variance<'run>(ctx: &CompileCtx<'run>, er: EnumRef<'run>) -> Result<(), AstError> {
    let def = ctx.get_enum(er);
    for param in &def.type_params {
        let used = def
            .variants
            .iter()
            .filter_map(|v| v.payload.get())
            .any(|ty| ty_mentions_param(ty, param.id));
        if used && param.variance == Variance::Contravariant {
            return Err(AstError::UnsoundVariance {
                type_name: def.name.clone(),
                param: param.name.clone(),
                range: param.range,
            });
        }
    }
    Ok(())
}

/// Whether `ty` mentions the type parameter `id` (directly or nested).
fn ty_mentions_param(ty: Ty<'_>, id: TypeParamId) -> bool {
    match ty.kind() {
        TyKind::Param(p) => *p == id,
        TyKind::Tuple(elems) => elems.iter().any(|e| ty_mentions_param(*e, id)),
        TyKind::App(_, args) => args.iter().any(|a| ty_mentions_param(*a, id)),
        TyKind::Region(inner, _) | TyKind::Ref(_, inner) | TyKind::RefMut(_, inner) => {
            ty_mentions_param(*inner, id)
        }
        _ => false,
    }
}

/// Parse each `type_param` in a `type_params` pair, applying the default
/// variance (`Covariant`) and kind (`Owned`) when their annotations are absent.
/// Region parameters in the same `<...>` list are handled by
/// [`collect_region_params`] and skipped here.
fn collect_type_params(pair: Pair<Rule>) -> Vec<TypeParamSpec> {
    assert_eq!(pair.as_rule(), Rule::type_params);
    pair.into_inner()
        .filter(|p| p.as_rule() == Rule::type_param)
        .map(|tp| {
            // type_param = { variance_ann? ~ identifier ~ (":" ~ kind_ann)? }
            let range = Range::from(&tp);
            let mut variance = Variance::Covariant;
            let mut kind = Kind::Owned;
            let mut name = String::new();
            for part in tp.into_inner() {
                match part.as_rule() {
                    Rule::variance_ann => {
                        variance = match part.as_str() {
                            "+" => Variance::Covariant,
                            "-" => Variance::Contravariant,
                            _ => Variance::Invariant,
                        };
                    }
                    Rule::identifier => name = part.as_str().to_string(),
                    Rule::kind_ann => {
                        kind = match part.as_str() {
                            "Never" => Kind::Never,
                            _ => Kind::Owned,
                        };
                    }
                    _ => {}
                }
            }
            TypeParamSpec {
                name,
                range,
                variance,
                kind,
            }
        })
        .collect()
}

/// Parse each `region_param` (`'r`) in a `type_params` pair. Type parameters in
/// the same `<...>` list are handled by [`collect_type_params`] and skipped.
fn collect_region_params(pair: Pair<Rule>) -> Vec<RegionParamSpec> {
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

fn build_parameter<'run>(
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
/// scope — `'static` always is, any other name must be a declared region
/// parameter of the enclosing item (`def f<'r>(...)`).
fn resolve_lifetime(ctx: &CompileCtx<'_>, lt: &Pair<Rule>) -> Result<Region, AstError> {
    assert_eq!(lt.as_rule(), Rule::lifetime);
    let name = lt.as_str().trim_start_matches('\'');
    ctx.resolve_region(name)
        .ok_or_else(|| AstError::UnknownRegion {
            name: name.to_string(),
            range: Range::from(lt),
        })
}

/// Build a type, applying an optional `@ 'r` region ascription (Calculus §2.3).
/// `type_ = { core_type ~ ("@" ~ lifetime)? }`.
fn build_type<'run>(ctx: &mut CompileCtx<'run>, pair: Pair<Rule>) -> Result<Ty<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::type_);
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();
    let core = inner.next().missing("core type", range)?;
    let mut ty = build_core_type(ctx, core)?;
    if let Some(lt) = inner.next() {
        let region = resolve_lifetime(ctx, &lt)?;
        ty = ctx.region_ty(ty, region);
    }
    Ok(ty)
}

fn build_core_type<'run>(
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
            // type_application = { identifier ~ "<" ~ type_ ~ ("," ~ type_)* ~ ">" }
            let app_range = Range::from(&inner);
            let mut parts = inner.into_inner();
            let name = parts
                .next()
                .missing("generic type name", app_range)?
                .as_str()
                .to_string();
            let arg_tys = parts
                .map(|p| build_type(ctx, p))
                .collect::<Result<Vec<Ty<'run>>, _>>()?;
            let er = ctx
                .lookup_enum_by_name(&name)
                .ok_or_else(|| AstError::UnknownType {
                    name: name.clone(),
                    range,
                })?;
            let params = ctx.get_enum(er).type_params.clone();
            if params.len() != arg_tys.len() {
                return Err(AstError::TypeArgArityMismatch {
                    name,
                    expected: params.len(),
                    found: arg_tys.len(),
                    range,
                });
            }
            // K-App (Calculus §5): each argument's kind must satisfy the
            // declared parameter kind.
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
            Ok(ctx.intern_app(er, arg_tys))
        }
        Some(inner) if inner.as_rule() == Rule::qualified_type => {
            // qualified_type = { identifier ~ "::" ~ identifier }
            let qrange = Range::from(&inner);
            let mut parts = inner.into_inner();
            let mod_name = parts
                .next()
                .missing("module name in qualified type", qrange)?
                .as_str();
            let type_name = parts
                .next()
                .missing("type name in qualified type", qrange)?
                .as_str();
            let mod_ref = ctx
                .get_mod_by_name(mod_name)
                .ok_or_else(|| AstError::UnknownModule {
                    module: mod_name.to_string(),
                    range,
                })?;
            ctx.lookup_enum_in_module(mod_ref, type_name)
                .map(|er| ctx.enum_ty(er))
                .ok_or_else(|| AstError::UnknownType {
                    name: format!("{mod_name}::{type_name}"),
                    range,
                })
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
                    Ok(ctx.param_ty(id))
                }
                other => ctx
                    .lookup_enum_by_name(other)
                    .map(|er| ctx.enum_ty(er))
                    .ok_or_else(|| AstError::UnknownType {
                        name: other.to_string(),
                        range,
                    }),
            }
        }
    }
}

// === statements ===

fn build_statement<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Statement<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::statement);
    // statement = ((declaration | assignment | expression) ~ ";")
    // capture pair span before moving
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let first = inner.next().missing("statement beginning", range)?;

    let inner_range = Range::from(&first);
    match first.as_rule() {
        Rule::declaration => {
            let mut decl_inner = first.into_inner();
            let first_child = decl_inner.next().missing("declaration body", inner_range)?;

            // Check for constructor-pattern binding: `let E#V(payload) = expr else
            // fallback`
            if first_child.as_rule() == Rule::let_constructor {
                let pattern = build_let_constructor(ctx, first_child)?;
                // Optional type annotation.
                let next = decl_inner
                    .next()
                    .missing("let_constructor declaration body", inner_range)?;
                let (ty, expr_pair) = if next.as_rule() == Rule::type_ {
                    let ty = build_type(ctx, next)?;
                    let ep = decl_inner
                        .next()
                        .missing("let_constructor declaration expression", inner_range)?;
                    (Some(ty), ep)
                } else {
                    (None, next)
                };
                let val = build_expr(ctx, expr_pair, src)?;
                // The `else` expression is mandatory for refutable patterns;
                // the type checker enforces this — here we just require it.
                let else_pair = decl_inner
                    .next()
                    .missing("let_constructor else expression", inner_range)?;
                let else_branch = build_expr(ctx, else_pair, src)?;
                return Ok(Statement::LetPattern {
                    pattern,
                    ty,
                    val,
                    else_branch,
                    range: inner_range,
                });
            }

            // Check for tuple-pattern binding: `let (a, mut b) = expr`
            if first_child.as_rule() == Rule::let_tuple {
                // Parse each element of the tuple pattern.
                let mut elems: Vec<(HirVar, bool, Range)> = Vec::new();
                for elem_pair in first_child.into_inner() {
                    // elem_pair matches `let_tuple_elem = { mut_kw? ~ identifier }`
                    let elem_range = Range::from(&elem_pair);
                    let mut elem_inner = elem_pair.into_inner();
                    let first_elem_child = elem_inner
                        .next()
                        .missing("let_tuple_elem body", elem_range)?;
                    let (is_mutable, ident_pair) = if first_elem_child.as_rule() == Rule::mut_kw {
                        (
                            true,
                            elem_inner
                                .next()
                                .missing("let_tuple_elem identifier", elem_range)?,
                        )
                    } else {
                        (false, first_elem_child)
                    };
                    // Register the element variable (using declaration context).
                    let var =
                        HirVar::Decl(ctx.new_original_variable(&ident_pair, Rule::declaration)?);
                    elems.push((var, is_mutable, elem_range));
                }
                // Optional type annotation, then the RHS expression.
                let next = decl_inner
                    .next()
                    .missing("let_tuple declaration body", inner_range)?;
                let (ty, expr_pair) = if next.as_rule() == Rule::type_ {
                    let ty = build_type(ctx, next)?;
                    let expr_pair = decl_inner
                        .next()
                        .missing("let_tuple declaration expression", inner_range)?;
                    (Some(ty), expr_pair)
                } else {
                    (None, next)
                };
                let expr = build_expr(ctx, expr_pair, src)?;
                return Ok(Statement::LetTuple {
                    elems,
                    ty,
                    val: expr,
                    range: inner_range,
                });
            }

            // Borrow binding `let &x : T = e` (shared) or `let &mut x : T = e`
            // (exclusive) (Calculus §6.4): desugar to `let x : &T = &e` /
            // `let x : &mut T = &mut e`, reusing the borrow-expression
            // machinery — `e` is borrowed (not consumed) and `x` holds the
            // reference. A `&mut` binding is assignable (`x = e` writes through
            // the borrow), so it is marked mutable.
            if first_child.as_rule() == Rule::borrow_binding {
                let mut bb_inner = first_child.into_inner().peekable();
                let mutable = if bb_inner.peek().map(|p| p.as_rule()) == Some(Rule::mut_kw) {
                    bb_inner.next();
                    true
                } else {
                    false
                };
                let name_pair = bb_inner
                    .next()
                    .missing("borrow binding name", inner_range)?;
                let var = HirVar::Decl(ctx.new_original_variable(&name_pair, Rule::declaration)?);
                let next = decl_inner
                    .next()
                    .missing("borrow declaration body", inner_range)?;
                let (ty, expr_pair) = if next.as_rule() == Rule::type_ {
                    let inner_ty = build_type(ctx, next)?;
                    let region = ctx.anon_region();
                    let ref_ty = if mutable {
                        ctx.ref_mut_ty(region, inner_ty)
                    } else {
                        ctx.ref_ty(region, inner_ty)
                    };
                    (
                        Some(ref_ty),
                        decl_inner
                            .next()
                            .missing("borrow declaration expression", inner_range)?,
                    )
                } else {
                    (None, next)
                };
                let inner_expr = build_expr(ctx, expr_pair, src)?;
                let expr_range = inner_expr.range;
                let borrowed = Expr {
                    expr: Expression::Borrow(Box::new(inner_expr), mutable),
                    range: expr_range,
                };
                return Ok(Statement::Declaration {
                    name: var,
                    range: inner_range,
                    ty,
                    is_mutable: mutable,
                    val: borrowed,
                });
            }

            // Regular single-binding declaration.
            let (is_mutable, name_pair) = if first_child.as_rule() == Rule::mut_kw {
                (
                    true,
                    decl_inner.next().missing("declaration name", inner_range)?,
                )
            } else {
                (false, first_child)
            };
            let var = HirVar::Decl(ctx.new_original_variable(&name_pair, Rule::declaration)?);
            tracing::trace!("declaration name: {}", name_pair.as_str());
            let next = decl_inner.next().missing("declaration body", inner_range)?;
            let (ty, expr_pair) = if next.as_rule() == Rule::type_ {
                let ty = build_type(ctx, next)?;
                let expr_pair = decl_inner
                    .next()
                    .missing("declaration expression", inner_range)?;
                (Some(ty), expr_pair)
            } else {
                (None, next)
            };
            let expr = build_expr(ctx, expr_pair, src)?;
            Ok(Statement::Declaration {
                name: var,
                range: inner_range,
                ty,
                is_mutable,
                val: expr,
            })
        }
        Rule::assignment => {
            let mut a_inner = first.into_inner();
            let name = a_inner
                .next()
                .missing("assignment name", inner_range)?
                .as_str()
                .to_string(); // identifier
            let expr = build_expr(
                ctx,
                a_inner.next().missing("assignment type", inner_range)?,
                src,
            )?;
            Ok(Statement::Assignment {
                name: HirVar::Unqualified(name),
                range: inner_range,
                val: expr,
            })
        }
        Rule::expression => {
            let expr = build_expr(ctx, first, src)?;
            Ok(Statement::Expr(expr))
        }
        other => {
            // use the statement pair span for location
            Err(AstError::UnexpectedRule {
                expected: "declaration | assignment | expression",
                got: other,
                range: inner_range,
            })
        }
    }
}

// === expressions ===
// rule hierarchy: expression -> logic_or -> logic_xor -> logic_and -> equality
// -> comparison -> add_sub -> mul_div -> power -> unary -> primary

fn build_expr<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    match pair.as_rule() {
        Rule::expression => {
            // expression wraps logic_or
            let inner = pair.into_inner().next().missing("expression body", range)?;
            build_expr(ctx, inner, src)
        }
        Rule::logic_or => build_logic_or(ctx, pair, src),
        Rule::logic_xor => build_logic_xor(ctx, pair, src),
        Rule::logic_and => build_logic_and(ctx, pair, src),
        Rule::equality => build_equality(ctx, pair, src),
        Rule::comparison => build_comparison(ctx, pair, src),
        Rule::add_sub => build_add_sub(ctx, pair, src),
        Rule::mul_div => build_mul_div(ctx, pair, src),
        Rule::power => build_power(ctx, pair, src),
        Rule::unary => build_unary(ctx, pair, src),
        Rule::primary => build_primary(ctx, pair, src),
        other => Err(AstError::UnexpectedRule {
            expected: "expression-like rule",
            got: other,
            range,
        }),
    }
}

// generic left-assoc binary fold helper
fn binop_fold<'run, F>(
    ctx: &mut CompileCtx<'run>,
    mut inner: pest::iterators::Pairs<'_, Rule>,
    mut next_level: F,
    src: &str,
    parent_range: Range,
) -> Result<Expr<'run>, AstError>
where
    F: FnMut(&mut CompileCtx<'run>, Pair<Rule>, &str) -> Result<Expr<'run>, AstError>,
{
    let first_pair = inner.next().missing("left operand", parent_range)?;
    let mut expr = next_level(ctx, first_pair, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("right operand", parent_range)?;
        let rhs = next_level(ctx, rhs_pair, src)?;
        let op = bop_from_rule(op_pair.as_rule());

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range: parent_range,
        };
    }

    Ok(expr)
}

// mapping only for token rules used in these folds (or/xor/and)
// other operators handled in their specific builders
fn bop_from_rule(rule: Rule) -> Bop {
    match rule {
        Rule::or => Bop::Or,
        Rule::xor => Bop::Xor,
        Rule::logand => Bop::And,
        Rule::bitand => Bop::BitAnd,
        _ => internal_bug!("unexpected bop_from_rule: {rule:?}"),
    }
}

// logic_or = { logic_xor ~ (or ~ logic_xor)* }
fn build_logic_or<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_logic_xor, src, range)
}

// logic_xor = { logic_and ~ (xor ~ logic_and)* }
fn build_logic_xor<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_logic_and, src, range)
}

// logic_and = { equality ~ (and ~ equality)* }
fn build_logic_and<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_equality, src, range)
}

// equality = { comparison ~ ( (eq | ne) ~ comparison )* }
fn build_equality<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_comparison(ctx, inner.next().missing("eq expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("eq right", range)?;
        let rhs = build_comparison(ctx, rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::eq => Bop::Comp(CompOp::Eq),
            Rule::ne => Bop::Comp(CompOp::Ne),
            other => internal_bug!("unexpected equality operator: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// comparison = { add_sub ~ ( (gt | lt | ge | le) ~ add_sub )* }
fn build_comparison<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_add_sub(ctx, inner.next().missing("comp expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("comp right", range)?;
        let rhs = build_add_sub(ctx, rhs_pair, src)?;
        let comp_op = match op_pair.as_rule() {
            Rule::gt => CompOp::Gt,
            Rule::lt => CompOp::Lt,
            Rule::ge => CompOp::Ge,
            Rule::le => CompOp::Le,
            other => internal_bug!("unexpected comp operator: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op: Bop::Comp(comp_op),
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// add_sub = { mul_div ~ ( (add | subtract) ~ mul_div )* }
fn build_add_sub<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_mul_div(ctx, inner.next().missing("add_sub expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("add_sub right", range)?;
        let rhs = build_mul_div(ctx, rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::add => Bop::Plus,
            Rule::subtract => Bop::Minus,
            other => internal_bug!("unexpected add_sub op: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// mul_div = { power ~ ( (multiply | divide) ~ power )* }
fn build_mul_div<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_power(ctx, inner.next().missing("mul_div expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("mul_div right", range)?;
        let rhs = build_power(ctx, rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::multiply => Bop::Mult,
            Rule::divide => Bop::Div,
            other => internal_bug!("unexpected mul_div op: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// power = { unary ~ (pow ~ power)? }  -> right-assoc
fn build_power<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let left_pair = inner.next().missing("power expression", range)?;
    let left = build_unary(ctx, left_pair, src)?;

    if let Some(_op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("power right", range)?;
        let rhs = build_power(ctx, rhs_pair, src)?;
        Ok(Expr {
            expr: Expression::BinOp {
                left: Box::new(left),
                op: Bop::Pow,
                right: Box::new(rhs),
            },
            range,
        })
    } else {
        Ok(left)
    }
}

// unary = { (unary_operand ~ unary) | primary }
fn build_unary<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::unary);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let first = inner.next().missing("unary expr", range)?;

    match first.as_rule() {
        Rule::unary_operand => {
            let op_pair = first.into_inner().next().missing("unary operator", range)?;
            let rhs = build_unary(ctx, inner.next().missing("unary rhs", range)?, src)?;

            let op = match op_pair.as_rule() {
                Rule::subtract => Uop::Neg,
                Rule::negate => Uop::Not,
                other => {
                    return Err(AstError::UnexpectedRule {
                        expected: "subtract | negate",
                        got: other,
                        range: Range::from(&op_pair),
                    });
                }
            };

            Ok(Expr {
                expr: Expression::UnOp {
                    op,
                    right: Box::new(rhs),
                },
                range,
            })
        }
        Rule::subtract => {
            let rhs = build_unary(ctx, inner.next().missing("subtract rhs", range)?, src)?;
            Ok(Expr {
                expr: Expression::UnOp {
                    op: Uop::Neg,
                    right: Box::new(rhs),
                },
                range,
            })
        }
        Rule::negate => {
            let rhs = build_unary(ctx, inner.next().missing("negate rhs", range)?, src)?;
            Ok(Expr {
                expr: Expression::UnOp {
                    op: Uop::Not,
                    right: Box::new(rhs),
                },
                range,
            })
        }
        _ => build_primary(ctx, first, src),
    }
}

fn build_primary<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::primary);
    let range = Range::from(&pair);

    let s = pair.as_str();
    if s.starts_with('{') {
        let mut statements = Vec::new();
        let mut expr: Option<Box<Expr>> = None;

        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::statement => statements.push(build_statement(ctx, inner, src)?),
                Rule::expression => expr = Some(Box::new(build_expr(ctx, inner, src)?)),
                other => {
                    return Err(AstError::UnexpectedRule {
                        expected: "statement | expression in block",
                        got: other,
                        range: Range::from(&inner),
                    });
                }
            }
        }

        return Ok(Expr {
            expr: Expression::Block { statements, expr },
            range,
        });
    }

    let inner = pair
        .into_inner()
        .next()
        .missing("inner expression", range)?;
    match inner.as_rule() {
        Rule::borrow_expr => {
            // borrow_expr = { "&" ~ mut_kw? ~ primary }
            let inner_range = Range::from(&inner);
            let mut parts = inner.into_inner().peekable();
            let mutable = if parts.peek().map(|p| p.as_rule()) == Some(Rule::mut_kw) {
                parts.next();
                true
            } else {
                false
            };
            let target = parts
                .next()
                .missing("borrow target expression", inner_range)?;
            let e = build_primary(ctx, target, src)?;
            Ok(Expr {
                expr: Expression::Borrow(Box::new(e), mutable),
                range,
            })
        }
        Rule::deref_expr => {
            // deref_expr = { "*" ~ primary }
            let inner_range = Range::from(&inner);
            let target = inner
                .into_inner()
                .next()
                .missing("dereference target expression", inner_range)?;
            let e = build_primary(ctx, target, src)?;
            Ok(Expr {
                expr: Expression::Deref(Box::new(e)),
                range,
            })
        }
        Rule::expression => build_expr(ctx, inner, src),
        Rule::ifstatement => build_if(ctx, inner, src),
        Rule::whileloop => build_while(ctx, inner, src),
        Rule::function_call | Rule::external_function_call => build_call(ctx, inner, src),
        Rule::external_constructor_expr => {
            // external_constructor_expr = { identifier ~ "::" ~ identifier ~ "#" ~
            // identifier ~ ("(" ~ expression ~ ")")? }
            let inner_range = Range::from(&inner);
            let mut parts = inner.into_inner();
            let mod_name = parts
                .next()
                .missing("module name in external constructor", inner_range)?
                .as_str()
                .to_string();
            let type_name = parts
                .next()
                .missing("type name in external constructor", inner_range)?
                .as_str()
                .to_string();
            let variant = parts
                .next()
                .missing("variant in external constructor", inner_range)?
                .as_str()
                .to_string();
            let payload = parts
                .next()
                .map(|p| build_expr(ctx, p, src))
                .transpose()?
                .map(Box::new);
            Ok(Expr {
                expr: Expression::ExternalConstructor {
                    mod_name,
                    type_name,
                    variant,
                    payload,
                },
                range: inner_range,
            })
        }
        Rule::constructor_expr => {
            // constructor_expr = { identifier ~ "#" ~ identifier ~ ("(" ~ expression ~
            // ")")? }
            let inner_range = Range::from(&inner);
            let mut parts = inner.into_inner();
            let type_name = parts
                .next()
                .missing("constructor type name", inner_range)?
                .as_str()
                .to_string();
            let variant = parts
                .next()
                .missing("constructor variant", inner_range)?
                .as_str()
                .to_string();
            let payload = parts
                .next()
                .map(|p| build_expr(ctx, p, src))
                .transpose()?
                .map(Box::new);
            Ok(Expr {
                expr: Expression::Constructor {
                    type_name,
                    variant,
                    payload,
                },
                range: inner_range,
            })
        }
        Rule::tuple_expr => {
            // tuple_expr = { "(" ~ expression ~ ("," ~ expression)+ ~ ")" }, arity >= 2
            let inner_range = Range::from(&inner);
            let elems = inner
                .into_inner()
                .map(|p| build_expr(ctx, p, src))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr {
                expr: Expression::Tuple(elems),
                range: inner_range,
            })
        }
        Rule::tag_expr => {
            // tag_expr = { "#" ~ identifier ~ ("(" ~ expression ~ ")")? }
            let inner_range = Range::from(&inner);
            let mut children = inner.into_inner();
            let variant = children
                .next()
                .missing("tag variant", inner_range)?
                .as_str()
                .to_string();
            // optional payload expression
            let payload = children
                .next()
                .map(|p| build_expr(ctx, p, src))
                .transpose()?
                .map(Box::new);
            Ok(Expr {
                expr: Expression::Tag { variant, payload },
                range: inner_range,
            })
        }
        Rule::match_expr => build_match(ctx, inner, src),
        Rule::number => {
            let s = inner.as_str().to_string();
            let v = s.parse::<i64>().map_err(|e| AstError::InvalidInteger {
                got: s.clone(),
                range: Range::from(&inner),
                source: e,
            })?;

            Ok(Expr {
                expr: Expression::Int(v),
                range: Range::from(&inner),
            })
        }
        Rule::boolean => {
            let b = match inner.as_str() {
                "true" => true,
                "false" => false,
                other => internal_bug!("invalid boolean literal: {other}"),
            };

            Ok(Expr {
                expr: Expression::Bool(b),
                range: Range::from(&inner),
            })
        }
        Rule::identifier => Ok(Expr {
            expr: Expression::Var(HirVar::Unqualified(inner.as_str().to_string())),
            range: Range::from(&inner),
        }),
        other => Err(AstError::UnexpectedRule {
            expected: "primary inner",
            got: other,
            range: Range::from(&inner),
        }),
    }
}

fn build_if<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::ifstatement);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let cond_pair = inner.next().missing("if condition", range)?;
    let then_pair = inner.next().missing("then branch", range)?;
    let else_pair = inner.next();

    let cond = build_expr(ctx, cond_pair, src)?;
    let then_e = build_expr(ctx, then_pair, src)?;
    let else_e = match else_pair {
        Some(p) => Some(Box::new(build_expr(ctx, p, src)?)),
        None => None,
    };

    Ok(Expr {
        expr: Expression::If {
            cond: Box::new(cond),
            t: Box::new(then_e),
            f: else_e,
        },
        range,
    })
}

fn build_while<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::whileloop);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let cond_pair = inner.next().missing("while condition", range)?;
    let body_pair = inner.next().missing("while body", range)?;

    let cond = build_expr(ctx, cond_pair, src)?;
    let body = build_expr(ctx, body_pair, src)?;

    Ok(Expr {
        expr: Expression::While {
            cond: Box::new(cond),
            body: Box::new(body),
        },
        range,
    })
}

fn build_match<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::match_expr);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let scrutinee_pair = inner.next().missing("match scrutinee", range)?;
    let scrutinee = build_expr(ctx, scrutinee_pair, src)?;

    let mut arms = Vec::new();
    for arm_pair in inner {
        assert_eq!(arm_pair.as_rule(), Rule::match_arm);
        let arm_range = Range::from(&arm_pair);
        let mut arm_inner = arm_pair.into_inner();
        let pattern_pair = arm_inner.next().missing("match arm pattern", arm_range)?;
        let body_pair = arm_inner.next().missing("match arm body", arm_range)?;

        let pattern = build_pattern(ctx, pattern_pair)?;
        let body = build_expr(ctx, body_pair, src)?;
        arms.push(HirMatchArm {
            pattern,
            body,
            range: arm_range,
        });
    }

    Ok(Expr {
        expr: Expression::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        },
        range,
    })
}

/// Parse a `let_constructor` node (the outermost constructor in a `let E#V(...)
/// = ...`).
///
/// `let_constructor = { identifier ~ "#" ~ identifier ~ ("(" ~ let_destructure
/// ~ ")")? }`
fn build_let_constructor<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<HirPattern<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::let_constructor);
    let range = Range::from(&pair);
    let mut parts = pair.into_inner();
    let type_name = parts
        .next()
        .missing("let_constructor type name", range)?
        .as_str()
        .to_string();
    let variant = parts
        .next()
        .missing("let_constructor variant name", range)?
        .as_str()
        .to_string();
    let payload = parts
        .next()
        .map(|p| build_let_destructure(ctx, p))
        .transpose()?
        .map(Box::new);
    Ok(HirPattern::Constructor {
        type_name,
        variant,
        payload,
    })
}

/// Parse a `let_destructure` node: a sub-pattern inside a `let_constructor`.
///
/// `let_destructure = { let_constructor | let_binding_tuple | let_binding_elem
/// }` where `let_binding_elem = { identifier | empty_identifier }` so wildcards
/// (`_`) are allowed.
///
/// All bindings here are **immutable** (no `mut_kw` in sub-patterns).
fn build_let_destructure<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<HirPattern<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::let_destructure);
    let range = Range::from(&pair);
    let inner = pair
        .into_inner()
        .next()
        .missing("let_destructure body", range)?;
    match inner.as_rule() {
        Rule::let_constructor => build_let_constructor(ctx, inner),
        Rule::let_binding_tuple => {
            // let_binding_tuple = { "(" ~ let_binding_elem ~ ("," ~ let_binding_elem)+ ~
            // ")" }
            let elems = inner
                .into_inner()
                .map(|elem| {
                    // let_binding_elem = { identifier | empty_identifier }
                    let r = Range::from(&elem);
                    let child = elem
                        .into_inner()
                        .next()
                        .missing("let_binding_elem body", r)?;
                    match child.as_rule() {
                        Rule::identifier => {
                            let var =
                                HirVar::Decl(ctx.new_original_variable(&child, Rule::declaration)?);
                            Ok(HirPattern::Binding { var, range: r })
                        }
                        Rule::empty_identifier => Ok(HirPattern::Wildcard),
                        other => Err(AstError::UnexpectedRule {
                            expected: "identifier | empty_identifier",
                            got: other,
                            range: r,
                        }),
                    }
                })
                .collect::<Result<Vec<_>, AstError>>()?;
            Ok(HirPattern::Tuple(elems))
        }
        Rule::let_binding_elem => {
            // let_binding_elem = { identifier | empty_identifier }
            let child = inner
                .into_inner()
                .next()
                .missing("let_binding_elem body", range)?;
            match child.as_rule() {
                Rule::identifier => {
                    let var = HirVar::Decl(ctx.new_original_variable(&child, Rule::declaration)?);
                    Ok(HirPattern::Binding { var, range })
                }
                Rule::empty_identifier => Ok(HirPattern::Wildcard),
                other => Err(AstError::UnexpectedRule {
                    expected: "identifier | empty_identifier",
                    got: other,
                    range,
                }),
            }
        }
        other => Err(AstError::UnexpectedRule {
            expected: "let_constructor | let_binding_tuple | let_binding_elem",
            got: other,
            range,
        }),
    }
}

fn build_pattern<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<HirPattern<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::pattern);
    let range = Range::from(&pair);
    let inner = pair.into_inner().next().missing("pattern body", range)?;
    match inner.as_rule() {
        Rule::constructor_pattern => {
            // constructor_pattern = { identifier ~ "#" ~ identifier ~ ("(" ~ pattern ~
            // ")")? }
            let mut parts = inner.into_inner();
            let type_name = parts
                .next()
                .missing("constructor type name", range)?
                .as_str()
                .to_string();
            let variant = parts
                .next()
                .missing("constructor variant name", range)?
                .as_str()
                .to_string();
            let payload = parts
                .next()
                .map(|p| build_pattern(ctx, p))
                .transpose()?
                .map(Box::new);
            Ok(HirPattern::Constructor {
                type_name,
                variant,
                payload,
            })
        }
        Rule::tag_pattern => {
            // tag_pattern = { "#" ~ identifier ~ ("(" ~ pattern ~ ")")? }
            let mut parts = inner.into_inner();
            let variant = parts
                .next()
                .missing("tag pattern variant", range)?
                .as_str()
                .to_string();
            let payload = parts
                .next()
                .map(|p| build_pattern(ctx, p))
                .transpose()?
                .map(Box::new);
            Ok(HirPattern::Tag { variant, payload })
        }
        Rule::tuple_pattern => {
            // tuple_pattern = { "(" ~ pattern ~ ("," ~ pattern)+ ~ ")" }
            let elems = inner
                .into_inner()
                .map(|p| build_pattern(ctx, p))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(HirPattern::Tuple(elems))
        }
        Rule::binding_pattern => {
            // binding_pattern = { identifier }
            let binding_range = Range::from(&inner);
            let name_pair = inner
                .into_inner()
                .next()
                .unwrap_or_else(|| unreachable!("binding_pattern always wraps an identifier"));
            let var = HirVar::Decl(ctx.new_original_variable(&name_pair, Rule::binding_pattern)?);
            Ok(HirPattern::Binding {
                var,
                range: binding_range,
            })
        }
        Rule::wildcard_pattern => Ok(HirPattern::Wildcard),
        Rule::int_literal_pattern => {
            let s = inner.as_str();
            let v = s.parse::<i64>().map_err(|e| AstError::InvalidInteger {
                got: s.to_string(),
                range,
                source: e,
            })?;
            Ok(HirPattern::IntLit(v))
        }
        Rule::bool_literal_pattern => {
            let b = match inner.as_str() {
                "true" => true,
                "false" => false,
                _ => unreachable!("bool_literal_pattern is 'true' | 'false'"),
            };
            Ok(HirPattern::BoolLit(b))
        }
        other => Err(AstError::UnexpectedRule {
            expected: "constructor_pattern | tag_pattern | tuple_pattern | wildcard_pattern | bool_literal_pattern | int_literal_pattern | binding_pattern",
            got: other,
            range,
        }),
    }
}

fn build_call<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let rule = pair.as_rule();
    assert!(matches!(
        rule,
        Rule::function_call | Rule::external_function_call
    ));
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let ext_call = if rule == Rule::external_function_call {
        Some(inner.next().missing("function call module", range)?)
    } else {
        None
    };
    let name_pair = inner.next().missing("function call name", range)?;
    let name = name_pair.as_str().to_string();

    let mut args = Vec::new();
    for expr_pair in inner {
        args.push(build_expr(ctx, expr_pair, src)?);
    }

    let fn_name = if let Some(mod_name) = ext_call {
        HirFnCall::External {
            module: mod_name.as_str().to_string(),
            name,
        }
    } else {
        HirFnCall::Local(name)
    };

    Ok(Expr {
        expr: Expression::Call { fn_name, args },
        range,
    })
}
