//! run a program

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input-file>", args[0]);
        std::process::exit(1);
    }

    let input_file = &args[1];
    let program_src = std::fs::read_to_string(input_file)
        .map_err(|e| anyhow::anyhow!("failed to read input file {}: {}", input_file, e))?;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.register_dummy_file();
    let ast = compile_hir(Map::from([(fr, program_src.as_str())]), &mut ctx)?;

    println!("{:?}", ast.interpret(&ctx));

    Ok(())
}
