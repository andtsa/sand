//! lower a source file to MIR and print it

use std::path::PathBuf;

use lang::castles::project::Project;
use lang::ir_types::mir::MirProgram;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input-file(s)>...", args[0]);
        std::process::exit(1);
    }

    let proj = Project::from_paths(&args[1..].iter().map(PathBuf::from).collect::<Vec<_>>())?.ok();
    let (ctx, ast) = proj.check().result_leaked()?;
    let mir = MirProgram::from_typed_program(&ast, &ctx);

    print!("{}", mir.dump(&ctx));

    Ok(())
}
