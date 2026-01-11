//! the uniquify pass of the compiler
//!
//! takes a program AST and ensures all variable and function names are unique
use std::collections::BTreeMap;

use anyhow::anyhow;

use crate::lang::*;
use crate::reserved::RESERVED_FUNCTION_NAMES;
use crate::reserved::assert_unique;
// ----------------------------------------------- Helper
// ------------------------------------------------------

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
    /// The currently active unique name for that identifier.
    /// # Panics
    /// If the identifier is not bound in any active scope (e.g. using a
    /// variable defined in an inner block, outside of that block).
    pub fn lookup_var(&self, name: &str) -> String {
        for scope in self.var_scopes.iter().rev() {
            if let Some(n) = scope.get(name) {
                return n.clone();
            }
        }
        panic!("Unbound variable: {}", name);
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
    /// The currently active unique name for that identifier.
    /// # Panics
    /// If the function is not defined in the global scope.
    pub fn lookup_fun(&self, name: &str) -> String {
        if let Some(n) = self.fun_scopes.get(name) {
            n.clone()
        } else {
            panic!("Undefined function: {}", name);
        }
    }
}

// ----------------------------------------------- Helper
// ------------------------------------------------------

/// Offers the uniquify pass publicly via Program::uniquify
impl Program {
    /// Produces a version of the program where all variable and function names
    /// are unique.
    /// # Returns
    /// A new Program AST with all its names uniquified but with the same
    /// functionality.
    pub fn uniquify(&self) -> anyhow::Result<Self> {
        let mut u = Context::new();

        // First, bind all function names
        // Helps with recursive / mutually recursive functions
        for f in &self.0 {
            u.bind_fun(&f.name);
        }

        // Then, enter those functions and uniquify them
        let functions = self
            .0
            .iter()
            .map(|f| uniquify_function(f, &mut u))
            .collect();

        let ast = Program(functions);
        assert_unique(&ast).map_err(|e| anyhow!("{e}"))?;
        Ok(ast)
    }
}

/// Renames a single function, its parameters, and body.
/// # Arguments
/// * 'f' - The function to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new Function`AST with all identifiers uniquely renamed.
fn uniquify_function(f: &Function, u: &mut Context) -> Function {
    u.enter_scope();

    let parameters = f
        .parameters
        .iter()
        .map(|p| {
            let new_name = u.bind_var(&p.name);
            Parameter {
                name: new_name,
                ty: p.ty,
            }
        })
        .collect();
    let body = uniquify_expr(&f.body, u); // Enter a new context and recursively uniquify its expressions

    u.exit_scope();

    Function {
        name: u.lookup_fun(&f.name),
        parameters,
        ret_type: f.ret_type,
        body,
    }
}

/// Recursively traverses and uniquifies an expression AST.
/// # Arguments
/// * 'e' - The Expression to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new 'Expr' with all identifiers renamed according to scope rules.
fn uniquify_expr(e: &Expr, u: &mut Context) -> Expr {
    let expr = match &e.expr {
        Expression::If { cond, t, f } => Expression::If {
            cond: Box::new(uniquify_expr(cond, u)),
            t: Box::new(uniquify_expr(t, u)),
            f: Box::new(uniquify_expr(f, u)),
        },

        Expression::While { cond, body } => Expression::While {
            cond: Box::new(uniquify_expr(cond, u)),
            body: Box::new(uniquify_expr(body, u)),
        },

        Expression::BinOp { left, op, right } => Expression::BinOp {
            left: Box::new(uniquify_expr(left, u)),
            op: *op,
            right: Box::new(uniquify_expr(right, u)),
        },

        Expression::UnOp { op, right } => Expression::UnOp {
            op: *op,
            right: Box::new(uniquify_expr(right, u)),
        },

        Expression::Call { fn_name, args } => Expression::Call {
            fn_name: u.lookup_fun(fn_name),
            args: args.iter().map(|a| uniquify_expr(a, u)).collect(),
        },

        Expression::Var(name) => Expression::Var(u.lookup_var(name)),
        Expression::Int(i) => Expression::Int(*i),
        Expression::Bool(b) => Expression::Bool(*b),
        Expression::Unit => Expression::Unit,

        Expression::Block { statements, expr } => {
            u.enter_scope();

            let statements = statements.iter().map(|s| uniquify_stmt(s, u)).collect();
            let expr = expr.as_ref().map(|e| Box::new(uniquify_expr(e, u)));

            u.exit_scope();

            Expression::Block { statements, expr }
        }
    };

    Expr {
        expr,
        start: e.start,
        end: e.end,
    }
}

/// Recursively traverses and uniquifies a statement AST.
/// # Arguments
/// * 'stmt' - The Statement to uniquify.
/// * 'u' - The entire current Context.
/// # Returns
/// A new Statement with variable names uniquely renamed
fn uniquify_stmt(stmt: &Statement, u: &mut Context) -> Statement {
    match stmt {
        Statement::Declaration { name, ty, val } => {
            let val = uniquify_expr(val, u);
            let new_name = u.bind_var(name);
            Statement::Declaration {
                name: new_name,
                ty: *ty,
                val,
            }
        }

        Statement::Assignment { name, val } => Statement::Assignment {
            name: u.lookup_var(name),
            val: uniquify_expr(val, u),
        },

        Statement::Expr(e) => Statement::Expr(uniquify_expr(e, u)),
    }
}
