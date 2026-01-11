//! checks for reserved keywords

use std::collections::BTreeSet;

use crate::lang::Expr;
use crate::lang::Expression;
use crate::lang::Program;
use crate::lang::Statement;

pub const RESERVED_FUNCTION_NAMES: [&str; 6] =
    ["print", "println", "printf", "scanf", "read", "readline"];

/// Checks that all variable and function names in the provided program AST are
/// unique. It does so by traversing all blocks and collecting all declared
/// names in a BTreeSet.
///
/// # Arguments
/// * 'prog' - The Program AST to check
///
/// # Returns
/// 'Ok(())' if all names are unique; otherwise, 'Err(name)' for the first
/// duplicate it finds.
pub fn assert_unique(prog: &Program) -> Result<(), String> {
    let mut seen_funs: BTreeSet<String> = RESERVED_FUNCTION_NAMES
        .iter()
        .map(|s| s.to_string())
        .collect();

    for func in &prog.0 {
        if RESERVED_FUNCTION_NAMES.contains(&func.name.as_str())
            || !seen_funs.insert(func.name.clone())
        {
            return Err(format!("Duplicate function name: {}", func.name));
        }

        let mut local_seen_vars = BTreeSet::new();
        for param in &func.parameters {
            if !local_seen_vars.insert(param.name.clone()) {
                return Err(format!(
                    "Duplicate parameter name in function {}: {}",
                    func.name, param.name
                ));
            }
        }

        check_expr(&func.body, &mut local_seen_vars)?;
    }

    Ok(())
}

/// Recursively checks an expression AST for uniqueness of all declared
/// identifiers. # Arguments
/// * 'expr' - The expression to traverse.
/// * 'seen' - The set of already encountered names.
/// # Returns
/// 'Ok(())' if all names are unique, otherwise 'Err(name)'.
fn check_expr(expr: &Expr, seen: &mut BTreeSet<String>) -> Result<(), String> {
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
/// * 'stmt' - The statement to traverse.
/// * 'seen' - The set of already encountered names.
/// # Returns
/// 'Ok(())' if all names are unique, otherwise 'Err(name)'.
fn check_stmt(stmt: &Statement, seen: &mut BTreeSet<String>) -> Result<(), String> {
    match stmt {
        Statement::Declaration { name, val, .. } => {
            if !seen.insert(name.clone()) {
                return Err(format!("Duplicate variable name: {}", name));
            }
            check_expr(val, seen)
        }

        Statement::Assignment { val, .. } => check_expr(val, seen),

        Statement::Expr(e) => check_expr(e, seen),
    }
}
