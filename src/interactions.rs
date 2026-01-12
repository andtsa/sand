//! find repeated expressions in a program,
//! keeping track of variable interactions

use std::collections::HashMap;
use std::collections::HashSet;

use petgraph::Directed;
use petgraph::Graph;
use petgraph::graph::NodeIndex;

use crate::AnnotatedExpression;
use crate::ProgramAnnotations;
use crate::TupleSpan;
use crate::lang::Expr;
use crate::lang::Expression;

pub fn find_interactions(
    cfg: Graph<AnnotatedExpression, (), Directed>,
) -> anyhow::Result<ProgramAnnotations> {
    let mut in_sets: HashMap<NodeIndex, HashSet<AnnotatedExpression>> = HashMap::new();
    let mut out_sets: HashMap<NodeIndex, HashSet<AnnotatedExpression>> = HashMap::new();

    for n in cfg.node_indices() {
        in_sets.insert(n, HashSet::new());
        out_sets.insert(n, HashSet::new());
    }

    let mut changed = true;
    while changed {
        changed = false;
        for n in cfg.node_indices() {
            // In set : The intersection of predecessors
            let preds: Vec<_> = cfg.neighbors_directed(n, petgraph::Incoming).collect();

            let new_in = if preds.is_empty() {
                HashSet::new()
            } else {
                preds
                    .iter()
                    .map(|p| out_sets[p].clone())
                    .reduce(|a, b| a.intersection(&b).cloned().collect())
                    .unwrap()
            };

            println!("{:?}", &cfg[n].expr);
            println!("{:?}: New in: {:?}", n, &new_in);

            let expr = &cfg[n];

            // Gen set: A set with only the node's expression itself if it can be memoized
            let mut generated = HashSet::new();
            if is_candidate(&expr.expr) {
                generated.insert(expr.clone());
            }

            println!("{:?}: Generated: {:?}", n, &generated);

            let in_gen_union: HashSet<_> = new_in.union(&generated).cloned().collect();

            // Kill set : Expressions with at least one rewritten variable
            let mut killed = HashSet::new();
            for e in &in_gen_union {
                for v in &e.depends_on {
                    if expr.mutates.contains(&v) {
                        killed.insert(e.clone());
                    }
                }
            }

            println!("{:?}: Killed: {:?}", n, &killed);

            let new_out = in_gen_union.difference(&killed).cloned().collect();

            println!("{:?}: New out: {:?}", n, &new_out);

            if new_out != out_sets[&n] {
                changed = true;
                in_sets.insert(n, new_in);
                out_sets.insert(n, new_out);
            }
        }
    }

    // Collect redundancies
    let mut expr_occurrences: HashMap<Expr, Vec<TupleSpan>> = HashMap::new();
    let mut available_at: HashMap<NodeIndex, HashSet<Expr>> = HashMap::new();

    for n in cfg.node_indices() {
        let node = &cfg[n];

        // available_at
        let exprs: HashSet<_> = in_sets[&n].iter().map(|ae| ae.expr.clone()).collect();
        available_at.insert(n, exprs.clone());

        if exprs.contains(&node.expr) {
            expr_occurrences
                .entry(node.expr.clone())
                .or_default()
                .push((node.expr.start, node.expr.end));
        }
    }

    // Construct ProgramAnnotations directly
    Ok(ProgramAnnotations {
        expr_occurrences,
        available_at,
    })
}

fn is_candidate(expr: &Expr) -> bool {
    matches!(
        expr.expr,
        Expression::BinOp { .. } | Expression::UnOp { .. } | Expression::Call { .. }
    )
}
