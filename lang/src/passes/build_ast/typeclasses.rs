#![allow(clippy::result_large_err)]

use pest::iterators::Pair;

use super::*;
use crate::compiler::context::CompileCtx;
use crate::compiler::context::DefTarget;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::ImplDef;
use crate::compiler::structure::Map;
use crate::compiler::structure::MethodDef;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::TypeConstraint;
use crate::compiler::structure::TypeHead;
use crate::compiler::structure::TypeParam;
use crate::compiler::structure::TypeclassDef;
use crate::compiler::structure::TypeclassRef;
use crate::passes::parse::Rule;

/// Phase 1: register a `typeclass` skeleton (name, type parameter, method
/// names, superclass names). Method *signatures* and superclass *refs* resolve
/// later in [`resolve_typeclass_sigs`], once every class + enum skeleton
/// exists.
pub(crate) fn collect_typeclass<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    child: &Pair<'i, Rule>,
    cur_mod: ModuleRef<'run>,
) -> Result<PendingClass<'i>, AstError> {
    let range = Range::from(child);
    let mut inner = child.clone().into_inner();
    let name = inner
        .next()
        .missing("typeclass name", range)?
        .as_str()
        .to_string();

    // grammar requires `type_params`; a class carries exactly one type parameter
    // and no region parameters.
    let tp_pair = inner.next().missing("typeclass type parameter", range)?;
    let type_param_specs = collect_type_params(ctx, tp_pair.clone());
    let region_param_specs = collect_region_params(tp_pair);
    if type_param_specs.len() != 1 || !region_param_specs.is_empty() {
        return Err(AstError::TypeclassParamArity { name, range });
    }
    let class_params = ctx.begin_type_params(&type_param_specs);
    let class_param_id = class_params[0].id;
    ctx.end_type_params();

    let mut superclass_names: Vec<(String, Range)> = Vec::new();
    let mut method_pairs: Vec<Pair<'i, Rule>> = Vec::new();
    let mut method_order: Vec<String> = Vec::new();
    for item in inner {
        match item.as_rule() {
            Rule::requires_clause => {
                for id in item.into_inner() {
                    superclass_names.push((id.as_str().to_string(), Range::from(&id)));
                }
            }
            Rule::typeclass_method => {
                let mrange = Range::from(&item);
                let mname = item
                    .clone()
                    .into_inner()
                    .next()
                    .missing("method name", mrange)?
                    .as_str()
                    .to_string();
                // one class per method name (across all classes, and within one).
                if ctx.method_class(&mname).is_some() || method_order.contains(&mname) {
                    return Err(AstError::DuplicateMethodName {
                        name: mname,
                        range: mrange,
                    });
                }
                method_order.push(mname);
                method_pairs.push(item);
            }
            _ => {}
        }
    }

    let def = TypeclassDef {
        name,
        param: class_param_id,
        superclasses: Vec::new(),
        methods: Map::new(),
        method_order,
        src_module: cur_mod,
        range,
    };
    let tref = ctx.register_typeclass(def);
    Ok(PendingClass {
        tref,
        class_params,
        method_pairs,
        superclass_names,
    })
}

