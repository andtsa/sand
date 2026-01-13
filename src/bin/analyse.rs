//! analyse a single file
//!
//! - read input file from command line args
//! - find repeated expressions
//! - print to stdout

use untitled::lang::Program;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input-file>", args[0]);
        std::process::exit(1);
    }

    let input_file = &args[1];
    let program_src = std::fs::read_to_string(input_file)
        .map_err(|e| anyhow::anyhow!("failed to read input file {}: {}", input_file, e))?;

    let ast = Program::parse(&program_src)?.uniquify()?;
    let annotations = untitled::analyse(&ast)?;

    println!("Program Annotations:\n{:#?}", annotations.expr_occurrences);

    Ok(())
}
