//! the uniquify pass of the compiler
//!
//! takes a program AST and ensures all variable and function names are unique

pub mod error;

use std::collections::BTreeMap;

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
    /// Each scope is represented as a BTreeMap from original names to renamed
    /// names and are stored in a stack-like vector, where the last element
    /// is the current scope.
    var_scopes: Vec<BTreeMap<String, UniqVar>>,

    // /// Function and variable names live in different namespaces in order to
    // /// allow function name shadowing without problems
    // fun_scopes: BTreeMap<String, String>,
    compile_ctx: &'uniq mut CompileCtx<'run>,
}

impl<'uniq, 'run> UniqCtx<'uniq, 'run> {
    /// Create a new Context, initialize its counter to zero, and push two empty
    /// BTreeMaps as the variable and function scopes.
    /// # Returns
    /// An initialized empty Context.
    fn new(ctx: &'uniq mut CompileCtx<'run>) -> Self {
        // let mut global = BTreeMap::new();

        // for &name in RESERVED_FUNCTION_NAMES.iter() {
        //     global.insert(name.to_string(), name.to_string());
        // }

        Self {
            compile_ctx: ctx,
            var_scopes: vec![BTreeMap::new()],
        }
    }

    /// Pushes a new empty scope onto the scope stack when entering a new block
    /// or function.
    fn enter_scope(&mut self) {
        self.var_scopes.push(BTreeMap::new());
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

    // /// Binds a given function to a newly generated unique name and stores it
    // /// in the current function scope.
    // /// # Arguments
    // /// * 'name' - The original identifier to bind.
    // /// # Returns
    // /// The newly generated unique identifier as a string.
    // pub fn bind_fun(&mut self, name: &str) -> String {
    //     let new_name = if name == "main" ||
    // RESERVED_FUNCTION_NAMES.contains(&name) {         name.to_string()
    //     } else {
    //         self.rename(name)
    //     };
    //     self.fun_scopes.insert(name.to_string(), new_name.clone());
    //     new_name
    // }

    // /// Looks up the unique name associated with a function in the global scope
    // /// # Arguments
    // /// * 'name' - The original identifier to look up.
    // /// # Returns
    // /// The currently active unique name for that identifier, or None if not
    // /// defined.
    // pub fn lookup_fun_opt(&self, name: &str) -> Option<String> {
    //     self.fun_scopes.get(name).cloned()
    // }
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
    let expr = match &e.expr {
        Expression::If { cond, t, f } => Expression::If {
            cond: Box::new(uniquify_expr(cond, u)?),
            t: Box::new(uniquify_expr(t, u)?),
            f: f.as_deref()
                .map(|e| uniquify_expr(e, u))
                .transpose()?
                .map(Box::new),
        },

        Expression::While { cond, body } => Expression::While {
            cond: Box::new(uniquify_expr(cond, u)?),
            body: Box::new(uniquify_expr(body, u)?),
        },

        Expression::BinOp { left, op, right } => Expression::BinOp {
            left: Box::new(uniquify_expr(left, u)?),
            op: *op,
            right: Box::new(uniquify_expr(right, u)?),
        },

        Expression::UnOp { op, right } => Expression::UnOp {
            op: *op,
            right: Box::new(uniquify_expr(right, u)?),
        },

        Expression::Call { fn_name, args } => {
            let args_res: Result<Vec<Expr>, UniquifyError> =
                args.iter().map(|a| uniquify_expr(a, u)).collect();
            Expression::Call {
                fn_name: fn_name.clone(),
                args: args_res?,
            }
        }

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
            Expression::Var(HirVar::Uniq(mapped))
        }
        Expression::Int(i) => Expression::Int(*i),
        Expression::Bool(b) => Expression::Bool(*b),
        Expression::Unit => Expression::Unit,
        Expression::Constructor { type_name, variant } => Expression::Constructor {
            type_name: type_name.clone(),
            variant: variant.clone(),
        },
        Expression::ExternalConstructor {
            mod_name,
            type_name,
            variant,
        } => Expression::ExternalConstructor {
            mod_name: mod_name.clone(),
            type_name: type_name.clone(),
            variant: variant.clone(),
        },
        Expression::Tag { variant } => Expression::Tag {
            variant: variant.clone(),
        },

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

            Expression::Block {
                statements: stmts,
                expr: inner_expr,
            }
        }
    };

    Ok(Expr {
        expr,

        range: e.range,
    })
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
