#![allow(clippy::result_large_err)]

use pest::iterators::Pair;

use super::*;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::internal_bug;
use crate::lang::intrinsics;
use crate::passes::parse::Rule;

pub(crate) fn build_functions<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    children: Vec<Pair<'i, Rule>>,
    src: &str,
    default_module: ModuleRef<'run>,
    file: FileRef,
    errors: &mut Vec<AstError>,
) -> BuiltModules<'run> {
    let mut mods: Map<ModuleRef, Vec<Function>> = Map::new();
    let mut funcs = Vec::new();
    let mut current_module = default_module;
    for child in children {
        match child.as_rule() {
            Rule::module => {
                let child_span = child.as_span();
                let modname_pair = match child.into_inner().next() {
                    Some(p) => p,
                    None => {
                        errors.push(AstError::Missing {
                            expected: "module name",
                            range: Range::from(child_span),
                        });
                        continue;
                    }
                };
                let mod_span = modname_pair.as_span();
                if modname_pair.as_rule() != Rule::identifier {
                    errors.push(AstError::UnexpectedRule {
                        expected: "identifier",
                        got: modname_pair.as_rule(),
                        range: Range::from(&modname_pair),
                    });
                    continue;
                }
                // flush accumulated functions into the current module slot
                if !funcs.is_empty() {
                    mods.entry(current_module).or_default().append(&mut funcs);
                }
                current_module = ctx
                    .get_mod_by_name(mod_span.as_str())
                    .unwrap_or_else(|| ctx.register_module(mod_span.as_str(), file));
            }
            Rule::function => match build_function(ctx, child, src, &current_module, None, &[]) {
                Ok(f) => funcs.push(f),
                Err(e) => errors.push(e),
            },
            Rule::extern_decl => {
                if let Err(e) = collect_extern(ctx, child, &current_module) {
                    errors.push(e);
                }
            }
            Rule::impl_decl => {
                if let Err(e) = build_impl(ctx, child, src, &current_module, &mut funcs) {
                    errors.push(e);
                }
            }
            // enum / `use` / typeclass declarations were handled in phase 1
            // (typeclass method *bodies*, the defaults, are built separately).
            Rule::type_alias | Rule::use_decl | Rule::typeclass_decl => {}
            Rule::EOI => continue,
            other => {
                errors.push(AstError::UnexpectedRule {
                    expected: "function or module declaration",
                    got: other,
                    range: Range::from(child),
                });
            }
        }
    }
    if !funcs.is_empty() {
        mods.entry(current_module).or_default().append(&mut funcs);
    }
    mods
}

pub(crate) fn build_function<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
    cur_module: &ModuleRef<'run>,
    name_override: Option<String>,
    // Type parameters already in scope from an enclosing `impl<…>` head (the
    // instance's own parameters). They are the base scope the function's own
    // generics extend, and are prepended to its stored `type_params` so
    // monomorphisation solves them too. Empty for a top-level `def`.
    ambient_type_params: &[TypeParam],
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
    // The ambient `impl<…>` params form the base scope; the function's own
    // generics extend it (fresh ids, same scope). For a top-level `def` the
    // ambient set is empty, so this is equivalent to a plain `begin_type_params`.
    let (own_type_params, region_params) =
        if inner.peek().map(|p| p.as_rule()) == Some(Rule::type_params) {
            let tp_pair = inner.next().missing("type parameters", range)?;
            let specs = collect_type_params(ctx, tp_pair.clone());
            ctx.enter_type_param_scope(ambient_type_params);
            let own = ctx.extend_type_params(&specs);
            let region_params = ctx.begin_region_params(&collect_region_params(tp_pair));
            (own, region_params)
        } else {
            ctx.enter_type_param_scope(ambient_type_params);
            (Vec::new(), ctx.begin_region_params(&[]))
        };
    // Stored generics = ambient (`impl` params) ++ the function's own, so
    // monomorphisation specialises over the instance parameters too.
    let mut type_params = ambient_type_params.to_vec();
    type_params.extend(own_type_params);

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
    let (where_constraints, type_constraints) =
        if inner.peek().map(|p| p.as_rule()) == Some(Rule::where_clause) {
            let wc_pair = inner.next().missing("where clause", range)?;
            build_where_clause(ctx, wc_pair)?
        } else {
            (Vec::new(), Vec::new())
        };

    // final child is the function body expression
    let body_pair = inner.next().missing("function body expression", range)?;
    let body = build_expr(ctx, body_pair, src)?;

    // An impl method is registered under a mangled, collision-free name (so two
    // `impl … { def eq }` blocks don't clash); a top-level function keeps its
    // source name.
    let ofref = match name_override {
        Some(n) => ctx.register_mono_function(n, *cur_module, Range::from(&name_pair)),
        None => ctx.register_function(&name_pair, cur_module)?,
    };
    ctx.end_type_params();

    Ok(Function {
        name: ofref,
        range: Range::from(name_pair),
        type_params,
        region_params,
        where_constraints,
        type_constraints,
        parameters,
        ret_type,
        body,
    })
}