/// Phase 2b: build each class's method signatures (with its type parameter in
/// scope) and resolve its superclass names to refs.
pub(crate) fn resolve_typeclass_sigs<'i>(
    ctx: &mut CompileCtx<'_>,
    pending: Vec<PendingClass<'i>>,
) -> Result<Vec<PendingDefault<'i>>, AstError> {
    let mut defaults = Vec::new();
    for pc in pending {
        // Attribute type/class references in this class's method signatures and
        // `requires` clause to the file the class is declared in.
        let class_module = ctx.get_typeclass(pc.tref).src_module;
        ctx.set_build_module(class_module);
        ctx.enter_type_param_scope(&pc.class_params);
        ctx.begin_region_params(&[]);
        let mut methods = Map::new();
        for mp in &pc.method_pairs {
            let (mname, mdef) = build_method_def(ctx, mp)?;
            if mdef.has_default {
                defaults.push(PendingDefault {
                    class: pc.tref,
                    class_params: pc.class_params.clone(),
                    method: mname.clone(),
                    method_pair: mp.clone(),
                });
            }
            methods.insert(mname, mdef);
        }
        ctx.end_type_params();

        let mut supers = Vec::new();
        for (sname, srange) in &pc.superclass_names {
            let sref = ctx
                .lookup_typeclass(sname)
                .ok_or_else(|| AstError::UnknownSuperclass {
                    name: sname.clone(),
                    range: *srange,
                })?;
            ctx.record_type_ref(*srange, DefTarget::Typeclass(sref));
            supers.push(sref);
        }
        ctx.set_typeclass_methods(pc.tref, methods, supers);
    }
    Ok(defaults)
}

/// A typeclass method with a default body, to be built once as a generic
/// function `<T> where T : C` (see [`build_default_methods`]).
pub(crate) struct PendingDefault<'i> {
    class: TypeclassRef,
    class_params: Vec<TypeParam>,
    method: String,
    method_pair: Pair<'i, Rule>,
}

/// Build each defaulted typeclass method as a generic function over the class
/// parameter (with a `where T : C` constraint so its sibling-method calls
/// type-check), register it, and record it on the method so impls that omit the
/// method dispatch to it. Returns the `(module, function)` pairs to add to the
/// program.
pub(crate) fn build_default_methods<'run>(
    ctx: &mut CompileCtx<'run>,
    defaults: Vec<PendingDefault<'_>>,
    src: &str,
) -> Result<Vec<(ModuleRef<'run>, Function<'run>)>, AstError> {
    let mut out = Vec::new();
    for d in defaults {
        let module = ctx.get_typeclass(d.class).src_module;
        let class_name = ctx.get_typeclass(d.class).name.clone();
        ctx.set_build_module(module);
        ctx.enter_type_param_scope(&d.class_params);
        ctx.begin_region_params(&[]);

        // parse the method header + default body.
        let range = Range::from(&d.method_pair);
        let mut inner = d.method_pair.clone().into_inner();
        let _name = inner.next().missing("method name", range)?;
        let mut parameters = Vec::new();
        if inner.peek().map(|p| p.as_rule()) == Some(Rule::parameters) {
            let ps = inner.next().missing("parameters", range)?;
            for pp in ps.into_inner() {
                parameters.push(build_parameter(ctx, pp)?);
            }
        }
        let ty_pair = inner.next().missing("method return type", range)?;
        let ret_type = build_type(ctx, ty_pair)?;
        // skip an optional method-level where clause, then the body expression.
        let mut body_pair = None;
        for rest in inner {
            if rest.as_rule() == Rule::expression {
                body_pair = Some(rest);
            }
        }
        let body = build_expr(ctx, body_pair.missing("default body", range)?, src)?;

        let mangled = format!("{class_name}$default${}", d.method);
        let fref = ctx.register_mono_function(mangled, module, range);
        ctx.end_type_params();
        ctx.set_method_default_fn(d.class, &d.method, fref);

        out.push((
            module,
            Function {
                name: fref,
                range,
                // generic over the class parameter, constrained to the class so
                // sibling method calls on `T` resolve.
                type_params: d.class_params.clone(),
                region_params: Vec::new(),
                where_constraints: Vec::new(),
                type_constraints: vec![TypeConstraint {
                    param: d.class_params[0].id,
                    class: d.class,
                }],
                parameters,
                ret_type,
                body,
            },
        ));
    }
    Ok(out)
}

