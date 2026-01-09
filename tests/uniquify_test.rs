//! Tests for the uniquify pass of the compiler
use untitled::lang::*;
use std::collections::HashSet;

// ----------------------------------------------- Helper ------------------------------------------------------

/// Checks that all variable and function names in the provided program AST are unique.
/// It does so by traversing all blocks and collecting all declared names in a HashSet.
/// # Arguments
/// * 'prog' - The Program AST to check
/// # Returns
/// 'Ok(())' if all names are unique; otherwise, 'Err(name)' for the first duplicate it finds.
fn assert_unique(prog: &Program) -> Result<(), String> {
    let mut seen: HashSet<String> = HashSet::new();

    for func in &prog.0 {
        if !seen.insert(func.name.clone()) {
            return Err(format!("Duplicate function name: {}", func.name));
        }

        check_expr(&func.body, &mut seen)?;
    }

    Ok(())
}

/// Recursively checks an expression AST for uniqueness of all declared identifiers.
/// # Arguments
/// * 'expr' - The expression to traverse.
/// * 'seen' - The set of already encountered names.
/// # Returns
/// 'Ok(())' if all names are unique, otherwise 'Err(name)'.
fn check_expr(expr: &Expr, seen: &mut HashSet<String>) -> Result<(), String> {
    match &expr.expr {
        Expression::If { cond, t, f } => {
            check_expr(cond, seen)?;
            check_expr(t, seen)?;
            check_expr(f, seen)?;
        }

        Expression::While { cond, body } => {
            check_expr(cond, seen)?;
            check_expr(body, seen)?;
        }

        Expression::BinOp { left, right, .. } => {
            check_expr(left, seen)?;
            check_expr(right, seen)?;
        }

        Expression::UnOp { right, .. } => {
            check_expr(right, seen)?;
        }

        Expression::Call { args, .. } => {
            for arg in args {
                check_expr(arg, seen)?;
            }
        }

        Expression::Block { statements, expr: inner_expr } => {
            for stmt in statements {
                check_stmt(stmt, seen)?;
            }
            if let Some(e) = inner_expr {
                check_expr(e, seen)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Recursively checks a statement AST for uniqueness of all declared identifiers.
/// # Arguments
/// * 'stmt' - The statement to traverse.
/// * 'seen' - The set of already encountered names.
/// # Returns
/// 'Ok(())' if all names are unique, otherwise 'Err(name)'.
fn check_stmt(stmt: &Statement, seen: &mut HashSet<String>) -> Result<(), String> {
    match stmt {
        Statement::Declaration { name, val, .. } => {
            if !seen.insert(name.clone()) {
                return Err(format!("Duplicate variable name: {}", name));
            }
            check_expr(val, seen)
        }

        Statement::Assignment { val, .. } => {
            check_expr(val, seen)
        }

        Statement::Expr(e) => check_expr(e, seen),
    }
}

/// Compares the behavior of the original and uniquified programs by interpreting both
/// and expecting that they produce the same result.
/// # Arguments
/// * 'original' - The original program AST.
/// * 'uniquified' - The program AST after being passed through uniquify.
fn assert_sound(original: &Program, uniquified: &Program) {
    let value1 = original.interpret().unwrap();
    let value2 = uniquified.interpret().unwrap();
    assert_eq!(value1, value2);
}

// ----------------------------------------------- Helper ------------------------------------------------------

/// Soundness test on a very simple program.
#[test]
fn soundness1() {
    let src = r#"
    def main(): Int := {
        let x: Int = 5;
        x
    }
    "#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();

    assert_sound(&original, &uniquified);
}

/// Soundness testing with multiple functions and shadowing.
#[test]
fn soundness2() {
    let src = r#"
    def fn(): Int := {
        let x: Int = 2;
        x
    }

    def main(): Int := {
        let x: Int = 5;
        x + fn()
    }
    "#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();

    assert_sound(&original, &uniquified);
}

/// Checks uniqueness in a very simple program.
#[test]
fn correctness1() {
    let src = r#"
    def main(): Int := {
        let x: Int = 5;
        x
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
}

/// Checks uniqueness in a very simple program.
#[test]
fn correctness2() {
    let src = r#"
    def main(): Int := {
        let x: Int = 5;
        let y: Int = 2;
        let z: Int = x + y;
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
}

/// Checks uniqueness in a program with a block.
#[test]
fn correctness3() {
    let src = r#"
    def main(): Int := {
        let a: Int = 1;
        let x: Int = {
            a = a + 1;
            let a: Int = 5;
            a = a + a;
            a
        };
        a = 3;
        x
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
}

/// Checks uniqueness in a program with multiple nested blocks.
#[test]
fn correctness4() {
    let src = r#"
    def main(): Int := {
        let a: Int = 1;
        let x: Int = {
            let x: Int = {
                let x: Int = {
                    let x: Int = 1;
                    x
                };
            };
        };
        x
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
}

/// Checks uniqueness in a program with an if statement.
#[test]
fn correctness5() {
    let src = r#"
    def main(a: Int, b: Int): Int := {
        let a: Int = 4;
        if a < 2 then {
            a
        } else {
            2
        };
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
}

/// Checks uniqueness in a program with a while loop.
#[test]
fn correctness6() {
    let src = r#"
    def main(a: Int, b: Int): Bool := {
        let a: Int = 1;
        let d: Bool = (a == 1);
        while d | false do {
            let a: Int = d + 1;
             d = !d;
        };
        d
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
}

/// Checks uniqueness in a program with nested if statements and while loops.
#[test]
fn correctness7() {
    let src = r#"
    def main(a: Int, b: Int): Int := {
        let c: Int = 2;
        let d: Bool = !(c == 1);
        let a: Int = -(1);
        if a < 2 & true then {
            while d | false do {
                a = a + 1;
                d = !d;
                if a then {
                    let d: Int = d + 1;
                };
            };
            a
        } else {
            d
        };
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
}

/// Checks uniqueness in a complicated program.
#[test]
fn correctness8() {
    let src = r#"
    def println(a: Int, b:  Int, c: Int): Unit := {
        a = b + c;
        while a | (b & c) do {
            let x: Int = {
                let a: Int = b;
                b
            };
        };
    }

    def main(a: Int, b: Int): Int := {
        let c: Int = 2;
        let d: Bool = !(c == 1);
        let a: Int = -(1);
        if a < 2 & true then {
            while d | false do {
                a = a + 1;
                d = !d;
                println(123, a, d);
            };
            a
        } else {
            d
        };
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
}