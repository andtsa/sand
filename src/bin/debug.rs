#![allow(unused)]
use sand::ir_types::hhir::ProgramModule;
use sand::ir_types::typed_hir::TypedProgram;
use sand::passes::parse::Rule;

fn _print_pairs(pairs: pest::iterators::Pairs<Rule>, indent: usize) {
    let indent_str = "  ".repeat(indent);

    for pair in pairs {
        println!("{}{:?}: {}", indent_str, pair.as_rule(), pair.as_str());
        _print_pairs(pair.into_inner(), indent + 1);
    }
}

fn main() -> anyhow::Result<()> {
    let _input = "\
    def main(a: Int, b: Int): Int := { \
        let c: Int = 2;
        let d: Bool = !(c == 1);
        let a: Int = -(1); \
        if a < 2 & true then { \
            while d | false do { \
                a = a + 1; \
                d = !d; \
                println(123, a, d); \
            }; \
            a \
        } else {\
            d\
        };\
        \
    }";
    let _src = r#"
        def main(x: Int, y: Int): Int := {
            let z: Int = x + y * 2;
            z
        }
        
    "#;

    let _test = r#"
        def println(x: Int, y: Int): Int := {
            x + y
        }
        def main(): Int := {
            let a: Int = 10;
            let b: Int = 20;
            while a < b do {
                a = a + 1;
                println(a - b);
            };
            a
        }
        "#;

    let _test_2 = r#"
        def main(): Int := {
            let a: Int = 9;
            let x: Int = {
                let y: Int = 4;
                a = a + y;
                let z: Int = 3;
                y * z / a
            };

            let f: Int = 5 * 4 / a;

            5 * 4 / a
            
        }
    "#;

    let _test_3 = r#"
    def shadow(): Int := {
        let shadow: Int = 2;
        shadow
    }
    def main(): Int := {
    let a: Int = 1;
    let x: Int = {
        a = a + 1;
        let a: Int = 5;
        a = a + a;
        a + shadow()
    };
    a = 3;
    x
    }"#;

    let _test_4 = r#"
def main(): Int := {
    let a: Int = 1;
    let x: Int = {
        let x: Int = {
            let x: Int = {
                let x: Int = 3;
                x + a
            };
            x
        };
        x
    };
    x
}"#;

    let program = ProgramModule::parse(_test_4)?;
    // println!("{:#?}", program);

    let uniquified = ProgramModule::uniquify(&program)?;
    // let typed = TypedProgram::from_ast_program(&uniquified)?;
    println!("{:#?}", uniquified);
    // println!("{:#?}", typed);

    let eval_u = uniquified.interpret()?;
    // let eval_p = program.interpret()?;
    // println!(
    //     "Program evaluated to: {:?}\nUniquified evaluated to: {:?}",
    //     eval_p, eval_u
    // );
    Ok(())
}
