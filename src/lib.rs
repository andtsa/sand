//! analysis logic

use anyhow::anyhow;

use crate::annotate::annotate;
use crate::interactions::find_interactions;
use crate::lang::Expr;
use crate::lang::Program;

pub mod annotate;
pub mod ast;
pub mod cfg;
pub mod interactions;
pub mod interpret;
pub mod lang;
pub mod parse;
pub mod reserved;
pub mod uniquify;

#[derive(Debug, Clone, Default)]
pub struct ProgramAnnotations {
    /// each `Expr` contains metadata on where it appears in the code,
    /// so we need to store all the instances of repeated expressions
    /// in order to visualise them later
    pub repeated_expressions: Vec<Vec<Expr>>,
}

#[derive(Debug, Clone)]
pub struct AnnotatedExpression {
    pub expr: Expr,
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
    let cfg = cfg::construct_cfg(&ast)?;

    // to annotate the program,
    // we need to use the topological ordering of the CFG
    let order: Vec<Expr> = petgraph::algo::toposort(&cfg, None)
        .map_err(|e| anyhow!("cycle in cfg: {e:?}"))?
        .into_iter()
        .filter_map(|node_idx| {
            match cfg.node_weight(node_idx).unwrap() {
                cfg::CfgNode::Expr(expr) => Some(expr.clone()),
                cfg::CfgNode::FunctionEntry(_) | cfg::CfgNode::FunctionExit(_) => None,
            }
        })
        .collect();

    // for every expression in the AST, find variable interactions.
    let expressions: Vec<AnnotatedExpression> = annotate(order)?;

    // we iterate through the above vector,
    // and for every expression we count how many times it appeared,
    // keeping track of whether the variables it depends on are in the
    // same state as the other instances of the expression.
    let annotations: ProgramAnnotations = find_interactions(expressions)?;

    Ok(annotations)
}

pub fn visualise_cfg(program: &Program) -> anyhow::Result<String> {
    // construct the CFG
    let cfg = cfg::construct_cfg(program)?;

    // convert to dot format
    let dot = petgraph::dot::Dot::new(&cfg);

    Ok(format!("{dot:?}"))
}
