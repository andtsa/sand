//! control flow graph construction

use petgraph::graph::NodeIndex;
use petgraph::Directed;
use petgraph::Graph;
use petgraph::csr::DefaultIx;

use crate::lang::{Expr, Expression, Program, Statement};

pub fn construct_cfg(ast: &Program) -> anyhow::Result<Graph<Expr, (), Directed, DefaultIx>> {
    let mut graph = Graph::<Expr, (), Directed, DefaultIx>::new();

    if let Some(function) = ast.0.first() {
        let _entry = build_cfg_expr(&mut graph, &function.body, None)?;
    }

    Ok(graph)
}

fn build_cfg_expr(
    graph: &mut Graph<Expr, (), Directed, DefaultIx>,
    expr: &Expr,
    exit: Option<NodeIndex>,
) -> anyhow::Result<NodeIndex> {
    match &expr.expr {
        Expression::If { cond, t , f } => {
            let cond_node = graph.add_node(cond.as_ref().clone());

            let then_entry = build_cfg_expr(graph, t, exit)?;
            graph.add_edge(cond_node, then_entry, ());

            let else_entry = build_cfg_expr(graph, f, exit)?;
            graph.add_edge(then_entry, else_entry, ());

            Ok(cond_node)
        }
        Expression::While { cond, body } => {
            let cond_node = graph.add_node(cond.as_ref().clone());

            let body_entry = build_cfg_expr(graph, body, Some(cond_node))?;
            graph.add_edge(cond_node, body_entry, ());

            if let Some(exit_node) = exit {
                graph.add_edge(cond_node, exit_node, ());
            }

            Ok(cond_node)
        }
        Expression::Block { statements, expr: block_expr} => {
            let mut current_node = None;

            for stmt in statements {
                let stmt_expr = match stmt {
                    Statement::Declaration { val, .. } => val,
                    Statement::Assignment {val, .. } => val,
                    Statement::Expr(e) => e,
                };

                let stmt_node = build_cfg_expr(graph, stmt_expr, exit)?;

                if let Some(prev) = current_node {
                    graph.add_edge(prev, stmt_node, ());
                }

                current_node = Some(stmt_node);
            }

            if let Some(final_expr) = block_expr {
                let final_node = build_cfg_expr(graph, final_expr, exit)?;

                if let Some(prev) = current_node {
                    graph.add_edge(prev, final_node, ());
                }

                current_node = Some(final_node);
            }

            if current_node.is_none() {
                current_node = Some(graph.add_node(expr.clone()));
            }

            Ok(current_node.unwrap())
        }
        _ => {
            let node  = graph.add_node(expr.clone());

            if let Some(exit_node) = exit {
                graph.add_edge(node, exit_node, ());
            }

            Ok(node)
        }

    }
}
