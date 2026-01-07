//! recursively find expressions in an AST and annotate them with variable
//! interactions

use crate::AnnotatedExpression;
use crate::lang::Program;

pub fn annotate(ast: &Program) -> anyhow::Result<Vec<AnnotatedExpression>> {
    println!("{ast:?}");
    todo!()
}