/// Build one typeclass method's signature (over the class parameter, which must
/// already be in scope). The default body, if present, is not built here
/// (it is synthesised per instance); only its presence is recorded.
pub(crate) fn build_method_def<'run>(
    ctx: &mut CompileCtx<'run>,
    mpair: &Pair<Rule>,
) -> Result<(String, MethodDef<'run>), AstError> {
    let range = Range::from(mpair);
    let mut inner = mpair.clone().into_inner();
    let name = inner
        .next()
        .missing("method name", range)?
        .as_str()
        .to_string();
    // A method may declare its own generics (`def fmap<A, B>(...`)
    // they are in scope *alongside* the class parameter while the signature is
    // resolved
    //
    // Pushed onto the current (class-parameter) scope and retracted afterwards.
    let method_params = if inner.peek().map(|p| p.as_rule()) == Some(Rule::type_params) {
        let tp_pair = inner.next().missing("method type parameters", range)?;
        let specs = collect_type_params(ctx, tp_pair);
        ctx.extend_type_params(&specs)
    } else {
        Vec::new()
    };

    let mut param_tys = Vec::new();
    if inner.peek().map(|p| p.as_rule()) == Some(Rule::parameters) {
        let params = inner.next().missing("parameters", range)?;
        for pp in params.into_inner() {
            param_tys.push(param_type(ctx, pp)?);
        }
    }

    let ty_pair = inner.next().missing("method return type", range)?;
    let ret_ty = build_type(ctx, ty_pair)?;

    let mut has_default = false;
    for rest in inner {
        if rest.as_rule() == Rule::expression {
            has_default = true;
        }
    }

    ctx.retract_type_params(&method_params);

    Ok((
        name.clone(),
        MethodDef {
            name,
            type_params: method_params,
            param_tys,
            ret_ty,
            has_default,
            default_fn: None,
            range,
        },
    ))
}

