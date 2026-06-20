//! build an AST from the Pair tree
#![allow(clippy::result_large_err)]

use std::num::ParseIntError;

use pest::Parser;
use pest::error::Error;
use pest::iterators::Pair;
use thiserror::Error;

use crate::compiler::context::CompileCtx;
use crate::compiler::context::ContextError;
use crate::compiler::context::DefTarget;
use crate::compiler::structure::Derivable;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::FunSig;
use crate::compiler::structure::HeapedStrategy;
use crate::compiler::structure::ImplDef;
use crate::compiler::structure::Map;
use crate::compiler::structure::MethodDef;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::RegionParamSpec;
use crate::compiler::structure::TypeConstraint;
use crate::compiler::structure::TypeHead;
use crate::compiler::structure::TypeParam;
use crate::compiler::structure::TypeParamSpec;
use crate::compiler::structure::TypeclassDef;
use crate::compiler::structure::TypeclassRef;
use crate::compiler::structure::UniqVar;
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
        "type '{name}' expects {expected} lifetime argument(s) but {found} were given at {range}"
    )]
    RegionArgArityMismatch {
        name: String,
        expected: usize,
        found: usize,
        range: Range,
    },

    #[error(
        "lifetime arguments must come before type arguments at {range} (write `{name}<'a, T>`, not `{name}<T, 'a>`)"
    )]
    RegionArgsNotFirst { name: String, range: Range },

    #[error(
        "a reference in a payload of type '{name}' at {range} must use a declared lifetime parameter (e.g. `type {name}<'a> = ...(&'a T)`) or `'static`; an elided `&` cannot be tracked"
    )]
    PayloadBorrowNeedsLifetime { name: String, range: Range },

    #[error(
        "type '{ty}' at {range} is not FFI-safe; an `extern def` may only use `Int`, `Unit`, or `Ptr<T>` across the C boundary"
    )]
    NonFfiSafeType { ty: String, range: Range },

    #[error("'{name}' at {range} is not a derivable property")]
    NotDerivable { name: String, range: Range },

    #[error("'{name}' is derived more than once at {range}")]
    DuplicateDerive { name: String, range: Range },

    #[error(
        "recursive type '{name}' at {range} must be heap-allocated (add `deriving Heaped`); without it its values would be infinite-sized and would leak"
    )]
    RecursiveTypeNeedsHeaped { name: String, range: Range },

    #[error("unknown typeclass '{name}' at {range}")]
    UnknownTypeclass { name: String, range: Range },

    #[error("typeclass '{name}' at {range} must declare exactly one type parameter")]
    TypeclassParamArity { name: String, range: Range },

    #[error("typeclass method at {range} may not declare its own generics yet")]
    MethodGenericsUnsupported { range: Range },

    #[error("unknown superclass '{name}' at {range}")]
    UnknownSuperclass { name: String, range: Range },

    #[error("method name '{name}' at {range} already names another typeclass method")]
    DuplicateMethodName { name: String, range: Range },

    #[error("cannot implement a typeclass for this type at {range}")]
    NonInstanceableType { range: Range },

    #[error("'{method}' at {range} is not a method of typeclass '{class}'")]
    UnknownMethod {
        class: String,
        method: String,
        range: Range,
    },

    #[error("instance of '{class}' at {range} is missing method '{method}'")]
    MissingMethod {
        class: String,
        method: String,
        range: Range,
    },

    #[error("duplicate instance of '{class}' for this type at {range}")]
    DuplicateInstance { class: String, range: Range },

    #[error(
        "orphan instance at {range}: implementing '{class}' requires the class or the type to be defined locally"
    )]
    OrphanInstance { class: String, range: Range },

    #[error(
        "instance of '{class}' at {range} requires an instance of its superclass '{superclass}' for the same type"
    )]
    MissingSuperclass {
        class: String,
        superclass: String,
        range: Range,
    },

    #[error("`Copy` at {range} requires every field to be `Copy`")]
    CopyPayloadNotCopy { range: Range },

    #[error("`Copy` for a generic type at {range} is not supported")]
    CopyOnGenericType { range: Range },

    #[error("malformed `use` at {range}: expected `use module::name;` or `use module::*;`")]
    MalformedUse { range: Range },

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
        "'{name}' is not a type constructor (it has a value kind), so it cannot be applied as '{name}<...>' at {range}"
    )]
    NotATypeConstructor { name: String, range: Range },

    #[error(
        "'{name}' is a type constructor (higher-kinded) and must be applied to arguments (e.g. `{name}<T>`) rather than used as a type at {range}"
    )]
    TypeConstructorNotApplied { name: String, range: Range },

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

