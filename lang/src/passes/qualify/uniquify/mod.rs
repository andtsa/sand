//! the uniquify pass of the compiler
//!
//! takes a program AST and ensures all variable and function names are unique.
//!
//! ## Monad-transformer rewrite
//!
//! This pass is the textbook `State (ScopeStack, &mut CompileCtx) (Except
//! UniquifyError)` computation, so it is written against the small transformer
//! toolkit in [`crate::compiler::structure::mtl`]. The monad threads the scope
//! stack *and* the `&mut CompileCtx` (by move: the State monad lets a `&mut`
//! flow linearly without aliasing) and short-circuits on the first error; the
//! pass code only sequences primitive actions with [`mdo!`] and `traverse`.
//!
//! Three lifetimes appear in the computation type [`Uniq`]:
//! * `'a`: the borrow of the *input* AST that an action's closures capture;
//! * `'u`: the borrow of the `&mut CompileCtx` carried in the state;
//! * `'tcx`: the arena lifetime of the IR.
//!
//! Keeping `'u` separate from `'a` is what lets a child action (whose captured
//! borrow `'a` is shorter) thread the *same* state value.

pub mod error;

use im::HashMap as Map;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::compiler::structure::mtl::ExW;
use crate::compiler::structure::mtl::StateT;
use crate::compiler::structure::mtl::state;
use crate::compiler::structure::mtl::traverse;
use crate::internal_bug;
use crate::ir_types::hhir::*;
use crate::mdo;
use crate::passes::qualify::uniquify::error::UniquifyError;

/// The uniquify monad: a `State`/`Except` stack over the scope context.
type Uniq<'a, 'u, 'tcx, A> = StateT<'a, UniqCtx<'u, 'tcx>, ExW<UniquifyError>, A>;

/// A helper struct that captures the active scopes for all identifiers at the
/// program's various levels and offers the functionality to keep track of and
/// rename them. It is the *state* threaded by the [`Uniq`] monad.
struct UniqCtx<'uniq, 'run> {
    /// Each scope is a Map from original names to renamed names, stored in a
    /// stack (the last element is the current scope).
    var_scopes: Vec<Map<String, UniqVar<'run>>>,
    compile_ctx: &'uniq mut CompileCtx<'run>,
}

impl<'uniq, 'run> UniqCtx<'uniq, 'run> {
    fn new(ctx: &'uniq mut CompileCtx<'run>) -> Self {
        Self {
            compile_ctx: ctx,
            var_scopes: vec![Map::new()],
        }
    }

    fn enter_scope(&mut self) {
        self.var_scopes.push(Map::new());
    }

    fn exit_scope(&mut self) {
        self.var_scopes.pop();
    }

    /// Bind a declaration variable to a freshly generated unique name in the
    /// current scope, returning the new name.
    fn bind_var(&mut self, name: &HirVar<'run>) -> UniqVar<'run> {
        let ovref = match name {
            HirVar::Decl(ovref) => *ovref,
            x => internal_bug!("uniquify binding a non-declaration {x:?}"),
        };
        let seen_as = self.compile_ctx.original_var_name(&ovref);
        let uniq = self.compile_ctx.uniquify_original_variable(ovref);
        self.var_scopes.last_mut().unwrap().insert(seen_as, uniq);
        uniq
    }

    /// Look up the unique name bound to `name`, innermost scope first.
    fn lookup_var_opt(&self, name: &HirVar<'run>) -> Option<UniqVar<'run>> {
        let HirVar::Unqualified(str_name) = name else {
            internal_bug!("uniquify tried resolving {name:?}");
        };
        self.var_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(str_name).copied())
    }

    fn display_hir_var(&self, hv: &HirVar<'run>) -> String {
        match hv {
            HirVar::Decl(ovref) => self.compile_ctx.original_var_name(ovref),
            HirVar::Uniq(uv) => self.compile_ctx.uniq_variable_name(uv),
            HirVar::Unqualified(s) => s.to_string(),
        }
    }
}

// primitive monadic actions over the scope state

/// Push a fresh lexical scope.
fn enter_scope<'a, 'u, 'tcx>() -> Uniq<'a, 'u, 'tcx, ()>
where
    'u: 'a,
    'tcx: 'a,
{
    StateT::modify(|mut u: UniqCtx<'u, 'tcx>| {
        u.enter_scope();
        u
    })
}

/// Pop the innermost lexical scope.
fn exit_scope<'a, 'u, 'tcx>() -> Uniq<'a, 'u, 'tcx, ()>
where
    'u: 'a,
    'tcx: 'a,
{
    StateT::modify(|mut u: UniqCtx<'u, 'tcx>| {
        u.exit_scope();
        u
    })
}

/// Create a fresh unique name for a declaration variable and record it.
fn fresh<'a, 'u, 'tcx>(name: HirVar<'tcx>) -> Uniq<'a, 'u, 'tcx, UniqVar<'tcx>>
where
    'u: 'a,
    'tcx: 'a,
{
    state(move |mut u: UniqCtx<'u, 'tcx>| {
        let v = u.bind_var(&name);
        (v, u)
    })
}

