//! checks for reserved keywords

use std::collections::BTreeMap;

use crate::lang::Expr;
use crate::lang::Expression;
use crate::lang::Program;
use crate::lang::Statement;

pub const RESERVED_FUNCTION_NAMES: [&str; 6] =
    ["print", "println", "printf", "scanf", "read", "readline"];

pub type SeenMap = BTreeMap<String, ((usize, usize), (usize, usize))>;

/// errors produced by the uniquify / reserved-name checking passes
#[derive(Debug)]
pub enum UniquifyError {
    UnboundVariable {
        name: String,
        at: ((usize, usize), (usize, usize)),
    },
    UndefinedFunction {
        name: String,
        at: ((usize, usize), (usize, usize)),
    },
    DuplicateFunction {
        name: String,
        first_instance: ((usize, usize), (usize, usize)),
        second_instance: ((usize, usize), (usize, usize)),
    },
    IllegalFunctionName {
        name: String,
        at: ((usize, usize), (usize, usize)),
    },
    DuplicateParameterName {
        name: String,
        first_instance: ((usize, usize), (usize, usize)),
        second_instance: ((usize, usize), (usize, usize)),
    },
    DuplicateVariableName {
        name: String,
        first_instance: ((usize, usize), (usize, usize)),
        second_instance: ((usize, usize), (usize, usize)),
    },
}

impl std::fmt::Display for UniquifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use UniquifyError::*;
        match self {
            UnboundVariable { name, at } => {
                write!(
                    f,
                    "unbound variable '{}' at span {:?}-{:?}",
                    name, at.0, at.1
                )
            }
            UndefinedFunction { name, at } => {
                write!(
                    f,
                    "undefined function '{}' at span {:?}-{:?}",
                    name, at.0, at.1
                )
            }
            DuplicateFunction {
                name,
                first_instance,
                second_instance,
            } => write!(
                f,
                "duplicate function '{}' at spans {:?}-{:?} and {:?}-{:?}",
                name, first_instance.0, first_instance.1, second_instance.0, second_instance.1
            ),
            IllegalFunctionName { name, at } => {
                write!(
                    f,
                    "illegal function name '{}' at span {:?}-{:?}",
                    name, at.0, at.1
                )
            }
            DuplicateParameterName {
                name,
                first_instance,
                second_instance,
            } => write!(
                f,
                "duplicate parameter '{}' at spans {:?}-{:?} and {:?}-{:?}",
                name, first_instance.0, first_instance.1, second_instance.0, second_instance.1
            ),
            DuplicateVariableName {
                name,
                first_instance,
                second_instance,
            } => write!(
                f,
                "duplicate variable '{}' at spans {:?}-{:?} and {:?}-{:?}",
                name, first_instance.0, first_instance.1, second_instance.0, second_instance.1
            ),
        }
    }
}

impl std::error::Error for UniquifyError {}

/// Checks that all variable and function names
/// in the provided program AST are unique
///
/// # Arguments
/// * 'prog' - The Program AST to check
///
/// # Returns
/// 'Ok(())' if all names are unique; otherwise, 'Err' with a `UniquifyError`
pub fn assert_unique(prog: &Program) -> Result<(), UniquifyError> {
    // Map function name -> (start,end) of first occurrence
    let mut seen_funs: SeenMap = BTreeMap::new();

    for func in &prog.0 {
        // if function uses an internal reserved name -> illegal
        if RESERVED_FUNCTION_NAMES.contains(&func.name.as_str()) {
            return Err(UniquifyError::IllegalFunctionName {
                name: func.name.clone(),
                at: (func.name_start, func.name_end),
            });
        }

        if let Some(first_span) = seen_funs.get(&func.name) {
            return Err(UniquifyError::DuplicateFunction {
                name: func.name.clone(),
                first_instance: *first_span,
                second_instance: (func.name_start, func.name_end),
            });
        }
        // record this function's name span
        seen_funs.insert(func.name.clone(), (func.name_start, func.name_end));

        // check parameters for duplicates within the same function,
        // mapping parameter name -> (start,end)
        let mut param_seen: SeenMap = BTreeMap::new();
        for param in &func.parameters {
            if let Some(first) = param_seen.get(&param.name) {
                return Err(UniquifyError::DuplicateParameterName {
                    name: param.name.clone(),
                    first_instance: *first,
                    second_instance: (param.start, param.end),
                });
            }
            param_seen.insert(param.name.clone(), (param.start, param.end));
        }

        // check the function body using a name->span map for locals
        let mut local_seen_vars: SeenMap = BTreeMap::new();
        check_expr(&func.body, &mut local_seen_vars)?;
    }

    Ok(())
}

/// Recursively checks an expression AST for uniqueness of all declared
/// identifiers. # Arguments
/// * 'expr' - the expression to traverse.
/// * 'seen' - the map of already encountered names to the span of their first
///   occurrence.
/// # Returns
/// 'Ok(())' if all names are unique, otherwise `UniquifyError`.
fn check_expr(expr: &Expr, seen: &mut SeenMap) -> Result<(), UniquifyError> {
    match &expr.expr {
        Expression::If { cond, t, f } => {
            check_expr(cond, seen)?;
            check_expr(t, seen)?;
            check_expr(f, seen)?;
        }

        Expression::While { cond, body } => {
            check_expr(cond, seen)?;
            check_expr(body, seen)?;
        }

        Expression::BinOp { left, right, .. } => {
            check_expr(left, seen)?;
            check_expr(right, seen)?;
        }

        Expression::UnOp { right, .. } => {
            check_expr(right, seen)?;
        }

        Expression::Call { args, .. } => {
            for arg in args {
                check_expr(arg, seen)?;
            }
        }

        Expression::Block {
            statements,
            expr: inner_expr,
        } => {
            let mut block_seen = seen.clone();
            for stmt in statements {
                check_stmt(stmt, &mut block_seen)?;
            }
            if let Some(e) = inner_expr {
                check_expr(e, &mut block_seen)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Recursively checks a statement AST for uniqueness of all declared
/// identifiers. # Arguments
/// * 'stmt' - the statement to traverse.
/// * 'seen' - the map of already encountered names to the span of their first
///   occurrence.
/// # Returns
/// 'Ok(())' if all names are unique, otherwise `UniquifyError`.
fn check_stmt(stmt: &Statement, seen: &mut SeenMap) -> Result<(), UniquifyError> {
    match stmt {
        Statement::Declaration {
            name,
            name_start,
            name_end,
            ty: _,
            val,
        } => {
            if let Some(first_span) = seen.get(name) {
                return Err(UniquifyError::DuplicateVariableName {
                    name: name.clone(),
                    first_instance: *first_span,
                    second_instance: (*name_start, *name_end),
                });
            }
            seen.insert(name.clone(), (*name_start, *name_end));
            check_expr(val, seen)
        }

        Statement::Assignment {
            name: _,
            name_start: _,
            name_end: _,
            val,
        } => {
            // assignment doesn't declare a new variable; it should refer to an existing one
            // uniqueness checker only needs to traverse the RHS expression
            check_expr(val, seen)
        }

        Statement::Expr(e) => check_expr(e, seen),
    }
}
