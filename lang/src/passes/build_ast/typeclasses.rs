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

    // Optional instance-level type parameters: `impl<E> …`. Allocated once (fixed
    // ids) and shared by the head's fixed slots and every method; `build_function`
    // re-enters them as its ambient scope. Left as the current scope until the
    // methods are built.
    let mut peeked = inner.next().missing("typeclass name", range)?;
    let impl_params: Vec<TypeParam> = if peeked.as_rule() == Rule::type_params {
        let specs = collect_type_params(ctx, peeked.clone());
        let params = ctx.begin_type_params(&specs);
        peeked = inner.next().missing("typeclass name", range)?;
        params
    } else {
        Vec::new()
    };

    let class_pair = peeked;
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
    // a *type constructor*: either a bare name (`impl C for Opt`, the all-holes
    // abstraction) or a partial application with explicit holes (`impl<E> C for
    // Result<_, E>`, Calculus: Partial application). Build it as a constructor
    // abstraction and check its kind against the class parameter. A
    // non-higher-kinded class takes an ordinary value type as before.
    let class_param = ctx.get_typeclass(tref).param;
    let class_kind = ctx.type_param_kind(class_param);
    let class_is_hk = matches!(class_kind, Kind::Arrow(_));
    let (for_ty, head) = if class_is_hk {
        let (for_ty, er) = build_impl_head(ctx, &ty_pair, range)?;
        let found = ctx.constructor_kind(for_ty);
        if found != class_kind {
            return Err(AstError::ImplHeadKindMismatch {
                class: class_name,
                expected: ctx.display_kind(class_kind),
                found: ctx.display_kind(found),
                range,
            });
        }
        (for_ty, TypeHead::Enum(er))
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

    let head_str = head_mangle(ctx, for_ty);
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
        let f = build_function(ctx, fpair, src, cur_module, Some(mangled), &impl_params)?;
        // The impl method must conform to the class's declared signature (once
        // `F` is the instance head and generics are renamed).
        let mdef = ctx.get_typeclass(tref).methods[&mname].clone();
        check_method_conformance(
            ctx,
            &class_name,
            &mname,
            &mdef,
            for_ty,
            class_param,
            impl_params.len(),
            &f,
        )?;
        methods.insert(mname, f.name);
        funcs.push(f);
    }
    ctx.end_type_params();

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
        impl_type_params: impl_params,
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

/// Method-conformance check (Calculus: Typeclasses): an impl method's signature
/// must match the class method's declaration, once the class parameter `F` is
/// replaced by the instance head (`for_ty`, a partial application for a
/// higher-kinded class) and the class method's generics are renamed to the impl
/// method's (positionally). Compared modulo regions, so `&'a T` / `&'b T`
/// agree. Catches a wrong return type, swapped/wrong argument types, or a
/// mismatched method-generic arity at the `impl` instead of at a later call
/// site.
#[allow(clippy::too_many_arguments)]
fn check_method_conformance<'run>(
    ctx: &mut CompileCtx<'run>,
    class_name: &str,
    method: &str,
    mdef: &MethodDef<'run>,
    for_ty: Ty<'run>,
    class_param: crate::lang::types::TypeParamId,
    ambient_len: usize,
    f: &Function<'run>,
) -> Result<(), AstError> {
    use crate::passes::type_ast::generics::subst;

    // Renaming: class param `F` -> the head abstraction; the class method's
    // generics -> the impl method's own generics (those after the ambient impl
    // params that `build_function` prepended).
    let impl_own = &f.type_params[ambient_len..];
    let mut sigma: Map<crate::lang::types::TypeParamId, Ty<'run>> = Map::new();
    sigma.insert(class_param, for_ty);
    let renameable = mdef.type_params.len() == impl_own.len();
    if renameable {
        for (c, i) in mdef.type_params.iter().zip(impl_own) {
            let p = ctx.param_ty(i.id);
            sigma.insert(c.id, p);
        }
    }

    let expected_params: Vec<Ty<'run>> = mdef
        .param_tys
        .iter()
        .map(|t| subst(ctx, *t, &sigma))
        .collect();
    let expected_ret = subst(ctx, mdef.ret_ty, &sigma);

    let ok = renameable
        && expected_params.len() == f.parameters.len()
        && expected_params
            .iter()
            .zip(&f.parameters)
            .all(|(e, p)| e.eq_modulo_regions(p.ty))
        && expected_ret.eq_modulo_regions(f.ret_type);
    if ok {
        return Ok(());
    }

    let render = |ctx: &CompileCtx<'run>, params: &[Ty<'run>], ret: Ty<'run>| {
        let ps = params
            .iter()
            .map(|t| ctx.display_ty(*t).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("({ps}) -> {}", ctx.display_ty(ret))
    };
    let found_params: Vec<Ty<'run>> = f.parameters.iter().map(|p| p.ty).collect();
    Err(AstError::MethodSignatureMismatch {
        class: class_name.to_string(),
        method: method.to_string(),
        expected: render(ctx, &expected_params, expected_ret),
        found: render(ctx, &found_params, f.ret_type),
        range: f.range,
    })
}