/// Resolve a use occurrence to its bound unique name, or fail with
/// `UnboundVariable`.
fn resolve<'a, 'u, 'tcx>(name: HirVar<'tcx>, at: Range) -> Uniq<'a, 'u, 'tcx, UniqVar<'tcx>>
where
    'u: 'a,
    'tcx: 'a,
{
    StateT::new(move |u: UniqCtx<'u, 'tcx>| match u.lookup_var_opt(&name) {
        Some(v) => Ok((v, u)),
        None => Err(UniquifyError::UnboundVariable {
            name: u.display_hir_var(&name),
            at,
        }),
    })
}

/// If `name` is *not* a known function but *is* a bound local, return that
/// local (an indirect call: applying a function value). Otherwise `None`.
fn indirect_callee<'a, 'u, 'tcx>(name: String) -> Uniq<'a, 'u, 'tcx, Option<UniqVar<'tcx>>>
where
    'u: 'a,
    'tcx: 'a,
{
    state(move |u: UniqCtx<'u, 'tcx>| {
        let r = if u.compile_ctx.lookup_function_by_name(&name).is_none() {
            u.lookup_var_opt(&HirVar::Unqualified(name))
        } else {
            None
        };
        (r, u)
    })
}

/// the core of the uniquify pass
impl<'tcx> ProgramModule<'tcx> {
    /// Produce a version of the program where all variable names are unique.
    pub fn uniquify(&self, ctx: &mut CompileCtx<'tcx>) -> Result<Self, UniquifyError> {
        let module_name = self.module_name;
        let program = traverse(self.functions.iter(), uniquify_function);
        let (functions, _final_state) = program.run(UniqCtx::new(ctx))?;
        Ok(ProgramModule {
            functions,
            module_name,
        })
    }
}

/// Uniquify a function: a fresh scope for its parameters + body.
fn uniquify_function<'a, 'u, 'tcx>(f: &'a Function<'tcx>) -> Uniq<'a, 'u, 'tcx, Function<'tcx>>
where
    'u: 'a,
    'tcx: 'a,
{
    mdo! {
        enter_scope();
        parameters <- bind_parameters(&f.parameters);
        body <- uniquify_expr(&f.body);
        exit_scope();
        StateT::pure(Function {
            name: f.name,
            range: f.range,
            type_params: f.type_params.clone(),
            region_params: f.region_params.clone(),
            where_constraints: f.where_constraints.clone(),
            type_constraints: f.type_constraints.clone(),
            parameters,
            ret_type: f.ret_type,
            body,
        })
    }
}

/// Bind every parameter to a fresh name, rejecting duplicate parameter names.
///
/// this is a self-contained imperative step over the threaded state (a monadic
/// primitive, like `get`/`put`).
fn bind_parameters<'a, 'u, 'tcx>(
    params: &'a [Parameter<'tcx>],
) -> Uniq<'a, 'u, 'tcx, Vec<Parameter<'tcx>>>
where
    'u: 'a,
    'tcx: 'a,
{
    StateT::new(move |mut u: UniqCtx<'u, 'tcx>| {
        let mut seen: Map<String, Range> = Map::new();
        let mut out = Vec::with_capacity(params.len());
        for p in params {
            let HirVar::Decl(x) = &p.name else {
                internal_bug!("non decl parameter variable");
            };
            let name = u.compile_ctx.original_var_name(x);
            if name != "_"
                && let Some(first_instance) = seen.insert(name.clone(), p.range)
            {
                return Err(UniquifyError::DuplicateParameterName {
                    name,
                    first_instance,
                    second_instance: p.range,
                });
            }
            let new_name = u.bind_var(&p.name);
            out.push(Parameter {
                name: HirVar::Uniq(new_name),
                ty: p.ty,
                range: p.range,
                is_mutable: p.is_mutable,
            });
        }
        Ok((out, u))
    })
}