pub fn build_program<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<'i, Rule>,
    src: &str,
    default_module: ModuleRef<'run>,
    file: FileRef,
) -> Result<Map<ModuleRef<'run>, Vec<Function<'run>>>, AstError> {
    assert_eq!(pair.as_rule(), Rule::program);
    let children: Vec<Pair<'i, Rule>> = pair.into_inner().collect();

    // The front end runs as a collect-then-resolve sequence:
    //
    //  1. a. collect declarations, register enum *skeletons* + `use` imports; b.
    //     resolve enum payloads, now that every skeleton exists, so forward /
    //     recursive payload types (`type Tree = Node((Tree, Tree))`) resolve; c.
    //     variance soundness, declared variance vs. payload positions;
    //
    //  2. build function bodies, names resolve against the collected decls.
    let collected = collect_declarations(ctx, &children, default_module, file)?;
    resolve_enum_payloads(ctx, collected.pending_payloads)?;
    // Heap-strategy legality (Calculus, `K-HeapedRec`): a (mutually) recursive
    // type must derive a heap strategy; a non-recursive one must not. Runs after
    // payloads resolve (recursion is visible only once payload types exist).
    check_heaped_legality(ctx)?;
    collected
        .generic_enums
        .iter()
        .try_for_each(|er| check_variance(ctx, *er))?;

    // Typeclass method signatures resolve after every enum + class skeleton
    // exists, so a method type may reference any enum or the class parameter.
    let defaults = resolve_typeclass_sigs(ctx, collected.pending_classes)?;
    // Build defaulted methods (as generic functions) *before* impls, so an impl
    // that omits a defaulted method can point its instance entry at the default.
    let default_fns = build_default_methods(ctx, defaults, src)?;
    let mut mods = build_functions(ctx, children, src, default_module, file)?;
    default_fns.into_iter().for_each(|(module, f)| {
        mods.entry(module).or_default().push(f);
    });
    // Instance-set checks need every instance registered (build_functions did
    // that): a subclass instance requires its superclass instances for the same
    // head type (Calculus: Typeclasses, `requires`).
    check_superclass_instances(ctx)?;
    check_copy_instances(ctx)?;
    Ok(mods)
}

/// The declarations gathered in phase 1, to be resolved in later phases.
struct Collected<'i, 'run> {
    /// `(enum, variant index, raw payload `type_` pair)`, resolved in phase
    /// 1.5 once every enum skeleton exists. Pairs borrow from the parse
    /// (`'i`).
    pending_payloads: Vec<(AdtRef<'run>, usize, Vec<Pair<'i, Rule>>)>,
    /// Generic enums, for the phase-1.6 variance check.
    generic_enums: Vec<AdtRef<'run>>,
    /// Typeclass skeletons whose method signatures + superclasses resolve in a
    /// later phase (once all classes/enums exist). Pairs borrow from the parse.
    pending_classes: Vec<PendingClass<'i>>,
}

/// A registered typeclass skeleton awaiting method-signature + superclass
/// resolution (see [`resolve_typeclass_sigs`]).
struct PendingClass<'i> {
    tref: TypeclassRef,
    /// the class's type parameter(s), exactly one, to re-enter while building
    /// method signatures (so `T` resolves in them).
    class_params: Vec<TypeParam>,
    method_pairs: Vec<Pair<'i, Rule>>,
    superclass_names: Vec<(String, Range)>,
}

