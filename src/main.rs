use pest::Parser;
use pest_derive::Parser;

pub mod lang;
pub mod parse;

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct MathParser;

fn print_pairs(pairs: pest::iterators::Pairs<Rule>, indent: usize) {
    let indent_str = "  ".repeat(indent);

    for pair in pairs {
        println!("{}{:?}: {}", indent_str, pair.as_rule(), pair.as_str());
        print_pairs(pair.into_inner(), indent + 1);
    }
}

        // a = (((a) * 2) - c) * ( a / b ));
//
fn main() -> anyhow::Result<()> {
    let input = "\
    def main(a: Int, b: Int): Int := { \
        let c: Int = 2;
        let d: Bool = !c;
        a =  a > 2 & true;
        if a > 2 & true then { \
            a;\
        } else {\
            b;\
        };\
    }";

    let pairs = MathParser::parse(Rule::program, input)?;

    print_pairs(pairs, 0);
    Ok(())
}
