//! the uniquify pass of the compiler
//!
//! takes a program AST and ensures all variable and function names are unique

pub mod error;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Map;
use crate::compiler::structure::UniqVar;
use crate::internal_bug;
use crate::ir_types::hhir::*;
use crate::passes::qualify::uniquify::error::UniquifyError;

/// A helper struct that captures the active scopes for all identifiers at the
/// program's various levels and offers the functionality to keep track of and
/// rename them.
struct UniqCtx<'uniq, 'run> {
    /// Each scope is represented as a Map from original names to renamed
    /// names and are stored in a stack-like vector, where the last element
    /// is the current scope.
    var_scopes: Vec<Map<String, UniqVar>>,

    compile_ctx: &'uniq mut CompileCtx<'run>,
}

impl<'uniq, 'run> UniqCtx<'uniq, 'run> {
    /// Create a new Context, initialize its counter to zero, and push two empty
    /// Maps as the variable and function scopes.
    /// # Returns
    /// An initialized empty Context.
    fn new(ctx: &'uniq mut CompileCtx<'run>) -> Self {
        Self {
            compile_ctx: ctx,
            var_scopes: vec![Map::new()],
        }
    }

    /// Pushes a new empty scope onto the scope stack when entering a new block
    /// or function.
    fn enter_scope(&mut self) {
        self.var_scopes.push(Map::new());
    }

    /// Pops the top scope from the scope stack when exiting a block or
    /// function.
    fn exit_scope(&mut self) {
        self.var_scopes.pop();
    }

    /// Binds a given variable to a newly generated unique name and stores it
    /// in the current variable scope.
    /// # Arguments
    /// * 'name' - The original identifier to bind.
    /// # Returns
    /// The newly generated unique identifier
    fn bind_var(&mut self, name: &HirVar) -> UniqVar {
        let ovref = match name {
            HirVar::Decl(ovref) => *ovref,
            x => internal_bug!("uniquify binding a non-declaration {x:?}"),
        };

        let seen_as = self.compile_ctx.original_var_name(&ovref);
        let uniq = self.compile_ctx.uniquify_original_variable(ovref);

        self.var_scopes.last_mut().unwrap().insert(seen_as, uniq);
        uniq
    }

    /// Looks up the unique name associated with a variable in the scope
    /// stack from the innermost to the outermost scope.
    /// # Arguments
    /// * 'name' - The original identifier to look up.
    /// # Returns
    /// The currently active unique name for that identifier, or None if not
    /// bound.
    pub fn lookup_var_opt(&self, name: &HirVar) -> Option<UniqVar> {
        let HirVar::Unqualified(str_name) = name else {
            internal_bug!("uniquify tried resolving {name:?}");
        };
        for scope in self.var_scopes.iter().rev() {
            if let Some(n) = scope.get(str_name) {
                return Some(*n);
            }
        }
        None
    }

    pub fn display_hir_var(&self, hv: &HirVar) -> String {
        match hv {
            HirVar::Decl(ovref) => self.compile_ctx.original_var_name(ovref),
            HirVar::Uniq(uv) => self.compile_ctx.uniq_variable_name(uv),
            HirVar::Unqualified(s) => s.to_string(),
        }
    }
}

/// Offers the uniquify pass publicly via Program::uniquify
impl ProgramModule {
    /// Produces a version of the program where all variable and function names
    /// are unique.
    /// # Returns
    /// A new Program AST with all its names uniquified but with the same
    /// functionality.
    pub fn uniquify<'run>(&self, ctx: &mut CompileCtx<'run>) -> Result<Self, UniquifyError> {
        let mut u = UniqCtx::new(ctx);

        let mut functions = Vec::new();
        for f in &self.functions {
            functions.push(uniquify_function(f, &mut u)?);
        }

        let ast = ProgramModule {
            functions,
            module_name: self.module_name,
        };

        Ok(ast)
    }
}

/// Renames a single function, its parameters, and body.
/// # Arguments
/// * 'f' - The function to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new Function`AST with all identifiers uniquely renamed.
fn uniquify_function(f: &Function, u: &mut UniqCtx) -> Result<Function, UniquifyError> {
    u.enter_scope();

    let span = tracing::trace_span!(
        "uniquify_function",
        name = u.compile_ctx.original_fun_name(f.name)
    );
    let _enter = span.enter();

    let mut seen = Map::new();
    let mut parameters: Vec<Parameter> = Vec::new();
    for p in &f.parameters {
        tracing::trace!("parameter {p:?}");
        if let HirVar::Decl(x) = &p.name {
            let name = u.compile_ctx.original_var_name(x);
            if name != "_"
                && let Some(seen_at) = seen.insert(name.clone(), p.range)
            {
                return Err(UniquifyError::DuplicateParameterName {
                    name,
                    first_instance: seen_at,
                    second_instance: p.range,
                });
            }
        } else {
            internal_bug!("non decl parameter variable");
        }
        let new_name = u.bind_var(&p.name);
        parameters.push(Parameter {
            name: HirVar::Uniq(new_name),
            ty: p.ty,
            range: p.range,
            is_mutable: p.is_mutable,
        });
    }
    let body = uniquify_expr(&f.body, u)?; // Enter a new context and recursively uniquify its expressions

    u.exit_scope();
    drop(_enter);

    Ok(Function {
        name: f.name,
        range: f.range,
        parameters,
        ret_type: f.ret_type,
        body,
    })
}