/// Uniquify an expression, threading scopes through every sub-computation.
fn uniquify_expr<'a, 'u, 'tcx>(e: &'a Expr<'tcx>) -> Uniq<'a, 'u, 'tcx, Expr<'tcx>>
where
    'u: 'a,
    'tcx: 'a,
{
    let range = e.range;
    match &e.expr {
        // a use occurrence: resolve it to its current unique binding.
        Expression::Var(name) => resolve(name.clone(), range).map(move |v| Expr {
            expr: Expression::Var(HirVar::Uniq(v)),
            range,
        }),

        // a block introduces a fresh lexical scope around its statements + tail.
        Expression::Block { statements, expr } => mdo! {
            enter_scope();
            stmts <- traverse(statements.iter(), uniquify_stmt);
            tail <- uniquify_opt(expr.as_deref());
            exit_scope();
            StateT::pure(Expr {
                expr: Expression::Block { statements: stmts, expr: tail.map(Box::new) },
                range,
            })
        },

        // each match arm gets its own scope (its pattern binds arm-local names).
        Expression::Match { scrutinee, arms } => mdo! {
            scrut <- uniquify_expr(scrutinee);
            arms <- traverse(arms.iter(), uniquify_arm);
            StateT::pure(Expr {
                expr: Expression::Match { scrutinee: Box::new(scrut), arms },
                range,
            })
        },

        // a lambda introduces a fresh scope binding its parameter.
        Expression::Lambda { param, body, mode } => {
            let mode = *mode;
            let pty = param.ty;
            let prange = param.range;
            let pmut = param.is_mutable;
            let pname = param.name.clone();
            mdo! {
                enter_scope();
                new_name <- fresh(pname);
                new_body <- uniquify_expr(body);
                exit_scope();
                StateT::pure(Expr {
                    expr: Expression::Lambda {
                        param: Parameter {
                            name: HirVar::Uniq(new_name),
                            ty: pty,
                            range: prange,
                            is_mutable: pmut,
                        },
                        body: Box::new(new_body),
                        mode,
                    },
                    range,
                })
            }
        }

        // a single-argument call to a bare `Local` name *may* be an indirect
        // call (applying a function value) if the name is a bound local and
        // not a known function; otherwise it is an ordinary call.
        Expression::Call {
            fn_name: HirFnCall::Local(name),
            args,
            type_args,
        } if type_args.is_empty() && args.len() == 1 => {
            let name = name.clone();
            mdo! {
                callee <- indirect_callee(name);
                match callee {
                    Some(var) => uniquify_expr(&args[0]).map(move |arg| Expr {
                        expr: Expression::Apply {
                            func: Box::new(Expr { expr: Expression::Var(HirVar::Uniq(var)), range }),
                            arg: Box::new(arg),
                        },
                        range,
                    }),
                    None => uniquify_subexprs(e),
                }
            }
        }

        // every other node has no scoping of its own: recurse uniformly into
        // its children (this is `traverseOf subexprs uniquify_expr`).
        _ => uniquify_subexprs(e),
    }
}

/// Uniquify an optional expression (`None` stays `None`).
fn uniquify_opt<'a, 'u, 'tcx>(e: Option<&'a Expr<'tcx>>) -> Uniq<'a, 'u, 'tcx, Option<Expr<'tcx>>>
where
    'u: 'a,
    'tcx: 'a,
{
    match e {
        Some(inner) => uniquify_expr(inner).map(Some),
        None => StateT::pure(None),
    }
}

/// Recurse uniformly into a node's immediate sub-expressions, threading the
/// scope state through each. Bridges the `Result`-returning
/// [`Expr::traverse_subexprs`] lens by shuttling the state through it.
fn uniquify_subexprs<'a, 'u, 'tcx>(e: &'a Expr<'tcx>) -> Uniq<'a, 'u, 'tcx, Expr<'tcx>>
where
    'u: 'a,
    'tcx: 'a,
{
    StateT::new(move |u| {
        let mut slot = Some(u);
        let rebuilt = e.traverse_subexprs(|sub| {
            let cur = slot.take().expect("state present before child");
            let (child, next) = uniquify_expr(sub).run(cur)?;
            slot = Some(next);
            Ok(child)
        });
        rebuilt.map(|expr| (expr, slot.take().expect("state present after children")))
    })
}

/// Uniquify one match arm in its own scope.
fn uniquify_arm<'a, 'u, 'tcx>(arm: &'a HirMatchArm<'tcx>) -> Uniq<'a, 'u, 'tcx, HirMatchArm<'tcx>>
where
    'u: 'a,
    'tcx: 'a,
{
    let range = arm.range;
    mdo! {
        enter_scope();
        pattern <- uniquify_pattern_action(&arm.pattern);
        body <- uniquify_expr(&arm.body);
        exit_scope();
        StateT::pure(HirMatchArm { pattern, body, range })
    }
}

