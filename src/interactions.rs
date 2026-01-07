//! find repeated expressions in a program,
//! keeping track of variable interactions

use crate::AnnotatedExpression;
use crate::ProgramAnnotations;

pub fn find_interactions(prog: Vec<AnnotatedExpression>) -> anyhow::Result<ProgramAnnotations> {
    println!("{prog:?}");
    todo!()
}
