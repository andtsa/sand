//! lower a source file to MIR and print it

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::ir_types::mir::MirProgram;
use sand::passes::llvm_codegen::LlvmCodegen;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 && args.len() != 3 {
        eprintln!("Usage: {} <input-file> [output.o]", args[0]);
        std::process::exit(1);
    }

    let src = std::fs::read_to_string(&args[1])
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", args[1], e))?;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.dummy_file();
    let ast = compile_hir(Map::from([(fr, src.as_str())]), &mut ctx)?;
    let mir = MirProgram::from_typed_program(&ast);
    let llvm_ctx = inkwell::context::Context::create();
    let codegen = LlvmCodegen::new(&llvm_ctx, "sand_module");
    codegen.emit_program(&mir, &ctx)?;
    if args.len() == 3 {
        codegen.write_object(&args[2], false)?;
    } else {
        codegen.write_object("output.o", false)?;
    }

    Ok(())
}
