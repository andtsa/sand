use petgraph::graph::Graph;
use petgraph::Directed;
use untitled::{AnnotatedExpression};
use untitled::interactions::find_interactions;
use untitled::lang::{Expr, Expression, Bop};

fn a_plus_b(start: (usize, usize)) -> Expr {
    Expr {
        expr: Expression::BinOp {
            left: Box::new(Expr {
                expr: Expression::Var("a".to_string()),
                start,
                end: start,
            }),
            op: Bop::Plus,
            right: Box::new(Expr {
                expr: Expression::Var("b".to_string()),
                start,
                end: start,
            }),
        },
        start,
        end: start,
    }
}

/// t1 = a+b
/// a = 1      // kills a -> (a+b)
/// t2 = a+b
#[test]
fn test_simple() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b((1, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["t1".into()],
    };

    // Node 2: compute a = 1
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Int(1),
            start: (2, 1),
            end: (2, 1),
        },
        depends_on: vec![],
        mutates: vec!["a".into()],
    };

    // Node 3: compute a + b
    let n3_expr = AnnotatedExpression {
        expr: a_plus_b((3, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["t2".into()],
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();

    let n1 = cfg.add_node(n1_expr);
    let n2 = cfg.add_node(n2_expr);
    let n3 = cfg.add_node(n3_expr);

    cfg.add_edge(n1, n2, ());
    cfg.add_edge(n2, n3, ());

    let annotations = find_interactions(cfg).unwrap();

    // Debug output
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert!(annotations.expr_occurrences.is_empty(), "Found available expressions when they shouldn't be any.");
}

/// t1 = a+b
/// t2 = a+b // (a+b) exists
#[test]
fn test_simple_2() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b((1, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["t1".into()],
    };

    // Node 2: compute a + b
    let n2_expr = AnnotatedExpression {
        expr: a_plus_b((2, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["t2".into()],
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();

    let n1 = cfg.add_node(n1_expr.clone());
    let n2 = cfg.add_node(n2_expr);

    cfg.add_edge(n1, n2, ());

    let annotations = find_interactions(cfg).unwrap();

    // Debug output
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(annotations.expr_occurrences.len(), 1, "Found more or less expressions than there should be.");
    assert_eq!(annotations.expr_occurrences.get(&n1_expr.expr), Some(&vec![((2,1), (2,1))]), "Available Expression found at wrong position.");

}

/// a = a+b  // Kills a -> (a+b)
#[test]
fn test_self_generation() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b((1, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["a".into()],
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();

    let n1 = cfg.add_node(n1_expr);

    let annotations = find_interactions(cfg).unwrap();

    // Debug output
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert!(annotations.expr_occurrences.is_empty(), "Found available expressions when they shouldn't be any.");
}

/// t1 = a + b
/// if (cond) {
///    x = 1          // kills x -> (none)
///    t2 = a + b
/// } else {
///   a = 10         // kills -> (a+b)
/// }
/// t3 = a + b
#[test]
fn test_if() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b((1, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["t1".into()],
    };

    // Node 2: branch condition (no-op)
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Var("cond".into()),
            start: (2, 1),
            end: (2, 4),
        },
        depends_on: vec!["cond".into()],
        mutates: vec![],
    };

    // Node 3 (then): x = 1
    let n3_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Int(1),
            start: (3, 1),
            end: (3, 1),
        },
        depends_on: vec![],
        mutates: vec!["x".into()],
    };

    // Node 4 (then): a + b
    let n4_expr = AnnotatedExpression {
        expr: a_plus_b((4, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["t2".into()],
    };

    // Node 5 (else): a = 10
    let n5_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Int(10),
            start: (5, 1),
            end: (5, 2),
        },
        depends_on: vec![],
        mutates: vec!["a".into()], // kills (a+b)
    };

    // Node 6: join point, compute a + b again
    let n6_expr = AnnotatedExpression {
        expr: a_plus_b((6, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["t3".into()],
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();

    let n1 = cfg.add_node(n1_expr.clone());
    let n2 = cfg.add_node(n2_expr);
    let n3 = cfg.add_node(n3_expr);
    let n4 = cfg.add_node(n4_expr);
    let n5 = cfg.add_node(n5_expr);
    let n6 = cfg.add_node(n6_expr);

    cfg.add_edge(n1, n2, ());
    cfg.add_edge(n2, n3, ());
    cfg.add_edge(n2, n5, ());
    cfg.add_edge(n3, n4, ());
    cfg.add_edge(n4, n6, ());
    cfg.add_edge(n5, n6, ());

    let annotations = find_interactions(cfg).unwrap();

    // Debug output
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(annotations.expr_occurrences.len(), 1, "Found more or less expressions than what there should be.");
    assert_eq!(annotations.expr_occurrences.get(&n1_expr.expr), Some(&vec![((4,1), (4,1))]), "Available Expression found at wrong position.");
}

/// a = 1
/// while (cond) {
///    t = a + b     // computes (a + b)
///    a = a + 1     // kills a -> (a + b)
/// }
/// t2 = a + b
#[test]
fn test_while() {
    use petgraph::graph::Graph;
    use petgraph::Directed;
    use untitled::AnnotatedExpression;
    use untitled::interactions::find_interactions;
    use untitled::lang::{Expr, Expression, Bop};

    // Node 1: a = 1
    let n1_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Int(1),
            start: (1, 1),
            end: (1, 1),
        },
        depends_on: vec![],
        mutates: vec!["a".into()],
    };

    // Node 2: loop condition (cond)
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Var("cond".into()),
            start: (2, 1),
            end: (2, 4),
        },
        depends_on: vec!["cond".into()],
        mutates: vec![],
    };

    // Node 3: t = a + b
    let n3_expr = AnnotatedExpression {
        expr: a_plus_b((3, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["t".into()],
    };

    // Node 4: a = a + 1   (kills a)
    let n4_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::BinOp {
                left: Box::new(Expr {
                    expr: Expression::Var("a".into()),
                    start: (4, 1),
                    end: (4, 1),
                }),
                op: Bop::Plus,
                right: Box::new(Expr {
                    expr: Expression::Int(1),
                    start: (4, 5),
                    end: (4, 5),
                }),
            },
            start: (4, 1),
            end: (4, 5),
        },
        depends_on: vec!["a".into()],
        mutates: vec!["a".into()],
    };

    // Node 5: after loop: t2 = a + b
    let n5_expr = AnnotatedExpression {
        expr: a_plus_b((6, 1)),
        depends_on: vec!["a".into(), "b".into()],
        mutates: vec!["t2".into()],
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();

    let n1 = cfg.add_node(n1_expr);
    let n2 = cfg.add_node(n2_expr);
    let n3 = cfg.add_node(n3_expr);
    let n4 = cfg.add_node(n4_expr);
    let n5 = cfg.add_node(n5_expr);

    cfg.add_edge(n1, n2, ());
    cfg.add_edge(n2, n3, ());
    cfg.add_edge(n3, n4, ());
    cfg.add_edge(n4, n2, ()); // back edge
    cfg.add_edge(n2, n5, ());

    let annotations = find_interactions(cfg).unwrap();

    // Debug output
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

   assert!(annotations.expr_occurrences.is_empty(), "Found available expressions when there shouldn't be any.")
}