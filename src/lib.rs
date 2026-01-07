//! analysis logic

use std::collections::BTreeMap;

use crate::lang::Expression;
use crate::lang::Program;

pub mod ast;
pub mod interpret;
pub mod lang;
pub mod parse;
pub mod uniquify;

#[derive(Debug, Default)]
pub struct ProgramAnnotations {
    pub repeated_expressions: BTreeMap<Expression, usize>,
}

pub struct AnnotatedExpression {
    pub expr: Expression,
    /// which variables does this expression depend on
    pub depends_on: Vec<String>,
    /// which variables does this expression mutate
    pub mutates: Vec<String>,
}

#[allow(unused)]
pub fn analyse(program: &str) -> anyhow::Result<ProgramAnnotations> {
    // first parse the whole program into an AST
    let ast = Program::parse(program)?;

    // then uniquify all variable and function names
    let ast = ast.uniquify();

    // for every expression in the AST, find variable interactions.
    // note that we need to traverse the AST recursively,
    // meaning that in the AST `a + (b * c)` we need to consider all of
    // `a`, `b`, `c`, `b * c`, and `a + (b * c)` as separate expressions.
    //
    // we should also consider the order of evaluation, such that expressions later
    // in the vector do not affect earlier ones - this similar to a control flow
    // graph, but for our very much linear program.
    let expressions: Vec<AnnotatedExpression>;

    // we start building program annotations
    let mut annotations = ProgramAnnotations::default();
    // we iterate through the above vector,
    // and for every expression we need to consider the following cases:
    // - no variable reads: can be added to repeated expressions directly
    // - (todo)

    todo!()
}
