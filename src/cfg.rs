//! control flow graph construction

use std::collections::HashMap;

use anyhow::Result;
use petgraph::Directed;
use petgraph::Graph;
use petgraph::graph::NodeIndex;

use crate::annotate::{get_dependencies};

use crate::lang::Expr;
use crate::lang::Expression;
use crate::lang::Function;
use crate::lang::Program;
use crate::lang::Statement;
use crate::AnnotatedExpression;


pub fn construct_cfg(ast: &Program) -> Result<Graph<AnnotatedExpression, (), Directed>> {
    let mut graph = Graph::<AnnotatedExpression, (), Directed>::new();
    let mut function_entries = HashMap::new();
    let mut function_exits = HashMap::new();

    // Add entry and exit nodes for every function
    for func in &ast.0 {
        let entry_annotated = AnnotatedExpression {
            expr: Expr {
                expr: Expression::Unit,
                start: (0, 0),
                end: (0, 0),
            },
            depends_on: vec![],
            mutates: vec![],
        };
        let entry = graph.add_node(entry_annotated);

        let exit_annotated = AnnotatedExpression {
            expr: Expr {
                expr: Expression::Unit,
                start: (0, 0),
                end: (0, 0),
            },
            depends_on: vec![],
            mutates: vec![],
        };
        let exit = graph.add_node(exit_annotated);

        function_entries.insert(func.name.clone(), entry);
        function_exits.insert(func.name.clone(), exit);
    }

    // Create cfg for each function and connect them if needed
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
    graph: &mut Graph<AnnotatedExpression, (), Directed>,
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
    graph: &mut Graph<AnnotatedExpression, (), Directed>,
    expr: &Expr,
    next: NodeIndex,
    function_entries: &HashMap<String, NodeIndex>,
    function_exits: &HashMap<String, NodeIndex>,
) -> Result<NodeIndex> {
    match &expr.expr {
        Expression::If { cond, t, f } => {
            let cond_annotated = AnnotatedExpression {
                expr: cond.as_ref().clone(),
                depends_on: get_dependencies(cond),
                mutates: vec![],
            };
            let cond_node = graph.add_node(cond_annotated);
            let then_entry = build_cfg_expr(graph, t, next, function_entries, function_exits)?;
            let else_entry = build_cfg_expr(graph, f, next, function_entries, function_exits)?;

            graph.add_edge(cond_node, then_entry, ());
            graph.add_edge(cond_node, else_entry, ());

            Ok(cond_node)
        }
        Expression::While { cond, body } => {
            let cond_annotated = AnnotatedExpression {
                expr: cond.as_ref().clone(),
                depends_on: get_dependencies(cond),
                mutates: vec![],
            };
            let cond_node = graph.add_node(cond_annotated);
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
                match stmt {
                    Statement::Declaration { name, val, .. } | Statement::Assignment { name, val } => {
                        current_node = build_cfg_expr(
                            graph,
                            val,
                            current_node,
                            function_entries,
                            function_exits,
                        )?;
                    },
                    Statement::Expr(e) => {
                        current_node = build_cfg_expr(
                            graph,
                            e,
                            current_node,
                            function_entries,
                            function_exits,
                        )?;
                    },
                }
            }

            Ok(current_node)
        }
        Expression::Call { fn_name, args } => {
            let mut current_node = next;

            let call_annotated = AnnotatedExpression {
                expr: expr.clone(),
                depends_on: get_dependencies(expr),
                mutates: vec![],
            };
            let call_node = graph.add_node(call_annotated);


            if let Some(&callee_entry) = function_entries.get(fn_name) {
                let callee_exit = function_exits[fn_name];

                graph.add_edge(call_node, callee_entry, ());
                graph.add_edge(callee_exit, next, ());

            } else {
                graph.add_edge(call_node, next, ());
            }

            current_node = call_node;

            for arg in args.iter().rev() {
                current_node =
                    build_cfg_expr(graph, arg, current_node, function_entries, function_exits)?;
            }

            Ok(current_node)
        }
        _ => {
            let annotated = AnnotatedExpression {
                expr: expr.clone(),
                depends_on: get_dependencies(expr),
                mutates: vec![],
            };
            let node = graph.add_node(annotated);
            graph.add_edge(node, next, ());
            Ok(node)
        }
    }
}
