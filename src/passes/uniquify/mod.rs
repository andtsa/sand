//! the uniquify pass of the compiler
//!
//! takes a program AST and ensures all variable and function names are unique
pub mod reserved;

use std::collections::BTreeMap;

use crate::ir_types::hhir::*;
use crate::passes::uniquify::reserved::RESERVED_FUNCTION_NAMES;
use crate::passes::uniquify::reserved::UniquifyError;
use crate::passes::uniquify::reserved::assert_unique;

/// A helper struct that captures the active scopes for all identifiers at the
/// program's various levels and offers the functionality to keep track of and
/// rename them.
struct Context {
    /// Each scope is represented as a BTreeMap from original names to renamed
    /// names and are stored in a stack-like vector, where the last element
    /// is the current scope.
    var_scopes: Vec<BTreeMap<String, String>>,

    /// Function and variable names live in different namespaces in order to
    /// allow function name shadowing without problems
    fun_scopes: BTreeMap<String, String>,

    /// A global counter used for generating unique names across the program.
    counter: usize,
}

impl Context {
    /// Create a new Context, initialize its counter to zero, and push two empty
    /// BTreeMaps as the variable and function scopes.
    /// # Returns
    /// An initialized empty Context.
    fn new() -> Self {
        let mut global = BTreeMap::new();

        for &name in RESERVED_FUNCTION_NAMES.iter() {
            global.insert(name.to_string(), name.to_string());
        }

        Self {
            var_scopes: vec![BTreeMap::new()],
            fun_scopes: global,
            counter: 0,
        }
    }

