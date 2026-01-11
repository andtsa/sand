//! Tests for the uniquify pass of the compiler
use std::collections::HashSet;

use untitled::lang::*;
use untitled::reserved::RESERVED_FUNCTION_NAMES;
// ----------------------------------------------- Helper
// ------------------------------------------------------

/// Checks that all variable and function names in the provided program AST are
/// unique. It does so by traversing all blocks and collecting all declared
/// names in a HashSet. # Arguments
/// * 'prog' - The Program AST to check
/// # Returns
/// 'Ok(())' if all names are unique; otherwise, 'Err(name)' for the first
/// duplicate it finds.
fn assert_unique(prog: &Program) -> Result<(), String> {
    let mut seen_funs: HashSet<String> = RESERVED_FUNCTION_NAMES
        .iter()
        .map(|s| s.to_string())
        .collect();

    for func in &prog.0 {
        if !RESERVED_FUNCTION_NAMES.contains(&func.name.as_str()) {
            if !seen_funs.insert(func.name.clone()) {
                return Err(format!("Duplicate function name: {}", func.name));
            }
        }

        let mut local_seen_vars = HashSet::new();
        for param in &func.parameters {
            if !local_seen_vars.insert(param.name.clone()) {
                return Err(format!(
                    "Duplicate parameter name in function {}: {}",
                    func.name, param.name
                ));
            }
        }

        check_expr(&func.body, &mut local_seen_vars)?;
    }

    Ok(())
}

/// Recursively checks an expression AST for uniqueness of all declared
/// identifiers. # Arguments
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

        Expression::Block {
            statements,
            expr: inner_expr,
        } => {
            let mut block_seen = seen.clone();
            for stmt in statements {
                check_stmt(stmt, &mut block_seen)?;
            }
            if let Some(e) = inner_expr {
                check_expr(e, &mut block_seen)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Recursively checks a statement AST for uniqueness of all declared
/// identifiers. # Arguments
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

        Statement::Assignment { val, .. } => check_expr(val, seen),

        Statement::Expr(e) => check_expr(e, seen),
    }
}

/// Compares the behavior of the original and uniquified programs by
/// interpreting both and expecting that they produce the same result.
/// # Arguments
/// * 'original' - The original program AST.
/// * 'uniquified' - The program AST after being passed through uniquify.
fn assert_sound(original: &Program, uniquified: &Program) -> bool {
    let value1 = original.interpret().unwrap();
    let value2 = uniquified.interpret().unwrap();
    value1 == value2
}

// ----------------------------------------------- Helper
// ------------------------------------------------------

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
    assert!(assert_sound(&original, &uniquified));
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
    assert!(assert_sound(&original, &uniquified));
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
    assert!(assert_sound(&original, &uniquified));
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
    assert!(assert_sound(&original, &uniquified));
}

/// Checks uniqueness in a program with an if statement.
#[test]
fn correctness5() {
    let src = r#"
    def main(a: Int, b: Int): Int := {
        let a: Int = 4;
        if a < 10 then {
            a
        } else {
            2
        };
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_sound(&original, &uniquified));
}

/// Checks uniqueness in a program with a while loop.
#[test]
fn correctness6() {
    let src = r#"
    def main(a: Int, b: Int): Bool := {
        let a: Int = 10;
        let d: Bool = (a != 1);
        while d do {
            a = a - 1;
            let a: Int = a;
             d = (a != 1);
        };
        d
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_sound(&original, &uniquified));
}

/// Checks uniqueness in a program with nested if statements and while loops.
#[test]
fn correctness7() {
    let src = r#"
    def main(a: Int, b: Int): Int := {
        let b: Int = 10;
        let d: Bool = true;
        let a: Int = -1;
        if a < 2 then {
            while d do {
                a = a + 1;
                d = !d;
                if a==0 then {
                    let d: Int = b + 1;
                    d
                };
            };
            a
        } else {
            a
        };
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_sound(&original, &uniquified));
}

/// Checks uniqueness in a complicated program.
#[test]
fn correctness8() {
    let src = r#"
    def power(base: Int, exp: Int): Int := {
        let result: Int = 1;
        let i: Int = 0;

        // multiply result by base exp times
        while i < exp do {
            result = result * base;
            i = i + 1;
        };

        result
    }

    def main(): Int := {
        let b: Int = 3;
        let e: Int = 4;

        let p: Int = power(b, e);

        let sum: Int = 0;
        let j: Int = 1;
        while j ≤ e do {
            sum = sum + power(j, j);
            j = j + 1;
        };

       sum
    }
    "#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_sound(&original, &uniquified));
}

/// Check uniqueness in recursive functions and var-fun shadowing
#[test]
fn correctness9() {
    let src = r#"
    def fib(x: Int): Int := if x ≤ 1 then x else fib(x - 1) + fib(x - 2)

    def main(): Unit := {
        let x: Int = 10;

        let fib: Int = fib(x);
        fib
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_sound(&original, &uniquified));
}

/// Check uniqueness in mutually recursive functions and var-fun shadowing
#[test]
fn correctness10() {
    let src = r#"
    def even(x: Int): Bool := if x == 0 then true else odd(x - 1)
    def odd(x: Int): Bool := if x == 0 then false else even(x - 1)

    def main(): Unit := {
        let x: Int = 10;

        even(x)
    }"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_sound(&original, &uniquified));
}

/// Check uniqueness with different function ordering
#[test]
fn correctness11() {
    let src = r#"
    def even(x: Int): Bool := if x == 0 then true else odd(x - 1)

    def main(): Unit := {
        let x: Int = 10;

        let y: Bool = even(x) | odd(x);
    }

    def odd(x: Int): Bool := if x == 0 then false else even(x - 1)"#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_sound(&original, &uniquified));
}

/// Check uniqueness with reserved function names and shadowing
#[test]
fn correctness12() {
    let src = r#"
    def square(a: Int): Int := {
        print(a);
        a * a
    }

    def main(): Int := {
        let a: Int = 10;
        let println: Int = a + a;
        let x: Int = {
            println(println);
            square(println)
        };
        x
    }
    "#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
}

/// Check uniqueness with shadowing parameters
#[test]
fn correctness13() {
    let src = r#"
    def main(a: Int, a: Int): Int := {
       let a: Int = 1;
       a
    }
    "#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_sound(&original, &uniquified));
}

/// Check uniqueness with condition blocks
#[test]
fn correctness14() {
    let src = r#"
    def main(): Bool := {
       let a: Int = 0;
       if ({
        let a: Int = 1;
        if a < 0 then {
            true
        } else {
            false
        }
       }) then true else false
    }
    "#;
    let original: Program = Program::parse(src).unwrap();
    let uniquified = original.uniquify();
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_sound(&original, &uniquified));
}

/// Check uniqueness with unbound var access
#[test]
fn correctness15() {
    let src = r#"
    def main(): Int := {
       let a: Int = 0;
       if ({
        let b: Int = 1;
        if b < 0 then {
            true
        } else {
            false
        }
       }) then b else a
    }
    "#;
    let original: Program = Program::parse(src).unwrap();
    let result = std::panic::catch_unwind(|| original.uniquify());
    assert!(result.is_err());
}