/// Uniquify a statement, threading scopes (and introducing bindings where the
/// statement declares names).
fn uniquify_stmt<'a, 'u, 'tcx>(stmt: &'a Statement<'tcx>) -> Uniq<'a, 'u, 'tcx, Statement<'tcx>>
where
    'u: 'a,
    'tcx: 'a,
{
    match stmt {
        // bind the RHS *before* introducing the new name (the RHS must not see it).
        Statement::Declaration {
            name,
            range,
            ty,
            is_mutable,
            val,
        } => {
            let (range, ty, is_mutable, name) = (*range, *ty, *is_mutable, name.clone());
            mdo! {
                val <- uniquify_expr(val);
                new_name <- fresh(name);
                StateT::pure(Statement::Declaration {
                    name: HirVar::Uniq(new_name),
                    range,
                    ty,
                    is_mutable,
                    val,
                })
            }
        }

        Statement::Assignment { name, range, val } => {
            let (range, name) = (*range, name.clone());
            mdo! {
                mapped <- resolve(name, range);
                val <- uniquify_expr(val);
                StateT::pure(Statement::Assignment {
                    name: HirVar::Uniq(mapped),
                    range,
                    val,
                })
            }
        }

        Statement::DerefAssign {
            reference,
            value,
            range,
        } => {
            let range = *range;
            mdo! {
                reference <- uniquify_expr(reference);
                value <- uniquify_expr(value);
                StateT::pure(Statement::DerefAssign { reference, value, range })
            }
        }

        // bind the RHS before introducing the tuple element names.
        Statement::LetTuple {
            elems,
            ty,
            val,
            range,
        } => {
            let (range, ty) = (*range, *ty);
            mdo! {
                val <- uniquify_expr(val);
                new_elems <- traverse(elems.iter().cloned(), |(name, is_mutable, elem_range)| {
                    fresh(name).map(move |v| (HirVar::Uniq(v), is_mutable, elem_range))
                });
                StateT::pure(Statement::LetTuple { elems: new_elems, ty, val, range })
            }
        }

        // bind the RHS and else-branch before binding the pattern variables.
        Statement::LetPattern {
            pattern,
            ty,
            val,
            else_branch,
            range,
        } => {
            let (range, ty) = (*range, *ty);
            mdo! {
                val <- uniquify_expr(val);
                else_branch <- uniquify_expr(else_branch);
                pattern <- uniquify_pattern_action(pattern);
                StateT::pure(Statement::LetPattern { pattern, ty, val, else_branch, range })
            }
        }

        Statement::Expr(e) => uniquify_expr(e).map(Statement::Expr),
    }
}

/// Uniquify a match-arm pattern as a monadic action
fn uniquify_pattern_action<'a, 'u, 'tcx>(
    pattern: &'a HirPattern<'tcx>,
) -> Uniq<'a, 'u, 'tcx, HirPattern<'tcx>>
where
    'u: 'a,
    'tcx: 'a,
{
    StateT::new(move |mut u| {
        let mut seen: Map<String, Range> = Map::new();
        uniquify_pattern(pattern, &mut u, &mut seen).map(|p| (p, u))
    })
}

/// The pattern walk creates fresh `Uniq` bindings for every `Binding` leaf and
/// rejects names bound twice within the *same* pattern.
fn uniquify_pattern<'tcx>(
    pattern: &HirPattern<'tcx>,
    u: &mut UniqCtx<'_, 'tcx>,
    seen: &mut Map<String, Range>,
) -> Result<HirPattern<'tcx>, UniquifyError> {
    match pattern {
        HirPattern::Constructor {
            type_name,
            variant,
            payload,
        } => Ok(HirPattern::Constructor {
            type_name: type_name.clone(),
            variant: variant.clone(),
            payload: payload
                .as_deref()
                .map(|p| uniquify_pattern(p, u, seen))
                .transpose()?
                .map(Box::new),
        }),
        HirPattern::Tag { variant, payload } => Ok(HirPattern::Tag {
            variant: variant.clone(),
            payload: payload
                .as_deref()
                .map(|p| uniquify_pattern(p, u, seen))
                .transpose()?
                .map(Box::new),
        }),
        HirPattern::Tuple(elems) => Ok(HirPattern::Tuple(
            elems
                .iter()
                .map(|p| uniquify_pattern(p, u, seen))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        HirPattern::Binding { var, range } => {
            let name = u.display_hir_var(var);
            if name != "_"
                && let Some(first_instance) = seen.insert(name.clone(), *range)
            {
                return Err(UniquifyError::DuplicateBindingInPattern {
                    name,
                    first_instance,
                    second_instance: *range,
                });
            }
            let uniq = u.bind_var(var);
            Ok(HirPattern::Binding {
                var: HirVar::Uniq(uniq),
                range: *range,
            })
        }
        HirPattern::IntLit(n) => Ok(HirPattern::IntLit(*n)),
        HirPattern::BoolLit(b) => Ok(HirPattern::BoolLit(*b)),
        HirPattern::Wildcard => Ok(HirPattern::Wildcard),
    }
}
