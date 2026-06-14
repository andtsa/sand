#![allow(unused)]
//! find repeated expressions in a program,
//! keeping track of variable interactions

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

use petgraph::Directed;
use petgraph::Graph;
use petgraph::graph::NodeIndex;

use crate::analysis::AnnotatedExpression;
use crate::analysis::ProgramAnnotations;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::Expression;
use crate::lang::intrinsics::RESERVED_FUNCTION_NAMES;

// `Expr`/`Ty` keys reach a `Cell<Option<Ty>>` (an enum variant's payload)
// through arena references, so clippy flags them as interior-mutable map keys.
// This is sound here: `Expr`/`Ty` hash and compare by structural/pointer
// identity and never read that `Cell`, so mutating a payload cannot change a
// key's hash.
#[allow(clippy::mutable_key_type)]
pub fn find_interactions<'tcx>(
    cfg: Graph<AnnotatedExpression<'tcx>, (), Directed>,
) -> ProgramAnnotations<'tcx> {
    let mut in_sets: HashMap<NodeIndex, HashSet<AnnotatedExpression<'tcx>>> = HashMap::new();
    let mut out_sets: HashMap<NodeIndex, HashSet<AnnotatedExpression<'tcx>>> = HashMap::new();

    for n in cfg.node_indices() {
        in_sets.insert(n, HashSet::new());
        out_sets.insert(n, HashSet::new());
    }

    let mut worklist = VecDeque::new();
    let mut visited = HashSet::new();
    worklist.push_back(NodeIndex::new(0)); // this is unsafe, it depends on the
    // internal implementation for the numbering, i would recommend u avoid.
    // // NOTE: starting from all nodes is inefficient but guarantees convergence
    // for n in cfg.node_indices() {
    //     worklist.push_back(n);
    // }

    while let Some(n) = worklist.pop_front() {
        let first_time = visited.insert(n);

        // In set : The intersection of predecessors
        let preds: Vec<_> = cfg.neighbors_directed(n, petgraph::Incoming).collect();

        let new_in: HashSet<AnnotatedExpression<'tcx>> = if preds.is_empty() {
            HashSet::new()
        } else {
            let mut result = out_sets[&preds[0]].clone();
            for i in 1..preds.len() {
                result = result
                    .intersection(&out_sets[&preds[i]])
                    .cloned()
                    .collect::<HashSet<AnnotatedExpression<'tcx>>>();
            }
            result
        };

        // println!("{:?}", &cfg[n].expr);
        // println!("{:?}: New in: {:?}", n, &new_in);

        let Some(expr) = &cfg.node_weight(n) else {
            continue;
        };

        // Gen set: A set with only the node's expression itself if it can be memoized
        let mut generated: HashSet<AnnotatedExpression<'tcx>> = HashSet::new();
        if is_candidate(&expr.expr) {
            generated.insert((*expr).clone());
        }

        // println!("{:?}: Generated: {:?}", n, &generated);

        let mut in_gen_union = new_in.clone();
        for g in generated {
            in_gen_union.insert(g);
        }

        // Kill set : Expressions with at least one rewritten variable
        let mut killed = HashSet::new();
        for e in &in_gen_union {
            for v in &e.depends_on {
                if expr.mutates.contains(v) {
                    killed.insert(e.clone());
                }
            }
        }

        // println!("{:?}: Killed: {:?}", n, &killed);

        let new_out = in_gen_union.difference(&killed).cloned().collect();

        // println!("{:?}: New out: {:?}", n, &new_out);

        if new_in != in_sets[&n] || new_out != out_sets[&n] || first_time {
            in_sets.insert(n, new_in);
            out_sets.insert(n, new_out);

            for succ in cfg.neighbors_directed(n, petgraph::Outgoing) {
                worklist.push_back(succ);
            }
        }
    }

    // Collect redundancies
    let mut expr_occurrences: HashMap<Expr<'tcx>, HashSet<(ModuleRef, Range)>> = HashMap::new();
    let mut available_at: HashMap<NodeIndex, HashSet<Expr<'tcx>>> = HashMap::new();

    for n in cfg.node_indices() {
        let node = &cfg[n];

        // available_at
        let exprs: HashSet<_> = in_sets[&n]
            .iter()
            // NOTE: including the out-set captures the first instance of an expression as well
            // .chain(out_sets[&n].iter())
            .map(|ae| ae.expr.clone())
            .collect();

        available_at.insert(n, exprs.clone());

        let mut subexprs = Vec::new();
        collect_expr_subtrees(&node.expr, &mut subexprs);
        for sub in subexprs {
            if available_at[&n].contains(sub) {
                expr_occurrences
                    .entry(sub.clone())
                    .or_default()
                    .insert((node.module, sub.range));
            }
        }
    }

    // Construct ProgramAnnotations directly
    ProgramAnnotations {
        expr_occurrences,
        available_at,
    }
}

fn is_candidate(expr: &Expr<'_>) -> bool {
    matches!(
        expr.expr,
        Expression::BinOp { .. } | Expression::UnOp { .. } | Expression::Call { .. }
    )
}

pub fn has_other_side_effects(expr: &Expr<'_>) -> bool {
    false
    // match &expr.expr {
    //     Expression::Call { fn_name, .. } if
    // RESERVED_FUNCTION_NAMES.contains(&fn_name.as_str()) => {         true
    //     }
    //     _ => false        // Expression::If { cond, t, f } =>
    // has_other_side_effects(&cond) }
}

fn collect_expr_subtrees<'a, 'tcx: 'a>(expr: &'a Expr<'tcx>, out: &mut Vec<&'a Expr<'tcx>>) {
    if has_other_side_effects(expr) {
        println!("side effects: {expr:?}, {out:?}");
        out.clear();
        return;
    }
    out.push(expr);

    match &expr.expr {
        Expression::BinOp { left, right, .. } => {
            collect_expr_subtrees(left, out);
            collect_expr_subtrees(right, out);
        }
        Expression::UnOp { right, .. } => {
            collect_expr_subtrees(right, out);
        }
        Expression::Call { args, .. } => {
            for a in args {
                collect_expr_subtrees(a, out);
            }
        }
        _ => {}
    }
}
