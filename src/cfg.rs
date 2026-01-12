//! control flow graph construction

use std::collections::HashMap;

use anyhow::Result;
use petgraph::Directed;
use petgraph::Graph;
use petgraph::graph::NodeIndex;

use crate::lang::Expr;
use crate::lang::Expression;
use crate::lang::Function;
use crate::lang::Program;
use crate::lang::Statement;

#[derive(Debug, Clone)]
pub enum CfgNode {
    Expr(Expr),
    FunctionEntry(String),
    FunctionExit(String),
}

pub fn construct_cfg(ast: &Program) -> Result<Graph<CfgNode, (), Directed>> {
    let mut graph = Graph::<CfgNode, (), Directed>::new();
    let mut function_entries = HashMap::new();
    let mut function_exits = HashMap::new();

    for func in &ast.0 {
        let entry = graph.add_node(CfgNode::FunctionEntry(func.name.clone()));
        let exit = graph.add_node(CfgNode::FunctionExit(func.name.clone()));
        function_entries.insert(func.name.clone(), entry);
        function_exits.insert(func.name.clone(), exit);
    }

    for func in &ast.0 {
        let entry = function_entries[&func.name];
        let exit = function_exits[&func.name];

        build_cfg_func(
            &mut graph,
            func,
            entry,
            exit,
            &function_entries,
            &function_exits,
        )?;
    }

    Ok(graph)
}

pub fn build_cfg_func(
    graph: &mut Graph<CfgNode, (), Directed>,
    func: &Function,
    entry: NodeIndex,
    exit: NodeIndex,
    function_entries: &HashMap<String, NodeIndex>,
    function_exits: &HashMap<String, NodeIndex>,
) -> Result<()> {
    let body_entry = build_cfg_expr(graph, &func.body, exit, function_entries, function_exits)?;

    graph.add_edge(entry, body_entry, ());

    Ok(())
}

fn build_cfg_expr(
    graph: &mut Graph<CfgNode, (), Directed>,
    expr: &Expr,
    next: NodeIndex,
    function_entries: &HashMap<String, NodeIndex>,
    function_exits: &HashMap<String, NodeIndex>,
) -> Result<NodeIndex> {
    match &expr.expr {
        Expression::If { cond, t, f } => {
            let cond_node = graph.add_node(CfgNode::Expr(cond.as_ref().clone()));
            let then_entry = build_cfg_expr(graph, t, next, function_entries, function_exits)?;
            let else_entry = build_cfg_expr(graph, f, next, function_entries, function_exits)?;

            graph.add_edge(cond_node, then_entry, ());
            graph.add_edge(cond_node, else_entry, ());

            Ok(cond_node)
        }
        Expression::While { cond, body } => {
            let cond_node = graph.add_node(CfgNode::Expr(cond.as_ref().clone()));
            let body_entry =
                build_cfg_expr(graph, body, cond_node, function_entries, function_exits)?;

            graph.add_edge(cond_node, body_entry, ());
            graph.add_edge(cond_node, next, ());

            Ok(cond_node)
        }
        Expression::Block {
            statements,
            expr: block_expr,
        } => {
            let mut current_node = next;

            if let Some(final_expr) = block_expr {
                current_node = build_cfg_expr(
                    graph,
                    final_expr,
                    current_node,
                    function_entries,
                    function_exits,
                )?;
            }

            for stmt in statements.iter().rev() {
                let stmt_expr = match stmt {
                    Statement::Declaration { val, .. } => val,
                    Statement::Assignment { val, .. } => val,
                    Statement::Expr(e) => e,
                };

                current_node = build_cfg_expr(
                    graph,
                    stmt_expr,
                    current_node,
                    function_entries,
                    function_exits,
                )?;
            }

            Ok(current_node)
        }
        Expression::Call { fn_name, args } => {
            let mut current_node = next;

            let return_node = graph.add_node(CfgNode::Expr(expr.clone()));

            if let Some(&callee_entry) = function_entries.get(fn_name) {
                let callee_exit = function_exits[fn_name];

                graph.add_edge(return_node, callee_entry, ());
                graph.add_edge(callee_exit, current_node, ());

                current_node = return_node;
            } else {
                graph.add_edge(return_node, current_node, ());
                current_node = return_node;
            }

            for arg in args {
                current_node =
                    build_cfg_expr(graph, arg, current_node, function_entries, function_exits)?;
            }

            Ok(current_node)
        }
        _ => {
            let node = graph.add_node(CfgNode::Expr(expr.clone()));
            graph.add_edge(node, next, ());
            Ok(node)
        }
    }
}
