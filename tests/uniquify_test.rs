//! Tests for the uniquify pass of the compiler

use anyhow::Result;
use sand::ir_types::hhir::*;
use sand::passes::uniquify::reserved::assert_unique;
// ----------------------------------------------- Helper

/// Compares the behavior of the original and uniquified programs by
/// interpreting both and expecting that they produce the same result.
/// # Arguments
/// * 'original' - The original program AST.
/// * 'uniquified' - The program AST after being passed through uniquify.
fn assert_uniquify_sound(original: &ProgramModule, uniquified: &ProgramModule) -> bool {
    let value1 = original.interpret().unwrap();
    let value2 = uniquified.interpret().unwrap();
    value1 == value2
}

// ----------------------------------------------- Helper

/// Checks uniqueness in a very simple program.
#[test]
fn correctness1() -> Result<()> {
    let src = r#"
    def main(): Int := {
        let x: Int = 5;
        x
    }"#;
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Checks uniqueness in a very simple program.
#[test]
fn correctness2() -> Result<()> {
    let src = r#"
    def main(): Int := {
        let x: Int = 5;
        let y: Int = 2;
        let z: Int = x + y;
    }"#;
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Checks uniqueness in a program with a block.
#[test]
fn correctness3() -> Result<()> {
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
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Checks uniqueness in a program with multiple nested blocks.
#[test]
fn correctness4() -> Result<()> {
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
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Checks uniqueness in a program with an if statement.
#[test]
fn correctness5() -> Result<()> {
    let src = r#"
    def main(a: Int, b: Int): Int := {
        let a: Int = 4;
        if a < 10 then {
            a
        } else {
            2
        };
    }"#;
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Checks uniqueness in a program with a while loop.
#[test]
fn correctness6() -> Result<()> {
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
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Checks uniqueness in a program with nested if statements and while loops.
#[test]
fn correctness7() -> Result<()> {
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
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Checks uniqueness in a complicated program.
#[test]
fn correctness8() -> Result<()> {
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
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Check uniqueness in recursive functions and var-fun shadowing
#[test]
fn correctness9() -> Result<()> {
    let src = r#"
    def fib(x: Int): Int := if x ≤ 1 then x else fib(x - 1) + fib(x - 2)

    def main(): Unit := {
        let x: Int = 10;

        let fib: Int = fib(x);
        fib
    }"#;
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Check uniqueness in mutually recursive functions and var-fun shadowing
#[test]
fn correctness10() -> Result<()> {
    let src = r#"
    def even(x: Int): Bool := if x == 0 then true else odd(x - 1)
    def odd(x: Int): Bool := if x == 0 then false else even(x - 1)

    def main(): Unit := {
        let x: Int = 10;

        even(x)
    }"#;
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Check uniqueness with different function ordering
#[test]
fn correctness11() -> Result<()> {
    let src = r#"
    def even(x: Int): Bool := if x == 0 then true else odd(x - 1)

    def main(): Unit := {
        let x: Int = 10;

        let y: Bool = even(x) | odd(x);
    }

    def odd(x: Int): Bool := if x == 0 then false else even(x - 1)"#;
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Check uniqueness with reserved function names and shadowing
#[test]
fn correctness12() -> Result<()> {
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
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    Ok(())
}

/// Check uniqueness with shadowing parameters
#[test]
fn correctness13() -> Result<()> {
    let src = r#"
    def main(a: Int, a: Int): Int := {
       let a: Int = 1;
       a
    }
    "#;
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Check uniqueness with condition blocks
#[test]
fn correctness14() -> Result<()> {
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
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let uniquified = original.uniquify()?;
    assert!(assert_unique(&uniquified).is_ok());
    assert!(assert_uniquify_sound(&original, &uniquified));
    Ok(())
}

/// Check uniqueness with unbound var access
#[test]
fn correctness15() -> Result<()> {
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
    let original: ProgramModule = ProgramModule::parse(src).unwrap();
    let result = original.uniquify();
    assert!(result.is_err());
    Ok(())
}
