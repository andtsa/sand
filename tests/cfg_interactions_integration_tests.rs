use std::collections::HashSet;

use sand::analysis::cfg::construct_cfg;
use sand::analysis::interactions::find_interactions;
use sand::ir_types::hhir::Expr;
use sand::ir_types::hhir::Expression;
use sand::ir_types::hhir::ProgramModule;
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
                range: Default::default(),
            }),
            op: Bop::Plus,
            right: Box::new(Expr {
                expr: Expression::Int(2),
                range: Default::default(),
            }),
        },
        range: Default::default(),
    };

    let program = ProgramModule::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(
        annotations.expr_occurrences.len(),
        1,
        "More or less available expression founds."
    );
    assert!(
        annotations.expr_occurrences.contains_key(&expr),
        "Different available expression found than target."
    );
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

    let program = ProgramModule::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(
        annotations.expr_occurrences.len(),
        0,
        "Found available expressions where there shouldn't be any."
    );
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

    let expr = a_plus_b(Range::new(0, 0, 0, 0));

    let program = ProgramModule::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    println!("{:?}", cfg);

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert!(
        annotations.expr_occurrences.contains_key(&expr),
        "Different available expression found."
    );
    assert_eq!(
        annotations.expr_occurrences.get(&expr),
        Some(&HashSet::from([Range::new(6, 22, 6, 25)])),
        "Available expression identified at wrong position."
    );
}

#[test]
fn test_while() {
    let src = r#"
        def main(): Int := {
            while cond do {
                t2 = a+b;
            };
            a+b
        }
    "#;

    let program = ProgramModule::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    println!("{:?}", cfg);

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(
        annotations.expr_occurrences.len(),
        0,
        "Different available expression found."
    );
}

#[test]
fn test_params() {
    let src = r#"
        def main(): Int := {
            fn(a+b);
            fn(a+b);
        }
    "#;

    let expr1 = a_plus_b(Range::new(0, 0, 0, 0));
    let expr2 = Expr {
        expr: Expression::Call {
            fn_name: "fn".to_string(),
            args: vec![a_plus_b(Range::new(1, 3, 1, 3))],
        },

        range: Default::default(),
    };

    let program = ProgramModule::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    println!("{:?}", cfg);

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(
        annotations.expr_occurrences.len(),
        2,
        "Different number of available expression found."
    );
    assert!(
        annotations.expr_occurrences.contains_key(&expr1),
        "Available expression missing."
    );
    assert!(
        annotations.expr_occurrences.contains_key(&expr2),
        "Available expression missing."
    );
    assert_eq!(
        annotations.expr_occurrences.get(&expr1).unwrap().len(),
        2,
        "Wrong number of occurrences."
    );
    assert_eq!(
        annotations.expr_occurrences.get(&expr2).unwrap().len(),
        1,
        "Wrong number of occurrences."
    );
}

#[test]
fn test_function_simple() {
    let src = r#"
        def fn(a: Int): Int := {
            let c: Int = a+1;
            c
        }

        def main(): Int := {
            let a: Int = 1;
            a = a+1;
            let b:Int = fn(a);
            a
        }
    "#;

    let program = ProgramModule::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();

    println!("{:?}", cfg);

    // Debug output
    let annotations = find_interactions(cfg).unwrap();

    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert_eq!(
        annotations.expr_occurrences.len(),
        0,
        "Found available expressions where there shouldn't be any."
    );
}

#[test]
fn test_function_intersection() {
    let src = r#"
    def foo():Int := {
        let x:Int = a + b;
    }

    def main():Int := {
        foo();
        let y:Int = a + b;
    }
    "#;

    let expr = a_plus_b(Range::new(0, 0, 0, 0));

    let program = ProgramModule::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();
    let annotations = find_interactions(cfg).unwrap();

    // Debug Output:
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert!(
        annotations.expr_occurrences.contains_key(&expr),
        "(a + b) should be available after main's call to foo."
    );
}

#[test]
fn test_block() {
    let src = r#"
    def main(): Int := {
        let e: Int = {
            let y:Int = a;
            let x:Int = a+b;
            x
        };
        let b:Int = a+b;
        b
    }
    "#;

    let expr = a_plus_b(Range::new(0, 0, 0, 0));

    let program = ProgramModule::parse(src).unwrap();
    let cfg = construct_cfg(&program).unwrap();
    let annotations = find_interactions(cfg).unwrap();

    // Debug Output:
    println!("\nAvailable expressions:");
    for (e, s) in &annotations.expr_occurrences {
        println!("{:?} -> {:?}", e, s);
    }

    assert!(
        annotations.expr_occurrences.contains_key(&expr),
        "(a + b) should be available after the block."
    );
}
