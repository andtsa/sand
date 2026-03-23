//! helper methods for integration tests
#![allow(dead_code)]

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;
use sand::interpreter::mir::MirValue;
use sand::ir_types::mir::MirProgram;
use sand::ir_types::typed_hir::Expression;

pub fn open_example_from_file(name: &str) -> String {
    let path = format!("examples/{}.sand", name);
    std::fs::read_to_string(path).expect("failed to read example file")
}

pub fn interpret_example(name: &str) -> anyhow::Result<Expression> {
    let mut ctx = CompileCtx::initial();
    let src = open_example_from_file(name);
    let code = Map::from([(ctx.dummy_file(), src.as_str())]);
    let program = compile_hir(code, &mut ctx)?;
    let hir_result = program.interpret(&ctx)?;
    assert_eq!(hir_result, interpret_mir_example(name)?);
    Ok(hir_result)
}

pub fn interpret_mir_example(name: &str) -> anyhow::Result<Expression> {
    let mut ctx = CompileCtx::initial();
    let src = open_example_from_file(name);
    let code = Map::from([(ctx.dummy_file(), src.as_str())]);
    let ast = compile_hir(code, &mut ctx)?;
    let mir = MirProgram::from_typed_program(&ast);
    let result = mir.interpret(&ctx).map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(match result {
        MirValue::Int(i) => Expression::Int(i),
        MirValue::Bool(b) => Expression::Bool(b),
        MirValue::Unit => Expression::Unit,
    })
}
