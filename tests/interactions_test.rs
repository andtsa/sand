use std::collections::HashSet;

use petgraph::Directed;
use petgraph::graph::Graph;
use sand::AnnotatedExpression;
use sand::analysis::interactions::find_interactions;
use sand::ir_types::hhir::Expr;
use sand::ir_types::hhir::Expression;
use sand::lang::ops::Bop;
use sand::lang::structure::Range;

fn a_plus_b(range: Range) -> Expr {
    Expr {
        expr: Expression::BinOp {
            left: Box::new(Expr {
                expr: Expression::Var("a".to_string()),
                range,
            }),
            op: Bop::Plus,
            right: Box::new(Expr {
                expr: Expression::Var("b".to_string()),
                range,
            }),
        },
        range,
    }
}

/// t1 = a+b
/// a = 1      // kills a -> (a+b)
/// t2 = a+b
#[test]
fn test_simple() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::default()),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t1".into()]),
    };

    // Node 2: compute a = 1
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Int(1),
            range: Range::default(),
        },
        depends_on: HashSet::from([]),
        mutates: HashSet::from(["a".into()]),
    };

    // Node 3: compute a + b
    let n3_expr = AnnotatedExpression {
        expr: a_plus_b(Range::default()),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t2".into()]),
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

    assert!(
        annotations.expr_occurrences.is_empty(),
        "Found available expressions when they shouldn't be any."
    );
}

/// t1 = a+b
/// t2 = a+b // (a+b) exists
#[test]
fn test_simple_2() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(1, 1, 1, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t1".into()]),
    };

    // Node 2: compute a + b
    let n2_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(2, 1, 2, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t2".into()]),
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

    assert_eq!(
        annotations.expr_occurrences.len(),
        1,
        "Found more or less expressions than there should be."
    );
    assert_eq!(
        annotations.expr_occurrences.get(&n1_expr.expr),
        Some(&HashSet::from([Range::new(2, 1, 2, 1)])),
        "Available Expression found at wrong position."
    );
}

/// a = a+b  // Kills a -> (a+b)
#[test]
fn test_self_generation() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(2, 1, 2, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["a".into()]),
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();

    cfg.add_node(n1_expr);

    let annotations = find_interactions(cfg).unwrap();

    // Debug output
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert!(
        annotations.expr_occurrences.is_empty(),
        "Found available expressions when they shouldn't be any."
    );
}

/// t1 = a + b
/// if (cond) {
///    x = 1          // kills x -> (none)
///    t2 = a + b    // (a+b) IS available
/// } else {
///   a = 10         // kills -> (a+b)
/// }
/// t3 = a + b
#[test]
fn test_if() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(1, 1, 1, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t1".into()]),
    };

    // Node 2: branch condition (no-op)
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Var("cond".into()),
            range: Range::new(2, 1, 2, 4),
        },
        depends_on: HashSet::from(["cond".into()]),
        mutates: HashSet::from([]),
    };

    // Node 3 (then): x = 1
    let n3_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Int(1),
            range: Range::new(3, 1, 3, 1),
        },
        depends_on: HashSet::from([]),
        mutates: HashSet::from(["x".into()]),
    };

    // Node 4 (then): a + b
    let n4_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(4, 1, 4, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t2".into()]),
    };

    // Node 5 (else): a = 10
    let n5_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Int(10),
            range: Range::new(5, 1, 5, 2),
        },
        depends_on: HashSet::from([]),
        mutates: HashSet::from(["a".into()]), // kills (a+b)
    };

    // Node 6: join point, compute a + b again
    let n6_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(6, 1, 6, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t3".into()]),
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

    assert_eq!(
        annotations.expr_occurrences.len(),
        1,
        "Found more or less expressions than what there should be."
    );
    assert_eq!(
        annotations.expr_occurrences.get(&n1_expr.expr),
        Some(&HashSet::from([Range::new(4, 1, 4, 1)])),
        "Available Expression found at wrong position."
    );
}