/// Recursively traverses and uniquifies an expression AST.
/// # Arguments
/// * 'e' - The Expression to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new 'Expr' with all identifiers renamed according to scope rules.
fn uniquify_expr(e: &Expr, u: &mut UniqCtx) -> Result<Expr, UniquifyError> {
    match &e.expr {
        // `Var` is a leaf that nonetheless needs rewriting: look up its
        // current unique binding in the scope stack (or fail if unbound).
        Expression::Var(name) => {
            let mapped = match u.lookup_var_opt(name) {
                Some(n) => n,
                None => {
                    return Err(UniquifyError::UnboundVariable {
                        name: u.display_hir_var(name),
                        at: e.range,
                    });
                }
            };
            Ok(Expr {
                expr: Expression::Var(HirVar::Uniq(mapped)),
                range: e.range,
            })
        }

        // `Block` introduces a new lexical scope and contains `Statement`
        // children rather than bare `Expr`s, so it sits outside what
        // `traverse_subexprs` can express — handle its scoping explicitly.
        Expression::Block { statements, expr } => {
            u.enter_scope();

            let mut stmts = Vec::new();
            for s in statements {
                stmts.push(uniquify_stmt(s, u)?);
            }
            let inner_expr = match expr.as_ref() {
                Some(inner) => Some(Box::new(uniquify_expr(inner, u)?)),
                None => None,
            };

            u.exit_scope();

            Ok(Expr {
                expr: Expression::Block {
                    statements: stmts,
                    expr: inner_expr,
                },
                range: e.range,
            })
        }

        // `Match` patterns can introduce bindings (`Circle(r)`, `(a, b)`)
        // that are scoped to their arm's body — exactly like `Block`
        // introduces scoped locals. Each arm therefore gets its own scope:
        // walk the pattern first (minting `Decl -> Uniq` bindings via
        // `bind_var`, and rejecting names bound twice within one pattern —
        // see `UniquifyError::DuplicateBindingInPattern`), then uniquify the
        // body in that scope, then pop it before moving to the next arm.
        Expression::Match { scrutinee, arms } => {
            let scrutinee = Box::new(uniquify_expr(scrutinee, u)?);

            let mut new_arms = Vec::with_capacity(arms.len());
            for arm in arms {
                u.enter_scope();
                let mut seen: Map<String, crate::compiler::structure::Range> = Map::new();
                let pattern = uniquify_pattern(&arm.pattern, u, &mut seen)?;
                let body = uniquify_expr(&arm.body, u)?;
                u.exit_scope();
                new_arms.push(HirMatchArm {
                    pattern,
                    body,
                    range: arm.range,
                });
            }

            Ok(Expr {
                expr: Expression::Match {
                    scrutinee,
                    arms: new_arms,
                },
                range: e.range,
            })
        }

        // Every other node — `If`, `While`, `BinOp`, `UnOp`, `Call`,
        // and the constructor/literal leaves — is handled uniformly by the
        // `subexprs` traversal: recurse into each child with `uniquify_expr`
        // and let it rebuild the node around the results. This is
        // `traverseOf subexprs (uniquifyExpr u)` in lens terms, and replaces
        // what used to be many near-identical match arms.
        _ => e.traverse_subexprs(|sub| uniquify_expr(sub, u)),
    }
}

/// Recursively walks a match-arm pattern, minting fresh `Uniq` bindings for
/// every `Binding` leaf (scoped to the arm — the caller must have already
/// pushed a fresh scope) and rejecting names that are bound more than once
/// within the *same* pattern (`(x, x)`, `Pair(x, x)`) — see
/// `UniquifyError::DuplicateBindingInPattern`. All other nodes are recursed
/// into structurally and otherwise passed through unchanged (their
/// `type_name`/`variant` string fields are resolved later, in `qualify`).
fn uniquify_pattern(
    pattern: &HirPattern,
    u: &mut UniqCtx,
    seen: &mut Map<String, crate::compiler::structure::Range>,
) -> Result<HirPattern, UniquifyError> {
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
        HirPattern::Wildcard => Ok(HirPattern::Wildcard),
    }
}

/// Recursively traverses and uniquifies a statement AST.
/// # Arguments
/// * 'stmt' - The Statement to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new Statement with variable names uniquely renamed
fn uniquify_stmt(stmt: &Statement, u: &mut UniqCtx) -> Result<Statement, UniquifyError> {
    match stmt {
        Statement::Declaration {
            name,
            range,
            ty,
            is_mutable,
            val,
        } => {
            let val = uniquify_expr(val, u)?;
            let new_name = u.bind_var(name);
            Ok(Statement::Declaration {
                name: HirVar::Uniq(new_name),
                range: *range,
                ty: *ty,
                is_mutable: *is_mutable,
                val,
            })
        }

        Statement::Assignment { name, range, val } => {
            let mapped = match u.lookup_var_opt(name) {
                Some(n) => n,
                None => {
                    return Err(UniquifyError::UnboundVariable {
                        name: u.display_hir_var(name),
                        at: *range,
                    });
                }
            };
            let val = uniquify_expr(val, u)?;
            Ok(Statement::Assignment {
                name: HirVar::Uniq(mapped),
                range: *range,
                val,
            })
        }

        Statement::Expr(e) => {
            let expr = uniquify_expr(e, u)?;
            Ok(Statement::Expr(expr))
        }
    }
}
