//! lower a source file to CFG-MIR and print it

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::ir_types::cfgmir::MirProgram;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input-file>", args[0]);
        std::process::exit(1);
    }

    let src = std::fs::read_to_string(&args[1])
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", args[1], e))?;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let ast = compile_hir(Map::from([(fr, src.as_str())]), &mut ctx)?;
    let mir = MirProgram::from_typed_program(&ast);

    print!("{}", mir.dump(&ctx));

    Ok(())
}
