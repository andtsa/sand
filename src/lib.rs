//! analysis logic

use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;

use petgraph::graph::NodeIndex;

use crate::interactions::find_interactions;
use crate::lang::Expr;
use crate::lang::Program;

pub mod annotate;
pub mod ast;
pub mod cfg;
pub mod interactions;
pub mod interpret;
pub mod lang;
pub mod lsp;
pub mod parse;
pub mod reserved;
pub mod traits;
pub mod uniquify;

pub type TupleSpan = ((usize, usize), (usize, usize));
#[derive(Debug, Clone, Default)]
pub struct ProgramAnnotations {
    /// Map from each expression to all its occurrences in the source code
    pub expr_occurrences: HashMap<Expr, Vec<TupleSpan>>,

    /// Available-expressions set at each CFG node
    pub available_at: HashMap<NodeIndex, HashSet<Expr>>,
}

#[derive(Debug, Clone)]
pub struct AnnotatedExpression {
    pub expr: Expr,
    /// which variables does this expression depend on
    pub depends_on: HashSet<String>,
    /// which variables does this expression mutate
    pub mutates: HashSet<String>,
}

impl PartialEq for AnnotatedExpression {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl Eq for AnnotatedExpression {}

impl Hash for AnnotatedExpression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

#[allow(unused)]
pub fn analyse(program: &str) -> anyhow::Result<ProgramAnnotations> {
    // first parse the whole program into an AST
    let ast = Program::parse(program)?;

    // then uniquify all variable and function names
    let ast = ast.uniquify()?;

    // create the "control flow graph" - the order in which expressions are
    // evaluated. for example:
    // ```
    // let x = {
    //   let y = 5;
    //   y + 2
    // };
    // x * 3
    // ```
    // here the order of evaluation is
    // // 1. `let y = 5;`
    // // 2. `y + 2`
    // // 3. `let x = { ... };`
    // // 4. `x * 3`
    //
    // note that we need to traverse the AST recursively,
    // meaning that in the AST `a + (b * c)` we need to consider all of
    // `a`, `b`, `c`, `b * c`, and `a + (b * c)` as separate expressions.
    //
    // additionally, the control flow graph should branch for conditionals and
    // loops, and indicate indirection for function calls.

    // /!\ temporarily commented out
    let cfg = cfg::construct_cfg(&ast)?;
    // let cfg = Graph::<AnnotatedExpression, (), Directed>::new();

    // we iterate through the above vector,
    // and for every expression we count how many times it appeared,
    // keeping track of whether the variables it depends on are in the
    // same state as the other instances of the expression.
    let annotations: ProgramAnnotations = find_interactions(cfg)?;

    Ok(annotations)
}

pub fn visualise_cfg(program: &Program) -> anyhow::Result<String> {
    // construct the CFG
    let cfg = cfg::construct_cfg(program)?;

    // convert to dot format
    let dot = petgraph::dot::Dot::new(&cfg);

    Ok(format!("{dot:?}"))
}