/// Collect one `extern def`: register a bodyless external
/// (FFI) function with a real `FunRef` + `FunSig` so calls resolve through the
/// normal path, and record its C symbol in the extern registry. Parameter and
/// return types must be FFI-safe (`Int`, `Unit`, `Ptr<T>`). No generics.
pub(crate) fn collect_extern<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    cur_module: &ModuleRef<'run>,
) -> Result<(), AstError> {
    assert_eq!(pair.as_rule(), Rule::extern_decl);
    ctx.set_build_module(*cur_module);
    let range = Range::from(&pair);

    // An extern declares no generics; give `build_type` empty param scopes.
    let _tp = ctx.begin_type_params(&[]);
    let _rp = ctx.begin_region_params(&[]);

    let mut inner = pair.into_inner();
    let name_pair = inner.next().missing("extern function name", range)?;
    let name_range = Range::from(&name_pair);
    if name_pair.as_rule() != Rule::identifier {
        return Err(AstError::UnexpectedRule {
            expected: "identifier",
            got: name_pair.as_rule(),
            range: name_range,
        });
    }
    let name = name_pair.as_str().to_string();
    if !intrinsics::fn_name_allowed(&name) {
        return Err(AstError::InvalidName {
            got: name,
            range: name_range,
        });
    }

    // collect parameters (optional), enforcing FFI-safe types
    let mut args: Vec<(UniqVar<'run>, Ty<'run>)> = Vec::new();
    let ret_type;
    loop {
        let peek = inner.peek().map(|p| p.as_rule());
        match peek {
            Some(Rule::parameter) | Some(Rule::parameters) => {
                let p = inner.next().missing("parameter", range)?;
                let param_pairs: Vec<Pair<Rule>> = if p.as_rule() == Rule::parameters {
                    p.into_inner().collect()
                } else {
                    vec![p]
                };
                for pp in param_pairs {
                    let prange = Range::from(&pp);
                    let param = build_parameter(ctx, pp)?;
                    require_ffi_safe(ctx, param.ty, prange)?;
                    let HirVar::Decl(ovref) = param.name else {
                        internal_bug!("build_parameter produced a non-declaration var");
                    };
                    let uv = ctx.uniquify_original_variable(ovref);
                    args.push((uv, param.ty));
                }
            }
            _ => {
                // next token is the return type_
                let ty_pair = inner.next().missing("extern return type", range)?;
                let trange = Range::from(&ty_pair);
                let ty = build_type(ctx, ty_pair)?;
                require_ffi_safe(ctx, ty, trange)?;
                ret_type = ty;
                break;
            }
        }
    }

    let fref = ctx.register_function(&name_pair, cur_module)?;
    ctx.set_fun_sig(
        fref,
        FunSig {
            args,
            ret_ty: ret_type,
            region_params: Vec::new(),
            where_constraints: Vec::new(),
            type_constraints: Vec::new(),
        },
    );
    // C symbol = the sand identifier (no renaming).
    ctx.register_extern(fref, *cur_module, name);

    ctx.end_type_params();
    Ok(())
}