/// a = 1
/// while (cond) {
///    t = a + b     // computes (a + b)
/// }
/// t2 = a + b      // (a+b) NOT available bc cond could be false
#[test]
fn test_while1() {
    // Node 1: a = 1
    let n1_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Int(1),
            range: Range::new(1, 1, 1, 1),
        },
        depends_on: HashSet::from([]),
        mutates: HashSet::from(["a".into()]),
    };

    // Node 2: loop condition (cond)
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Var("cond".into()),
            range: Range::new(2, 1, 2, 4),
        },
        depends_on: HashSet::from(["cond".into()]),
        mutates: HashSet::from([]),
    };

    // Node 3: t = a + b
    let n3_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(3, 1, 3, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t".into()]),
    };

    // Node 4: after loop: t2 = a + b
    let n4_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(4, 1, 4, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t2".into()]),
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();

    let n1 = cfg.add_node(n1_expr);
    let n2 = cfg.add_node(n2_expr);
    let n3 = cfg.add_node(n3_expr);
    let n4 = cfg.add_node(n4_expr);

    cfg.add_edge(n1, n2, ());
    cfg.add_edge(n2, n3, ());
    cfg.add_edge(n3, n2, ()); // back edge
    cfg.add_edge(n2, n4, ());

    let annotations = find_interactions(cfg).unwrap();

    // Debug output
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert!(
        annotations.expr_occurrences.is_empty(),
        "Found available expressions when there shouldn't be any."
    )
}

/// a = 1
/// while (cond) {
///    t = a + b     // computes (a + b)
///    a = a + 1     // kills a -> (a + b)
/// }
/// t2 = a + b
#[test]
fn test_while2() {
    // Node 1: a = 1
    let n1_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Int(1),
            range: Range::new(1, 1, 1, 1),
        },
        depends_on: HashSet::from([]),
        mutates: HashSet::from(["a".into()]),
    };

    // Node 2: loop condition (cond)
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Var("cond".into()),
            range: Range::new(2, 1, 2, 4),
        },
        depends_on: HashSet::from(["cond".into()]),
        mutates: HashSet::from([]),
    };

    // Node 3: t = a + b
    let n3_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(3, 1, 3, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t".into()]),
    };

    // Node 4: a = a + 1   (kills a)
    let n4_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::BinOp {
                left: Box::new(Expr {
                    expr: Expression::Var("a".into()),
                    range: Range::new(4, 1, 4, 1),
                }),
                op: Bop::Plus,
                right: Box::new(Expr {
                    expr: Expression::Int(1),
                    range: Range::new(4, 5, 4, 5),
                }),
            },
            range: Range::new(4, 1, 4, 5),
        },
        depends_on: HashSet::from(["a".into()]),
        mutates: HashSet::from(["a".into()]),
    };

    // Node 5: after loop: t2 = a + b
    let n5_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(6, 1, 6, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t2".into()]),
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

    assert!(
        annotations.expr_occurrences.is_empty(),
        "Found available expressions when there shouldn't be any."
    )
}

/// t1 = a+b
/// t2 = c+(a+b)+d // (a+b) inside is available
/// t3 = a+b      // (a+b) is available
#[test]
fn test_subexpressions() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(1, 1, 1, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t1".into()]),
    };

    // Node 2: compute c+(a+b)+d
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            range: Range::new(2, 1, 2, 1),
            expr: Expression::BinOp {
                left: Box::new(Expr {
                    range: Range::new(2, 1, 2, 1),
                    expr: Expression::BinOp {
                        left: Box::new(Expr {
                            range: Range::new(2, 1, 2, 1),
                            expr: Expression::Var("c".to_string()),
                        }),
                        op: Bop::Plus,
                        right: Box::new(Expr {
                            range: Range::new(2, 1, 2, 1),
                            expr: Expression::BinOp {
                                left: Box::new(Expr {
                                    range: Range::new(2, 1, 2, 1),
                                    expr: Expression::Var("a".to_string()),
                                }),
                                op: Bop::Plus,
                                right: Box::new(Expr {
                                    range: Range::new(2, 1, 2, 1),
                                    expr: Expression::Var("b".to_string()),
                                }),
                            },
                        }),
                    },
                }),
                op: Bop::Plus,
                right: Box::new(Expr {
                    range: Range::new(2, 1, 2, 1),
                    expr: Expression::Var("d".to_string()),
                }),
            },
        },
        depends_on: HashSet::from(["a".into(), "b".into(), "c".into(), "d".into()]),
        mutates: HashSet::from(["t2".into()]),
    };

    let n3_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(3, 1, 3, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t3".into()]),
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();

    let n1 = cfg.add_node(n1_expr.clone());
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

    assert_eq!(
        annotations.expr_occurrences.len(),
        1,
        "Found more or less expressions than there should be."
    );
    assert_eq!(
        annotations.expr_occurrences.get(&n1_expr.expr),
        Some(&HashSet::from([
            Range::new(2, 1, 2, 1),
            Range::new(3, 1, 3, 1)
        ])),
        "Available Expression found at wrong position."
    );
}

