//! checks for reserved keywords
//! TODO: remove dead code, consolidate with pass from hhir to qhir

use std::collections::BTreeMap;

use crate::ir_types::hhir::ProgramModule;
use crate::lang::intrinsics::fn_name_allowed;
use crate::compiler::structure::Range;

pub type FnSeenMap = BTreeMap<String, Range>;


/// Checks that all variable and function names
/// in the provided program AST are unique
///
/// # Arguments
/// * 'prog' - The Program AST to check
///
/// # Returns
/// 'Ok(())' if all names are unique; otherwise, 'Err' with a `UniquifyError`
pub fn assert_unique(prog: &ProgramModule) -> Result<(), UniquifyError> {
    // Map function name -> (start,end) of first occurrence
    let mut seen_funs: FnSeenMap = BTreeMap::new();

    for func in &prog.functions {
        // if function uses an internal reserved name -> illegal
        if !fn_name_allowed(&func.name) {
            return Err(UniquifyError::IllegalFunctionName {
                name: func.name.clone(),
                at: func.range,
            });
        }

        if let Some(first_span) = seen_funs.get(&func.name) {
            return Err(UniquifyError::DuplicateFunction {
                name: func.name.clone(),
                first_instance: *first_span,
                second_instance: func.range,
            });
        }
        // record this function's name span
        seen_funs.insert(func.name.clone(), func.range);

        // check parameters for duplicates within the same function,
        // mapping parameter name -> (start,end)
        // let mut param_seen: VarSeenMap = BTreeMap::new();
        // for param in &func.parameters {
        //     if let Some(first) = param_seen.get(&param.name) {
        //         return Err(UniquifyError::DuplicateParameterName {
        //             name: format!("{:?}", param.name),
        //             first_instance: *first,
        //             second_instance: param.range,
        //         });
        //     }
        //     param_seen.insert(param.name.clone(), param.range);
        // }

        // // check the function body using a name->span map for locals
        // let mut local_seen_vars: VarSeenMap = BTreeMap::new();
        // check_expr(&func.body, &mut local_seen_vars)?;
    }

    Ok(())
}

// /// Recursively checks an expression AST for uniqueness of all declared
// /// identifiers. # Arguments
// /// * 'expr' - the expression to traverse.
// /// * 'seen' - the map of already encountered names to the span of their first
// ///   occurrence.
// /// # Returns
// /// 'Ok(())' if all names are unique, otherwise `UniquifyError`.
// pub fn check_expr(expr: &Expr, seen: &mut VarSeenMap) -> Result<(), UniquifyError> {
//     match &expr.expr {
//         Expression::Int(_) | Expression::Bool(_) | Expression::Unit | Expression::Var(_) => {}
//         Expression::If { cond, t, f } => {
//             check_expr(cond, seen)?;
//             check_expr(t, seen)?;
//             check_expr(f, seen)?;
//         }

//         Expression::While { cond, body } => {
//             check_expr(cond, seen)?;
//             check_expr(body, seen)?;
//         }

//         Expression::BinOp { left, right, .. } => {
//             check_expr(left, seen)?;
//             check_expr(right, seen)?;
//         }

//         Expression::UnOp { right, .. } => {
//             check_expr(right, seen)?;
//         }

//         Expression::Call { args, .. } => {
//             for arg in args {
//                 check_expr(arg, seen)?;
//             }
//         }

//         Expression::Block {
//             statements,
//             expr: inner_expr,
//         } => {
//             let mut block_seen = seen.clone();
//             for stmt in statements {
//                 check_stmt(stmt, &mut block_seen)?;
//             }
//             if let Some(e) = inner_expr {
//                 check_expr(e, &mut block_seen)?;
//             }
//         }
//     }
//     Ok(())
// }

// /// Recursively checks a statement AST for uniqueness of all declared
// /// identifiers. # Arguments
// /// * 'stmt' - the statement to traverse.
// /// * 'seen' - the map of already encountered names to the span of their first
// ///   occurrence.
// /// # Returns
// /// 'Ok(())' if all names are unique, otherwise `UniquifyError`.
// pub fn check_stmt(stmt: &Statement, seen: &mut VarSeenMap) -> Result<(), UniquifyError> {
//     match stmt {
//         Statement::Declaration {
//             name,
//             range,
//             ty: _,
//             val,
//         } => {
//             if let Some(first_span) = seen.get(name) {
//                 return Err(UniquifyError::DuplicateVariableName {
//                     name: name.clone(),
//                     first_instance: *first_span,
//                     second_instance: *range,
//                 });
//             }
//             seen.insert(name.clone(), *range);
//             check_expr(val, seen)
//         }

//         Statement::Assignment { name: _, val, .. } => {
//             // assignment doesn't declare a new variable; it should refer to an existing one
//             // uniqueness checker only needs to traverse the RHS expression
//             check_expr(val, seen)
//         }

//         Statement::Expr(e) => check_expr(e, seen),
//     }
// }