/// A collision-free discriminator for an instance's mangled method names,
/// derived from its head abstraction (Calculus: Partial application). A bare
/// `Enum`/ground type mangles to its name (so plain instances keep their
/// existing names); a partial application appends its slots (`h` for a hole,
/// the fixed type otherwise), so disjoint higher-kinded instances like `Foo<_,
/// Int>` and `Foo<_, Bool>` get distinct method names instead of clashing on
/// the bare constructor name.
fn head_mangle<'a>(ctx: &CompileCtx<'a>, ty: Ty<'a>) -> String {
    match ty.kind() {
        TyKind::Enum(er) => ctx.get_enum(*er).name.clone(),
        TyKind::App(er, args, _) => {
            let mut s = ctx.get_enum(*er).name.clone();
            for a in args.iter() {
                s.push('_');
                s.push_str(&head_mangle(ctx, *a));
            }
            s
        }
        TyKind::Hole(_) => "h".to_string(),
        TyKind::Param(id) => format!("p{}", id.0),
        TyKind::Int => "Int".to_string(),
        TyKind::Bool => "Bool".to_string(),
        TyKind::Unit => "Unit".to_string(),
        TyKind::Tuple(es) => {
            let mut s = format!("Tup{}", es.len());
            for e in es.iter() {
                s.push('_');
                s.push_str(&head_mangle(ctx, *e));
            }
            s
        }
        TyKind::Ref(_, t) => format!("Ref_{}", head_mangle(ctx, *t)),
        TyKind::RefMut(_, t) => format!("RefMut_{}", head_mangle(ctx, *t)),
        TyKind::Ptr(t) => format!("Ptr_{}", head_mangle(ctx, *t)),
        _ => "T".to_string(),
    }
}

/// Elaborate a higher-kinded `impl` head into a constructor abstraction
/// (Calculus: Partial application) and its base enum. A bare name `Foo` is the
/// all-holes abstraction (returned as the `Enum(er)` shorthand;
/// `constructor_kind` reads its arrow from the enum's arity). `Foo<_, E>`
/// becomes `App(er, [Hole(0), E])` with holes numbered by left-to-right
/// appearance and the fixed slots resolved against the instance's parameters
/// (in scope via the caller's `begin_type_params`).
fn build_impl_head<'run>(
    ctx: &mut CompileCtx<'run>,
    ty_pair: &Pair<Rule>,
    range: Range,
) -> Result<(Ty<'run>, AdtRef<'run>), AstError> {
    // `type_` -> `core_type` -> (identifier | type_application).
    let core = ty_pair
        .clone()
        .into_inner()
        .next()
        .missing("impl head type", range)?;
    if core.as_rule() != Rule::core_type {
        return Err(AstError::NonInstanceableType { range });
    }
    let node_opt = core.clone().into_inner().next();
    match node_opt {
        // `Foo<_, E>`: a partial application with explicit holes.
        Some(node) if node.as_rule() == Rule::type_application => {
            let mut parts = node.into_inner();
            let name = parts
                .next()
                .missing("impl head constructor name", range)?
                .as_str()
                .to_string();
            let er = ctx
                .lookup_enum_current(&name)
                .ok_or(AstError::UnknownType {
                    name: name.clone(),
                    range,
                })?;
            let arity = ctx.get_enum(er).type_params.len();
            let mut slots: Vec<Ty<'run>> = Vec::new();
            let mut hole_idx: u32 = 0;
            for arg in parts {
                let child = arg
                    .into_inner()
                    .next()
                    .missing("impl head argument", range)?;
                match child.as_rule() {
                    Rule::hole => {
                        slots.push(ctx.hole_ty(hole_idx));
                        hole_idx += 1;
                    }
                    // Holes abstract type parameters only; a region argument on a
                    // higher-kinded head is unsupported.
                    Rule::lifetime => {
                        return Err(AstError::RegionArgArityMismatch {
                            name,
                            expected: 0,
                            found: 1,
                            range,
                        });
                    }
                    _ => slots.push(build_type(ctx, child)?),
                }
            }
            if slots.len() != arity {
                return Err(AstError::TypeArgArityMismatch {
                    name,
                    expected: arity,
                    found: slots.len(),
                    range,
                });
            }
            Ok((ctx.intern_app(er, slots, Vec::new()), er))
        }
        // bare constructor `Foo` (a captured identifier, or the `core_type` leaf).
        Some(node) if node.as_rule() == Rule::identifier => {
            let name = node.as_str().to_string();
            let er = ctx
                .lookup_enum_current(&name)
                .ok_or(AstError::UnknownType { name, range })?;
            Ok((ctx.enum_ty(er), er))
        }
        None => {
            let name = core.as_str().trim().to_string();
            let er = ctx
                .lookup_enum_current(&name)
                .ok_or(AstError::UnknownType { name, range })?;
            Ok((ctx.enum_ty(er), er))
        }
        // `&T`, tuples, primitives, … cannot be a higher-kinded constructor.
        Some(_) => Err(AstError::NonInstanceableType { range }),
    }
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