/// t2 = a+b+c;   // Generates (a+b+c) NOT (a+b),(b+c),(a+c)
/// t3 = a+b     // (a+b) is NOT available
#[test]
fn test_not_subexpressions() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(1, 1, 1, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t1".into()]),
    };

    // Node 2: compute c+(a+b)+d
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            range: Range::new(2, 1, 2, 1),
            expr: Expression::BinOp {
                left: Box::new(Expr {
                    range: Range::new(2, 1, 2, 1),
                    expr: Expression::BinOp {
                        left: Box::new(Expr {
                            range: Range::new(2, 1, 2, 1),
                            expr: Expression::Var("c".to_string()),
                        }),
                        op: Bop::Plus,
                        right: Box::new(Expr {
                            range: Range::new(2, 1, 2, 1),
                            expr: Expression::BinOp {
                                left: Box::new(Expr {
                                    range: Range::new(2, 1, 2, 1),
                                    expr: Expression::Var("a".to_string()),
                                }),
                                op: Bop::Plus,
                                right: Box::new(Expr {
                                    range: Range::new(2, 1, 2, 1),
                                    expr: Expression::Var("b".to_string()),
                                }),
                            },
                        }),
                    },
                }),
                op: Bop::Plus,
                right: Box::new(Expr {
                    range: Range::new(2, 1, 2, 1),
                    expr: Expression::Var("d".to_string()),
                }),
            },
        },
        depends_on: HashSet::from(["a".into(), "b".into(), "c".into(), "d".into()]),
        mutates: HashSet::from(["t2".into()]),
    };

    let n3_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(3, 1, 3, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t3".into()]),
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();

    let n1 = cfg.add_node(n1_expr.clone());
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

    assert_eq!(
        annotations.expr_occurrences.len(),
        1,
        "Found more or less expressions than there should be."
    );
    assert_eq!(
        annotations.expr_occurrences.get(&n1_expr.expr),
        Some(&HashSet::from([
            Range::new(2, 1, 2, 1),
            Range::new(3, 1, 3, 1)
        ])),
        "Available Expression found at wrong position."
    );
}

/// t1 = a + b
/// fn(a + b)   // (a + b) IS available
#[test]
fn test_parameters1() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(1, 1, 1, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t1".into()]),
    };

    // Node 2: call fn(a + b)
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Call {
                fn_name: "fn".to_string(),
                args: vec![a_plus_b(Range::new(2, 1, 2, 1))],
            },
            range: Range::new(2, 1, 2, 1),
        },
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::new(), // function call itself doesn't assign here
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

    assert_eq!(
        annotations.expr_occurrences.len(),
        1,
        "Found more or less expressions than there should be."
    );

    assert_eq!(
        annotations.expr_occurrences.get(&n1_expr.expr),
        Some(&HashSet::from([Range::new(2, 1, 2, 1)])),
        "Available expression not detected in function argument."
    );
}

/// t1 = a + b
/// fn(a + b, c)   // (a + b) IS available
#[test]
fn test_parameters2() {
    // Node 1: compute a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(1, 1, 1, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t1".into()]),
    };

    // Node 2: call fn(a + b, c)
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            expr: Expression::Call {
                fn_name: "fn".to_string(),
                args: vec![
                    a_plus_b(Range::new(2, 1, 2, 1)),
                    Expr {
                        expr: Expression::Var("c".to_string()),
                        range: Range::new(2, 1, 2, 1),
                    },
                ],
            },
            range: Range::new(2, 1, 2, 1),
        },
        depends_on: HashSet::from(["a".into(), "b".into(), "c".into()]),
        mutates: HashSet::new(),
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

    // Exactly one available expression: a + b
    assert_eq!(
        annotations.expr_occurrences.len(),
        1,
        "Found more or less expressions than there should be."
    );

    // It should be detected at the function call site
    assert_eq!(
        annotations.expr_occurrences.get(&n1_expr.expr),
        Some(&HashSet::from([Range::new(2, 1, 2, 1)])),
        "Available expression not detected in multi-argument function call."
    );
}