/// phase 1, walk the top level and register each enum *skeleton* (name +
/// variant names, payloads deferred) and each `use` import, tracking the
/// current module. No payload types are resolved yet (a payload may
/// forward-reference another enum), so every `EnumRef` exists before any
/// `build_type` runs.
fn collect_declarations<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    children: &[Pair<'i, Rule>],
    default_module: ModuleRef<'run>,
    file: FileRef,
) -> Result<Collected<'i, 'run>, AstError> {
    let mut pending_payloads: Vec<(AdtRef<'run>, usize, Vec<Pair<'i, Rule>>)> = Vec::new();
    let mut generic_enums: Vec<AdtRef<'run>> = Vec::new();
    let mut pending_classes: Vec<PendingClass<'i>> = Vec::new();
    let mut cur_mod = default_module;
    for child in children {
        match child.as_rule() {
            Rule::typeclass_decl => {
                pending_classes.push(collect_typeclass(ctx, child, cur_mod)?);
            }
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
            Rule::type_alias => collect_enum_skeleton(
                ctx,
                child,
                cur_mod,
                &mut pending_payloads,
                &mut generic_enums,
            )?,
            Rule::use_decl => collect_use(ctx, child, cur_mod)?,
            _ => {}
        }
    }
    Ok(Collected {
        pending_payloads,
        generic_enums,
        pending_classes,
    })
}

/// Register one `type` declaration's skeleton (name, type/region params,
/// variant names) and stash its raw payload pairs for phase 1b.
fn collect_enum_skeleton<'i, 'run>(
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
fn parse_deriving_clause(pair: &Pair<Rule>) -> Result<Vec<Derivable>, AstError> {
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

/// Record one `use src::name;` / `use src::*;` import into `cur_mod`'s scope.
/// (`use_decl = { "use" ~ use_path ~ ";" }`;
/// `use_path = { identifier ~ ("::" ~ identifier)* ~ ("::" ~ "*")? }`.)
fn collect_use<'run>(
    ctx: &mut CompileCtx<'run>,
    child: &Pair<Rule>,
    cur_mod: ModuleRef<'run>,
) -> Result<(), AstError> {
    let upath = child
        .clone()
        .into_inner()
        .next()
        .missing("use path", Range::from(child))?;
    let mut segs: Vec<String> = Vec::new();
    let mut glob = false;
    for seg in upath.into_inner() {
        match seg.as_rule() {
            Rule::identifier => segs.push(seg.as_str().to_string()),
            Rule::use_glob => glob = true,
            _ => {}
        }
    }
    if glob {
        ctx.add_import(cur_mod, segs.join("::"), None);
    } else if segs.len() >= 2 {
        let item = segs.pop().expect("checked len >= 2");
        ctx.add_import(cur_mod, segs.join("::"), Some(item));
    } else {
        return Err(AstError::MalformedUse {
            range: Range::from(child),
        });
    }
    Ok(())
}

/// phase 1b, resolve each stashed variant payload type, now that every enum
/// skeleton exists. Each payload resolves with its enum's type/region
/// parameters in scope and its bare type names in the enum's own module; a
/// borrow in a payload must name a declared region parameter (or `'static`).
fn resolve_enum_payloads<'i, 'run>(
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
fn head_name<'a>(ctx: &CompileCtx<'a>, head: TypeHead<'a>) -> String {
    match head {
        TypeHead::Int => "Int".to_string(),
        TypeHead::Bool => "Bool".to_string(),
        TypeHead::Unit => "Unit".to_string(),
        TypeHead::Enum(er) => ctx.get_enum(er).name.clone(),
    }
}

/// Phase 1: register a `typeclass` skeleton (name, type parameter, method
/// names, superclass names). Method *signatures* and superclass *refs* resolve
/// later in [`resolve_typeclass_sigs`], once every class + enum skeleton
/// exists.
fn collect_typeclass<'i, 'run>(
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
fn resolve_typeclass_sigs<'i>(
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
struct PendingDefault<'i> {
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
fn build_default_methods<'run>(
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
fn build_method_def<'run>(
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

/// Extract a parameter's declared type without registering a variable (used for
/// typeclass method signatures, which have no bodies).
fn param_type<'run>(ctx: &mut CompileCtx<'run>, p: Pair<Rule>) -> Result<Ty<'run>, AstError> {
    let range = Range::from(&p);
    let ty_pair = p
        .into_inner()
        .find(|c| c.as_rule() == Rule::type_)
        .missing("parameter type", range)?;
    build_type(ctx, ty_pair)
}

