//! control flow graph construction

use petgraph::Directed;
use petgraph::Graph;
use petgraph::csr::DefaultIx;

use crate::lang::Expr;
use crate::lang::Program;

pub fn construct_cfg(_ast: &Program) -> anyhow::Result<Graph<Expr, (), Directed, DefaultIx>> {
    todo!()
}