    /// Generates a new unique name for a given identifier by appending to it
    /// the current counter
    /// # Arguments
    /// * 'name' - The identifier to be renamed
    /// # Returns
    /// The string containing the identifier's new name
    fn rename(&mut self, name: &str) -> String {
        let id = self.counter;
        self.counter += 1;
        format!("{}_{}", name, id)
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
    /// The newly generated unique identifier as a string.
    fn bind_var(&mut self, name: &str) -> String {
        let new_name = self.rename(name);
        self.var_scopes
            .last_mut()
            .unwrap()
            .insert(name.to_string(), new_name.clone());
        new_name
    }

    /// Looks up the unique name associated with a variable in the scope
    /// stack from the innermost to the outermost scope.
    /// # Arguments
    /// * 'name' - The original identifier to look up.
    /// # Returns
    /// The currently active unique name for that identifier, or None if not
    /// bound.
    pub fn lookup_var_opt(&self, name: &str) -> Option<String> {
        for scope in self.var_scopes.iter().rev() {
            if let Some(n) = scope.get(name) {
                return Some(n.clone());
            }
        }
        None
    }

    /// Binds a given function to a newly generated unique name and stores it
    /// in the current function scope.
    /// # Arguments
    /// * 'name' - The original identifier to bind.
    /// # Returns
    /// The newly generated unique identifier as a string.
    pub fn bind_fun(&mut self, name: &str) -> String {
        let new_name = if name == "main" || RESERVED_FUNCTION_NAMES.contains(&name) {
            name.to_string()
        } else {
            self.rename(name)
        };
        self.fun_scopes.insert(name.to_string(), new_name.clone());
        new_name
    }

    /// Looks up the unique name associated with a function in the global scope
    /// # Arguments
    /// * 'name' - The original identifier to look up.
    /// # Returns
    /// The currently active unique name for that identifier, or None if not
    /// defined.
    pub fn lookup_fun_opt(&self, name: &str) -> Option<String> {
        self.fun_scopes.get(name).cloned()
    }
}

/// Offers the uniquify pass publicly via Program::uniquify
impl Program {
    /// Produces a version of the program where all variable and function names
    /// are unique.
    /// # Returns
    /// A new Program AST with all its names uniquified but with the same
    /// functionality.
    pub fn uniquify(&self) -> Result<Self, UniquifyError> {
        let mut u = Context::new();

        // First, bind all function names
        // Helps with recursive / mutually recursive functions
        for f in &self.0 {
            u.bind_fun(&f.name);
        }

        // Then, enter those functions and uniquify them
        let mut functions = Vec::new();
        for f in &self.0 {
            functions.push(uniquify_function(f, &mut u)?);
        }

        let ast = Program(functions);
        // propagate uniqueness errors from the reserved checks
        assert_unique(&ast)?;
        Ok(ast)
    }
}

/// Renames a single function, its parameters, and body.
/// # Arguments
/// * 'f' - The function to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new Function`AST with all identifiers uniquely renamed.
fn uniquify_function(f: &Function, u: &mut Context) -> Result<Function, UniquifyError> {
    u.enter_scope();

    let mut parameters: Vec<Parameter> = Vec::new();
    for p in &f.parameters {
        let new_name = u.bind_var(&p.name);
        parameters.push(Parameter {
            name: new_name,
            ty: p.ty,
            start: p.start,
            end: p.end,
        });
    }
    let body = uniquify_expr(&f.body, u)?; // Enter a new context and recursively uniquify its expressions

    u.exit_scope();

    let name = match u.lookup_fun_opt(&f.name) {
        Some(n) => n,
        None => {
            return Err(UniquifyError::UndefinedFunction {
                name: f.name.clone(),
                at: (f.name_start, f.name_end),
            });
        }
    };

    Ok(Function {
        name,
        name_start: f.name_start,
        name_end: f.name_end,
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
fn uniquify_expr(e: &Expr, u: &mut Context) -> Result<Expr, UniquifyError> {
    let expr = match &e.expr {
        Expression::If { cond, t, f } => Expression::If {
            cond: Box::new(uniquify_expr(cond, u)?),
            t: Box::new(uniquify_expr(t, u)?),
            f: Box::new(uniquify_expr(f, u)?),
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
            let mapped = match u.lookup_fun_opt(fn_name) {
                Some(n) => n,
                None => {
                    return Err(UniquifyError::UndefinedFunction {
                        name: fn_name.clone(),
                        at: (e.start, e.end),
                    });
                }
            };
            let args_res: Result<Vec<Expr>, UniquifyError> =
                args.iter().map(|a| uniquify_expr(a, u)).collect();
            Expression::Call {
                fn_name: mapped,
                args: args_res?,
            }
        }

        Expression::Var(name) => {
            let mapped = match u.lookup_var_opt(name) {
                Some(n) => n,
                None => {
                    return Err(UniquifyError::UnboundVariable {
                        name: name.clone(),
                        at: (e.start, e.end),
                    });
                }
            };
            Expression::Var(mapped)
        }
        Expression::Int(i) => Expression::Int(*i),
        Expression::Bool(b) => Expression::Bool(*b),
        Expression::Unit => Expression::Unit,

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
        start: e.start,
        end: e.end,
    })
}

/// Recursively traverses and uniquifies a statement AST.
/// # Arguments
/// * 'stmt' - The Statement to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new Statement with variable names uniquely renamed
fn uniquify_stmt(stmt: &Statement, u: &mut Context) -> Result<Statement, UniquifyError> {
    match stmt {
        Statement::Declaration {
            name,
            name_start,
            name_end,
            ty,
            val,
        } => {
            let val = uniquify_expr(val, u)?;
            let new_name = u.bind_var(name);
            Ok(Statement::Declaration {
                name: new_name,
                name_start: *name_start,
                name_end: *name_end,
                ty: *ty,
                val,
            })
        }

        Statement::Assignment {
            name,
            name_start,
            name_end,
            val,
        } => {
            let mapped = match u.lookup_var_opt(name) {
                Some(n) => n,
                None => {
                    return Err(UniquifyError::UnboundVariable {
                        name: name.clone(),
                        at: (*name_start, *name_end),
                    });
                }
            };
            let val = uniquify_expr(val, u)?;
            Ok(Statement::Assignment {
                name: mapped,
                name_start: *name_start,
                name_end: *name_end,
                val,
            })
        }

        Statement::Expr(e) => {
            let expr = uniquify_expr(e, u)?;
            Ok(Statement::Expr(expr))
        }
    }
}