/// Phase 3 (in `build_functions`): build an `impl C for T { … }`. Each method
/// is built as an ordinary function under a mangled name and recorded as the
/// instance's implementation; the instance is registered (coherence-checked)
/// and orphan + completeness rules are enforced here.
fn build_impl<'run>(
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
fn check_copy_instances(ctx: &CompileCtx<'_>) -> Result<(), AstError> {
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
fn check_superclass_instances(ctx: &CompileCtx<'_>) -> Result<(), AstError> {
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

fn build_functions<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    children: Vec<Pair<'i, Rule>>,
    src: &str,
    default_module: ModuleRef<'run>,
    file: FileRef,
) -> Result<Map<ModuleRef<'run>, Vec<Function<'run>>>, AstError> {
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
                funcs.push(build_function(ctx, child, src, &current_module, None)?);
            }
            Rule::extern_decl => {
                collect_extern(ctx, child, &current_module)?;
            }
            Rule::impl_decl => {
                build_impl(ctx, child, src, &current_module, &mut funcs)?;
            }
            // enum / `use` / typeclass declarations were handled in phase 1
            // (typeclass method *bodies*, the defaults, are built separately).
            Rule::type_alias | Rule::use_decl | Rule::typeclass_decl => {}
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
    name_override: Option<String>,
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
            let specs = collect_type_params(ctx, tp_pair.clone());
            let type_params = ctx.begin_type_params(&specs);
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
fn collect_extern<'run>(
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

/// Heap-strategy legality (Calculus, `K-HeapedRec`): a (mutually) recursive
/// `type` *must* derive a heap strategy (`deriving HeapedUnique`); without it
/// its values would be infinite-sized and leak. A non-recursive type *may*
/// derive one (to opt a large value onto the heap) but need not.
fn check_heaped_legality(ctx: &CompileCtx<'_>) -> Result<(), AstError> {
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
fn enum_successors<'tcx>(ctx: &CompileCtx<'tcx>, er: AdtRef<'tcx>) -> Vec<AdtRef<'tcx>> {
    let mut out = Vec::new();
    for v in &ctx.get_enum(er).variants {
        if let Some(ty) = v.payload.get() {
            collect_referenced_enums(ty, &mut out);
        }
    }
    out
}

/// Push every enum mentioned in `ty` (directly or nested) into `out`.
fn collect_referenced_enums<'tcx>(ty: Ty<'tcx>, out: &mut Vec<AdtRef<'tcx>>) {
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
fn is_recursive_enum<'tcx>(ctx: &CompileCtx<'tcx>, start: AdtRef<'tcx>) -> bool {
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
fn require_ffi_safe<'tcx>(
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

/// Build the outlives constraints from a `where 'r >= 's, ...` clause. Both
/// lifetimes must already be in scope (declared region parameters or
/// `'static`).
#[allow(clippy::type_complexity)]
fn build_where_clause(
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
fn check_variance<'run>(ctx: &CompileCtx<'run>, er: AdtRef<'run>) -> Result<(), AstError> {
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
struct Occurrence {
    /// occurs in a covariant (producer) position.
    pos: bool,
    /// occurs in a contravariant (consumer) position.
    neg: bool,
}

/// Polarity of the position currently being descended into.
#[derive(Clone, Copy, PartialEq)]
enum Sign {
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
fn param_polarity<'run>(
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

/// Parse each `type_param` in a `type_params` pair, applying the default
/// variance (`Covariant`) and kind (`Owned`) when their annotations are absent.
/// Region parameters in the same `<...>` list are handled by
/// [`collect_region_params`] and skipped here.
fn collect_type_params(ctx: &mut CompileCtx<'_>, pair: Pair<Rule>) -> Vec<TypeParamSpec> {
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
fn build_kind(ctx: &mut CompileCtx<'_>, pair: Pair<Rule>) -> Kind {
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
fn build_kind_atom(ctx: &mut CompileCtx<'_>, pair: Pair<Rule>) -> Kind {
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
/// scope. `'static` always is, any other name must be a declared region
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

/// Build a type, applying an optional `@ 'r` region ascription (Calculus:
/// Types). `type_ = { fn_type | core_type ~ ("@" ~ lifetime)? }`.
fn build_type<'run>(ctx: &mut CompileCtx<'run>, pair: Pair<Rule>) -> Result<Ty<'run>, AstError> {
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
fn build_fn_type<'run>(ctx: &mut CompileCtx<'run>, pair: Pair<Rule>) -> Result<Ty<'run>, AstError> {
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
fn build_fn_arrow(pair: &Pair<Rule>) -> FnMode {
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
                // the type checker enforces this, here we just require it.
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
            // (exclusive) (Calculus, the `Let` rules): desugar to `let x : &T = &e` /
            // `let x : &mut T = &mut e`, reusing the borrow-expression
            // machinery (`e` is borrowed, not consumed, and `x` holds the
            // reference). A `&mut` binding is assignable (`x = e` writes through
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
            let target = a_inner.next().missing("assignment target", inner_range)?;
            match target.as_rule() {
                Rule::identifier => {
                    let name = target.as_str().to_string();
                    let expr = build_expr(
                        ctx,
                        a_inner.next().missing("assignment value", inner_range)?,
                        src,
                    )?;
                    Ok(Statement::Assignment {
                        name: HirVar::Unqualified(name),
                        range: inner_range,
                        val: expr,
                    })
                }
                // `*r = e`: write-through. The reference is the deref's inner.
                Rule::deref_expr => {
                    let ref_pair = target
                        .into_inner()
                        .next()
                        .missing("dereference target", inner_range)?;
                    let reference = build_primary(ctx, ref_pair, src)?;
                    let value = build_expr(
                        ctx,
                        a_inner.next().missing("assignment value", inner_range)?,
                        src,
                    )?;
                    Ok(Statement::DerefAssign {
                        reference,
                        value,
                        range: inner_range,
                    })
                }
                other => internal_bug!(
                    "assignment target was neither identifier nor deref_expr: {other:?}"
                ),
            }
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
        Rule::lambda_expr => build_lambda(ctx, pair, src),
        other => Err(AstError::UnexpectedRule {
            expected: "expression-like rule",
            got: other,
            range,
        }),
    }
}

/// Build a lambda `fn (x: T) -> e`.
/// `lambda_expr = { "fn" ~ lambda_param ~ "->" ~ expression }`,
/// `lambda_param = { "(" ~ mut_kw? ~ identifier ~ ":" ~ type_ ~ ")" }`.
fn build_lambda<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::lambda_expr);
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();
    let param_pair = inner.next().missing("lambda parameter", range)?;
    let arrow = inner.next().missing("lambda arrow", range)?;
    let mode = build_fn_arrow(&arrow);
    let body_pair = inner.next().missing("lambda body", range)?;

    // lambda_param = { "(" ~ mut_kw? ~ identifier ~ ":" ~ type_ ~ ")" }
    let prange = Range::from(&param_pair);
    let mut pparts = param_pair.into_inner().peekable();
    let is_mutable = pparts.peek().map(|p| p.as_rule()) == Some(Rule::mut_kw);
    if is_mutable {
        pparts.next();
    }
    let name = pparts.next().missing("lambda parameter name", prange)?;
    let ty_pair = pparts.next().missing("lambda parameter type", prange)?;
    let ty = build_type(ctx, ty_pair)?;
    let var = HirVar::Decl(ctx.new_original_variable(&name, Rule::parameter)?);
    let param = Parameter {
        name: var,
        ty,
        range: prange,
        is_mutable,
    };

    let body = Box::new(build_expr(ctx, body_pair, src)?);
    Ok(Expr {
        expr: Expression::Lambda { param, body, mode },
        range,
    })
}

/// The lang-item name of the monadic bind that `do`-notation desugars to.
const BIND_FN: &str = "bind";

/// Desugar a block that uses do-notation (contains a top-level `<-`) into
/// nested `bind` calls. Called by [`build_block`] when a block has any
/// `monadic_bind` child; an ordinary block (no `<-`) never reaches here.
///
/// `{ x: T <- e; <rest> }` becomes `bind(e, fn (x: T) -> <rest>)`, applied
/// right-to-left so the *rest of the block* is the continuation. Ordinary
/// statements (`let`, ..) between binds are gathered into a `Block` that wraps
/// the continuation, and the trailing expression is the innermost result. The
/// result is plain HHIR (`Call`/`Lambda`/`Block`), so no downstream pass needs
/// to know do-notation ever existed.
///
/// A block using `<-` **must** end in a trailing expression (its monadic
/// result), otherwise there is nothing for a final bind to continue into.
///
/// **Error reporting.** Every synthesised node is given an actual source range:
/// the `bind` call and its continuation lambda point at the originating
/// `x <- e;` line, the lambda parameter points at the bound identifier, and `e`
/// / the trailing expression keep their own spans. So a type error in `e`, a
/// missing `Monad` instance, or a wrong continuation type all land on source
/// the user actually wrote. Node construction is funnelled through this one
/// function so a future "in this `<-` expansion" provenance note can be
/// attached in a single place. (For now the desugaring is mandatory-annotation
/// only; the `T` in `x: T <-` is what lets the continuation lambda type-check
/// without lambda-parameter inference.)
fn build_monadic_block<'run>(
    ctx: &mut CompileCtx<'run>,
    mut children: Vec<Pair<Rule>>,
    block_range: Range,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    // The final child must be the trailing expression (the monadic result).
    if children.last().map(|c| c.as_rule()) != Some(Rule::expression) {
        return Err(AstError::UnexpectedRule {
            expected: "trailing expression (a block using `<-` must end with its monadic result)",
            got: children
                .last()
                .map(|c| c.as_rule())
                .unwrap_or(Rule::monadic_bind),
            range: block_range,
        });
    }
    let tail_pair = children.pop().expect("checked non-empty above");
    let mut acc = build_expr(ctx, tail_pair, src)?;
    let items = children;

    // Walk the leading items (binds and statements) in reverse, folding each into
    // the accumulating continuation. Consecutive plain statements are buffered
    // (in reverse) and flushed into one `Block` when a bind or the start is hit.
    let mut pending: Vec<Statement<'run>> = Vec::new();
    let flush = |pending: &mut Vec<Statement<'run>>, acc: Expr<'run>| -> Expr<'run> {
        if pending.is_empty() {
            return acc;
        }
        pending.reverse();
        let stmts = std::mem::take(pending);
        let range = acc.range;
        Expr {
            expr: Expression::Block {
                statements: stmts,
                expr: Some(Box::new(acc)),
            },
            range,
        }
    };

    for item in items.into_iter().rev() {
        match item.as_rule() {
            Rule::monadic_bind => {
                // statements *after* this bind belong to the continuation body.
                acc = flush(&mut pending, acc);

                // monadic_bind = { identifier ~ ":" ~ type_ ~ "<-" ~ expression ~ ";" }
                let bind_range = Range::from(&item);
                let mut parts = item.into_inner();
                let name = parts.next().missing("do-bind variable", bind_range)?;
                let ty_pair = parts.next().missing("do-bind type", bind_range)?;
                let e_pair = parts.next().missing("do-bind expression", bind_range)?;

                let param_range = Range::from(&name);
                let ty = build_type(ctx, ty_pair)?;
                let var = HirVar::Decl(ctx.new_original_variable(&name, Rule::parameter)?);
                let param = Parameter {
                    name: var,
                    ty,
                    range: param_range,
                    is_mutable: false,
                };
                let bound = build_expr(ctx, e_pair, src)?;

                // bind(e, fn (x: T) -> <continuation>)
                let cont = Expr {
                    expr: Expression::Lambda {
                        param,
                        body: Box::new(acc),
                        mode: crate::lang::types::FnMode::Reusable,
                    },
                    range: bind_range,
                };
                acc = Expr {
                    expr: Expression::Call {
                        fn_name: HirFnCall::Local(BIND_FN.to_string()),
                        args: vec![bound, cont],
                        type_args: Vec::new(),
                    },
                    range: bind_range,
                };
            }
            Rule::statement => pending.push(build_statement(ctx, item, src)?),
            other => {
                return Err(AstError::UnexpectedRule {
                    expected: "monadic_bind | statement in do-block",
                    got: other,
                    range: Range::from(&item),
                });
            }
        }
    }
    Ok(flush(&mut pending, acc))
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

// Maps every left-associative binary operator token to its `Bop`, for the
// `binop_fold` precedence levels. (`pow` is right-associative and handled
// directly in `build_power`.)
fn bop_from_rule(rule: Rule) -> Bop {
    match rule {
        Rule::or => Bop::Or,
        Rule::xor => Bop::Xor,
        Rule::logand => Bop::And,
        Rule::bitand => Bop::BitAnd,
        Rule::eq => Bop::Comp(CompOp::Eq),
        Rule::ne => Bop::Comp(CompOp::Ne),
        Rule::gt => Bop::Comp(CompOp::Gt),
        Rule::lt => Bop::Comp(CompOp::Lt),
        Rule::ge => Bop::Comp(CompOp::Ge),
        Rule::le => Bop::Comp(CompOp::Le),
        Rule::add => Bop::Plus,
        Rule::subtract => Bop::Minus,
        Rule::multiply => Bop::Mult,
        Rule::divide => Bop::Div,
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
    binop_fold(ctx, pair.into_inner(), build_comparison, src, range)
}

// comparison = { add_sub ~ ( (gt | lt | ge | le) ~ add_sub )* }
fn build_comparison<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    binop_fold(ctx, pair.into_inner(), build_add_sub, src, range)
}

// add_sub = { mul_div ~ ( (add | subtract) ~ mul_div )* }
fn build_add_sub<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    binop_fold(ctx, pair.into_inner(), build_mul_div, src, range)
}

