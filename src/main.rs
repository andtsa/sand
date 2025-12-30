use crate::lang::Program;
use crate::parse::Rule;

pub mod ast;
pub mod lang;
pub mod parse;

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
        let d: Bool = !c;
        a =  a > 2 & true;
        if a > 2 & true then { \
            a\
        } else {\
            b\
        };\
        \
    }";
    let src = r#"
        def main(x: Int, y: Int): Int := {
            let z: Int = x + y * 2;
            z
        }
        
    "#;

    let program = Program::parse(src)?;
    println!("{:#?}", program);

    Ok(())
}
