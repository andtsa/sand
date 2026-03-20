//! helper methods for integration tests
#![allow(dead_code)]

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::ir_types::typed_hir::Expression;

pub fn open_example_from_file(name: &str) -> String {
    let path = format!("examples/{}.sand", name);
    std::fs::read_to_string(path).expect("failed to read example file")
}

pub fn interpret_example(name: &str) -> anyhow::Result<Expression> {
    let mut ctx = CompileCtx::initial();
    let src = open_example_from_file(name);
    let code = Map::from([(ctx.register_dummy_file(), src.as_str())]);
    let program = compile_hir(code, &mut ctx)?;
    let result = program.interpret(&ctx)?;
    Ok(result)
}