/// t1 = fn(a, b)
/// t2 = fn(a, b)   // fn(a, b) IS available
#[test]
fn test_function_call() {
    // Node 1: compute fn(a, b)
    let n1_expr = AnnotatedExpression {
        expr: Expr {
            range: Range::new(1, 1, 1, 1),
            expr: Expression::Call {
                fn_name: "fn".to_string(),
                args: vec![
                    Expr {
                        range: Range::new(1, 1, 1, 1),
                        expr: Expression::Var("a".to_string()),
                    },
                    Expr {
                        range: Range::new(1, 1, 1, 1),
                        expr: Expression::Var("b".to_string()),
                    },
                ],
            },
        },
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t1".into()]),
    };

    // Node 2: compute fn(a, b) again
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            range: Range::new(2, 1, 2, 1),
            expr: Expression::Call {
                fn_name: "fn".to_string(),
                args: vec![
                    Expr {
                        range: Range::new(2, 1, 2, 1),
                        expr: Expression::Var("a".to_string()),
                    },
                    Expr {
                        range: Range::new(2, 1, 2, 1),
                        expr: Expression::Var("b".to_string()),
                    },
                ],
            },
        },
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t2".into()]),
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

    assert_eq!(
        annotations.expr_occurrences.len(),
        1,
        "Found more or less expressions than there should be."
    );

    assert_eq!(
        annotations.expr_occurrences.get(&n1_expr.expr),
        Some(&HashSet::from([Range::new(2, 1, 2, 1)])),
        "Entire function call was not detected as available."
    );
}

/// x = a+b
/// t1 = fn(a+b)   // (a+b) is available
/// t2 = fn(a+b)   // (a+b) AND fn(a+b) are available
#[test]
fn test_subexpression_and_function_call() {
    // Node 1: x = a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(1, 1, 1, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["x".into()]),
    };

    // Node 2: t1 = fn(a + b)
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            range: Range::new(2, 1, 2, 1),
            expr: Expression::Call {
                fn_name: "fn".to_string(),
                args: vec![a_plus_b(Range::new(2, 1, 2, 1))],
            },
        },
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t1".into()]),
    };

    // Node 3: t2 = fn(a + b)
    let n3_expr = AnnotatedExpression {
        expr: Expr {
            range: Range::new(3, 1, 3, 1),
            expr: Expression::Call {
                fn_name: "fn".to_string(),
                args: vec![a_plus_b(Range::new(3, 1, 3, 1))],
            },
        },
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["t2".into()]),
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();
    let n1 = cfg.add_node(n1_expr.clone());
    let n2 = cfg.add_node(n2_expr.clone());
    let n3 = cfg.add_node(n3_expr.clone());

    cfg.add_edge(n1, n2, ());
    cfg.add_edge(n2, n3, ());

    let annotations = find_interactions(cfg).unwrap();

    // Debug output
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert!(
        annotations.available_at[&n2].contains(&n1_expr.expr),
        "Available expression missing"
    );
    assert!(
        annotations.available_at[&n3].contains(&n2_expr.expr),
        "Available expression missing"
    );
}

///        x = a+b
///      /        \
///   y=x+3 --->  a+b
///  At the end: (a+b) is available but x+3 isn't
#[test]
fn test_branching() {
    // Node 1: x = a + b
    let n1_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(1, 1, 1, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::from(["x".into()]),
    };

    // Node 2: y = x + 3
    let n2_expr = AnnotatedExpression {
        expr: Expr {
            range: Range::new(2, 1, 2, 1),
            expr: Expression::BinOp {
                left: Box::new(Expr {
                    range: Range::new(2, 1, 2, 1),
                    expr: Expression::Var("x".to_string()),
                }),
                op: Bop::Plus,
                right: Box::new(Expr {
                    range: Range::new(2, 1, 2, 1),
                    expr: Expression::Int(3),
                }),
            },
        },
        depends_on: HashSet::from(["x".into()]),
        mutates: HashSet::from(["y".into()]),
    };

    // Node 3: a + b
    let n3_expr = AnnotatedExpression {
        expr: a_plus_b(Range::new(3, 1, 3, 1)),
        depends_on: HashSet::from(["a".into(), "b".into()]),
        mutates: HashSet::new(),
    };

    // Build CFG
    let mut cfg: Graph<AnnotatedExpression, (), Directed> = Graph::new();
    let n1 = cfg.add_node(n1_expr.clone());
    let n2 = cfg.add_node(n2_expr);
    let n3 = cfg.add_node(n3_expr.clone());

    // Edges: n1 -> n2 and n1 -> n3, then n2 -> n3 to create join
    cfg.add_edge(n1, n2, ());
    cfg.add_edge(n1, n3, ());
    cfg.add_edge(n2, n3, ());

    let annotations = find_interactions(cfg.clone()).unwrap();

    // Debug output
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    // n3 should only have 'a+b' from the n1->n3 branch if intersection works
    assert!(
        annotations.available_at[&n3].contains(&n1_expr.expr),
        "(a+b) is not available at join."
    );

    let n2_expr_struct = &cfg[n2].expr;
    assert!(
        !annotations.available_at[&n3].contains(n2_expr_struct),
        "Expression that wasn't reachable from all paths reached the join."
    );
}
