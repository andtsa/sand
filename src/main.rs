use pest::Parser;
use pest_derive::Parser;

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

fn main() {
    let input = "\
    def main(a: Int, b: Int): Int := { \
        var c: Int = 2
        a = a * 2 - c * ( a / b )
        if a>2 and true then { \
            return a \
        } else { \
            return b\
        } \
    }";

    let pairs = MathParser::parse(Rule::program, input)
        .expect("Failed to parse input");

    print_pairs(pairs, 0);
}