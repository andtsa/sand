use untitled::cfg::construct_cfg;
use untitled::interactions::find_interactions;
use untitled::lang::Bop;
use untitled::lang::Expr;
use untitled::lang::Expression;
use untitled::lang::Program;

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

    let program = Program::parse(src).unwrap();
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