// mul_div = { power ~ ( (multiply | divide) ~ power )* }
fn build_mul_div<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    binop_fold(ctx, pair.into_inner(), build_power, src, range)
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
        let children: Vec<Pair<Rule>> = pair.into_inner().collect();

        // do-notation: a top-level `<-` anywhere in the block makes the whole
        // block monadic (desugars to nested `bind`); otherwise it is ordinary.
        if children.iter().any(|c| c.as_rule() == Rule::monadic_bind) {
            return build_monadic_block(ctx, children, range, src);
        }

        let mut statements = Vec::new();
        let mut expr: Option<Box<Expr>> = None;
        for inner in children {
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
            let payload = build_payload_expr(ctx, parts, src, inner_range)?;
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
            let payload = build_payload_expr(ctx, parts, src, inner_range)?;
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
            // optional payload expression(s); more than one desugar to a tuple payload.
            let payload = build_payload_expr(ctx, children, src, inner_range)?;
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
    // Multiple sub-patterns desugar to a single tuple sub-pattern:
    // `let Cons(x, rest) = …` ≡ `let Cons((x, rest)) = …`.
    let mut subs = parts
        .map(|p| build_let_destructure(ctx, p))
        .collect::<Result<Vec<_>, _>>()?;
    let payload = match subs.len() {
        0 => None,
        1 => Some(Box::new(subs.pop().unwrap())),
        _ => Some(Box::new(HirPattern::Tuple(subs))),
    };
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

/// Build a constructor/tag payload from its (zero or more) argument
/// expressions. Multiple arguments desugar to a single tuple payload: `Ok(a,
/// b)` ≡ `Ok((a, b))`.
fn build_payload_expr<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    parts: impl Iterator<Item = Pair<'i, Rule>>,
    src: &str,
    range: Range,
) -> Result<Option<Box<Expr<'run>>>, AstError> {
    let mut exprs = parts
        .map(|p| build_expr(ctx, p, src))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match exprs.len() {
        0 => None,
        1 => Some(Box::new(exprs.pop().unwrap())),
        _ => Some(Box::new(Expr {
            expr: Expression::Tuple(exprs),
            range,
        })),
    })
}

/// Build a constructor/tag payload sub-pattern from its (zero or more) argument
/// patterns. Multiple arguments desugar to a single tuple sub-pattern:
/// `Cons(x, rest)` ≡ `Cons((x, rest))`.
fn build_payload_pattern<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    parts: impl Iterator<Item = Pair<'i, Rule>>,
) -> Result<Option<Box<HirPattern<'run>>>, AstError> {
    let mut pats = parts
        .map(|p| build_pattern(ctx, p))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match pats.len() {
        0 => None,
        1 => Some(Box::new(pats.pop().unwrap())),
        _ => Some(Box::new(HirPattern::Tuple(pats))),
    })
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
            let payload = build_payload_pattern(ctx, parts)?;
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
            let payload = build_payload_pattern(ctx, parts)?;
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

    // optional turbofish `::<T, …>` (function_call only)
    let mut type_args = Vec::new();
    if inner.peek().map(|p| p.as_rule()) == Some(Rule::turbofish) {
        let tf = inner.next().missing("turbofish", range)?;
        for ty_pair in tf.into_inner() {
            type_args.push(build_type(ctx, ty_pair)?);
        }
    }

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
        expr: Expression::Call {
            fn_name,
            args,
            type_args,
        },
        range,
    })
}
