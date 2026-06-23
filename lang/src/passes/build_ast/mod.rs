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
use crate::compiler::structure::TypeParam;
use crate::compiler::structure::TypeclassRef;
use crate::compiler::structure::UriError;
use crate::ir_types::hhir::*;
use crate::lang::ops::*;
use crate::lang::types::*;
use crate::passes::parse::LangParser;
use crate::passes::parse::Rule;

mod enums;
mod exprs;
mod functions;
mod typeclasses;
mod types;

pub(crate) use enums::*;
pub(crate) use exprs::*;
pub(crate) use functions::*;
pub(crate) use typeclasses::*;
pub(crate) use types::*;

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

pub(crate) trait AstExt<T> {
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
    /// Parse + build a source file. A pest failure is returned as `Err` (the
    /// file is unrecoverable); *build* errors (unknown types, malformed
    /// signatures, …) are collected per item and returned alongside the modules
    /// that built successfully, so callers can report several at once.
    pub fn parse_source_file(
        ctx: &mut CompileCtx<'run>,
        src: &str,
        file: FileRef,
    ) -> Result<(Vec<ProgramModule<'run>>, Vec<AstError>), AstError> {
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

        let (map, errors) = build_program(ctx, program_pair, src, dm, file)?;
        let modules = map
            .into_iter()
            .map(|(module_name, functions)| ProgramModule {
                functions,
                module_name,
            })
            .collect::<Vec<_>>();
        Ok((modules, errors))
    }

    pub fn parse_stub(ctx: &mut CompileCtx<'run>, src: &str) -> Result<Self, AstError> {
        let fr = ctx.stub_file();
        let (modules, errors) = Self::parse_source_file(ctx, src, fr)?;
        // `parse_stub` keeps an all-or-nothing contract (tests rely on it): a
        // build error surfaces as `Err`, collapsing the collected list.
        if let Some(e) = errors.into_iter().next() {
            return Err(e);
        }
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

/// Functions grouped by the module they were declared in
type BuiltModules<'run> = Map<ModuleRef<'run>, Vec<Function<'run>>>;

pub fn build_program<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<'i, Rule>,
    src: &str,
    default_module: ModuleRef<'run>,
    file: FileRef,
) -> Result<(BuiltModules<'run>, Vec<AstError>), AstError> {
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
    let mut errors: Vec<AstError> = Vec::new();

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
    let mut mods = build_functions(ctx, children, src, default_module, file, &mut errors);
    default_fns.into_iter().for_each(|(module, f)| {
        mods.entry(module).or_default().push(f);
    });
    // Instance-set checks need every instance registered (build_functions did
    // that): a subclass instance requires its superclass instances for the same
    // head type (Calculus: Typeclasses, `requires`).
    // Skip them when functions already failed to build,
    // the instance set is partial and they would only cascade.
    if errors.is_empty() {
        check_superclass_instances(ctx)?;
        check_copy_instances(ctx)?;
    }
    Ok((mods, errors))
}

/// The declarations gathered in phase 1, to be resolved in later phases.
pub(crate) struct Collected<'i, 'run> {
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
pub(crate) struct PendingClass<'i> {
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
pub(crate) fn collect_declarations<'i, 'run>(
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

/// Record one `use src::name;` / `use src::*;` import into `cur_mod`'s scope.
/// (`use_decl = { "use" ~ use_path ~ ";" }`;
/// `use_path = { identifier ~ ("::" ~ identifier)* ~ ("::" ~ "*")? }`.)
pub(crate) fn collect_use<'run>(
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
