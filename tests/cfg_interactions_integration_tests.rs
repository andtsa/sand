use std::collections::HashSet;
use untitled::cfg::construct_cfg;
use untitled::interactions::find_interactions;
use untitled::lang::{Bop, Expr, Expression};
use untitled::lang::Program;

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

#[test]
fn test_simple() {
    let src = r#"
        def main(): Int := {
            let x: Int = 1 + 2;
            let y: Int = 1 + 2;
            x
        }
    "#;

    let expr = Expr {
        expr: Expression::BinOp {
            left: Box::new(Expr {
                expr: Expression::Int(1),
                start: (0, 0),
                end: (0, 0),
            }),
            op: Bop::Plus,
            right: Box::new(Expr {
                expr: Expression::Int(2),
                start: (0, 0),
                end: (0, 0),
            }),
        },
        start: (0, 0),
        end: (0, 0),
    };

    let program = Program::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(annotations.expr_occurrences.len(), 1, "More or less available expression founds.");
    assert!(annotations.expr_occurrences.contains_key(&expr), "Different available expression found than target.");
}

#[test]
fn test_simple_kill() {
    let src = r#"
        def main(): Int := {
            let x: Int = a+b;
            a = a + 1;
            let y: Int = a+b;
            x
        }
    "#;

    let program = Program::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(annotations.expr_occurrences.len(), 0, "Found available expressions where there shouldn't be any.");
}

#[test]
fn test_if() {
    let src = r#"
        def main(): Int := {
            t1 = a+b;
            if cond then {
                x = 1;
                t2 = a+b;
            } else {
                a = 10;
            };
            a+b
        }
    "#;

    let expr = a_plus_b((0,0));

    let program = Program::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    println!("{:?}", cfg);

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert!(annotations.expr_occurrences.contains_key(&expr), "Different available expression found.");
    assert_eq!(
        annotations.expr_occurrences.get(&expr),
        Some(&HashSet::from([((6, 22), (6, 25))])),
        "Available expression identified at wrong position."
    );
}

#[test]
fn test_while1() {
    let src = r#"
        def main(): Int := {
            while cond do {
                t2 = a+b;
            };
            a+b
        }
    "#;

    let program = Program::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    println!("{:?}", cfg);

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(annotations.expr_occurrences.len(), 0, "Different available expression found.");
}

#[test]
fn test_while2() {
    let src = r#"
        def main(): Int := {
            fn(a+b);
            fn(a+b);
        }
    "#;

    let expr1 = a_plus_b((0,0));
    let expr2 = Expr {
        expr: Expression::Call {
            fn_name: "fn".to_string(),
            args: vec![a_plus_b((3, 1))],
        },
        start: (0, 0),
        end: (0, 0),
    };

    let program = Program::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    println!("{:?}", cfg);

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(annotations.expr_occurrences.len(), 2, "Different number of available expression found.");
    assert!(annotations.expr_occurrences.contains_key(&expr1));
    assert!(annotations.expr_occurrences.contains_key(&expr2));
    assert_eq!(annotations.expr_occurrences.get(&expr1).unwrap().len(), 2);
    assert_eq!(annotations.expr_occurrences.get(&expr2).unwrap().len(), 1);
}