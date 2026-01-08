//! recursively find expressions in an AST and annotate them with variable
//! interactions

use crate::AnnotatedExpression;
use crate::lang::Expr;

pub fn annotate(exprs: Vec<Expr>) -> anyhow::Result<Vec<AnnotatedExpression>> {
    println!("{exprs:?}");
    todo!()
}