/// Phase 3 (in `build_functions`): build an `impl C for T { … }`. Each method
/// is built as an ordinary function under a mangled name and recorded as the
/// instance's implementation; the instance is registered (coherence-checked)
/// and orphan + completeness rules are enforced here.
pub(crate) fn build_impl<'run>(
    ctx: &mut CompileCtx<'run>,
    child: Pair<Rule>,
    src: &str,
    cur_module: &ModuleRef<'run>,
    funcs: &mut Vec<Function<'run>>,
) -> Result<(), AstError> {
    ctx.set_build_module(*cur_module);
    let range = Range::from(&child);
    let mut inner = child.into_inner();
    let class_pair = inner.next().missing("typeclass name", range)?;
    let class_range = Range::from(&class_pair);
    let class_name = class_pair.as_str().to_string();
    let tref = ctx
        .lookup_typeclass(&class_name)
        .ok_or_else(|| AstError::UnknownTypeclass {
            name: class_name.clone(),
            range,
        })?;
    ctx.record_type_ref(class_range, DefTarget::Typeclass(tref));
    let ty_pair = inner.next().missing("impl target type", range)?;
    // For a higher-kinded class (`class C<F : Owned -> Owned>`), the impl head is
    // a *type constructor* (`impl C for Opt`), written as a bare generic-enum
    // name, which `build_type` would reject as under-applied. Resolve it
    // directly to the constructor's `TypeHead` instead.
    let class_param = ctx.get_typeclass(tref).param;
    let class_is_hk = matches!(ctx.type_param_kind(class_param), Kind::Arrow(_));
    let (for_ty, head) = if class_is_hk {
        let cname = ty_pair.as_str().trim().to_string();
        let er = ctx
            .lookup_enum_current(&cname)
            .ok_or(AstError::UnknownType { name: cname, range })?;
        (ctx.enum_ty(er), TypeHead::Enum(er))
    } else {
        let for_ty = build_type(ctx, ty_pair)?;
        let head = ctx
            .type_head(for_ty)
            .ok_or(AstError::NonInstanceableType { range })?;
        (for_ty, head)
    };

    // orphan rule: the impl is legal only if the class or the implemented type is
    // *at home*, declared in the impl's own module. (This is the whole-program
    // analogue of Rust's crate-orphan rule; it lets `core.sand` implement its own
    // `Copy`/`Clone` for primitives while still rejecting a user module that
    // implements a foreign class for a foreign type.)
    let class_at_home = ctx.get_typeclass(tref).src_module == *cur_module;
    let type_at_home = match head {
        TypeHead::Enum(er) => ctx.get_enum(er).src_module == *cur_module,
        _ => false, // primitives belong to no module
    };
    if !class_at_home && !type_at_home {
        return Err(AstError::OrphanInstance {
            class: class_name,
            range,
        });
    }

    let head_str = head_name(ctx, head);
    let mut methods: Map<String, FunRef> = Map::new();
    for fpair in inner {
        if fpair.as_rule() != Rule::function {
            continue;
        }
        let mrange = Range::from(&fpair);
        let mname = fpair
            .clone()
            .into_inner()
            .next()
            .missing("method name", mrange)?
            .as_str()
            .to_string();
        if !ctx.get_typeclass(tref).methods.contains_key(&mname) {
            return Err(AstError::UnknownMethod {
                class: class_name,
                method: mname,
                range: mrange,
            });
        }
        let mangled = format!("{class_name}${head_str}${mname}");
        let f = build_function(ctx, fpair, src, cur_module, Some(mangled))?;
        methods.insert(mname, f.name);
        funcs.push(f);
    }

    // completeness: every method must end up implemented, by the impl or by
    // the class's default (a generic function built in `build_default_methods`).
    let order = ctx.get_typeclass(tref).method_order.clone();
    for mname in &order {
        if methods.contains_key(mname) {
            continue;
        }
        match ctx.get_typeclass(tref).methods[mname].default_fn {
            Some(default_fr) => {
                methods.insert(mname.clone(), default_fr);
            }
            None => {
                return Err(AstError::MissingMethod {
                    class: class_name,
                    method: mname.clone(),
                    range,
                });
            }
        }
    }

    let impl_def = ImplDef {
        class: tref,
        for_ty,
        head,
        methods,
        src_module: *cur_module,
        range,
    };
    ctx.register_instance(impl_def)
        .map_err(|_existing| AstError::DuplicateInstance {
            class: class_name,
            range,
        })?;
    Ok(())
}

/// Final check: a `Copy` instance is sound only if every field/payload of the
/// type is itself `Copy`, and the type is not generic (no conditional
/// or blanket `Copy` impls).
pub(crate) fn check_copy_instances(ctx: &CompileCtx<'_>) -> Result<(), AstError> {
    let Some(copy) = ctx.copy_class() else {
        return Ok(());
    };
    for (class, head, range) in ctx.instance_keys() {
        if class != copy {
            continue;
        }
        if let TypeHead::Enum(er) = head {
            let def = ctx.get_enum(er);
            if !def.type_params.is_empty() {
                return Err(AstError::CopyOnGenericType { range });
            }
            for v in &def.variants {
                if let Some(payload) = v.payload.get()
                    && !ctx.is_copy(payload)
                {
                    return Err(AstError::CopyPayloadNotCopy { range });
                }
            }
        }
    }
    Ok(())
}

/// Final check: a subclass instance requires its superclass instances for the
/// same head type (Calculus: Typeclasses, `requires`).
pub(crate) fn check_superclass_instances(ctx: &CompileCtx<'_>) -> Result<(), AstError> {
    for (class, head, range) in ctx.instance_keys() {
        let supers = ctx.get_typeclass(class).superclasses.clone();
        for s in supers {
            if ctx.lookup_instance(s, head).is_none() {
                return Err(AstError::MissingSuperclass {
                    class: ctx.get_typeclass(class).name.clone(),
                    superclass: ctx.get_typeclass(s).name.clone(),
                    range,
                });
            }
        }
    }
    Ok(())
}
