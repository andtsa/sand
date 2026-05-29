//! Shared test utilities for all test suites
//!
//! This module provides common helper functions for:
//! - Parsing and compilation at various IR layers
//! - Running programs in HIR and MIR interpreters
//! - Loading example programs from files

#![allow(dead_code)]

use lang::SandLangError;
use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::interpreter::mir::MirValue;
use lang::ir_types::hhir::ProgramModule;
use lang::ir_types::mir::MirProgram;
use lang::ir_types::qhir;
use lang::ir_types::typed_hir::Expression;
use lang::ir_types::typed_hir::TypedProgram;

pub fn compile_hir_ctx(src: &str) -> anyhow::Result<(CompileCtx<'_>, TypedProgram)> {
    let mut proj = Project::empty();
    proj.create_virtual_file(src.to_string(), &std::panic::Location::caller().to_string());
    Ok(proj.check().result()?)
}

pub fn compile_hir(src: &str) -> anyhow::Result<TypedProgram> {
    Ok(compile_hir_ctx(src)?.1)
}

pub fn compile_err_ctx(src: &str) -> (CompileCtx<'_>, SandLangError) {
    let mut proj = Project::empty();
    proj.create_virtual_file(src.to_string(), &std::panic::Location::caller().to_string());
    proj.check().ctx_err().expect("expected compile error")
}

pub fn compile_err(src: &str) -> SandLangError {
    compile_err_ctx(src).1
}

/// Parse source code into an HHIR ProgramModule (no qualification or type
/// checking)
pub fn parse(src: &str) -> ProgramModule {
    let mut ctx = CompileCtx::initial();
    ProgramModule::parse_stub(&mut ctx, src).expect("parse failed")
}

/// Parse source code and expect it to fail at the parse stage
pub fn parse_fails(src: &str) {
    let mut ctx = CompileCtx::initial();
    assert!(
        ProgramModule::parse_stub(&mut ctx, src).is_err(),
        "expected parse to fail, but it succeeded"
    );
}

/// Parse → qualify source code into a QHIR Program (no type checking)
pub fn qualify(src: &str) -> qhir::Program {
    let mut ctx = CompileCtx::initial();
    let pm = ProgramModule::parse_stub(&mut ctx, src).expect("parse failed");
    qhir::Program::combine(&mut ctx, vec![pm]).expect("qualify failed")
}

/// Parse → qualify and expect qualification to fail
pub fn qualify_fails(src: &str) {
    let mut ctx = CompileCtx::initial();
    let pm = ProgramModule::parse_stub(&mut ctx, src).expect("parse ok");
    assert!(
        qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
        "expected qualify to fail, but it succeeded"
    );
}

/// Parse → qualify → type-check source code into a TypedProgram
pub fn typecheck(src: &str) -> TypedProgram {
    compile_hir(src).expect("compile failed")
}

/// Parse → qualify → type-check and expect type checking to fail
pub fn typecheck_fails(src: &str) {
    assert!(
        compile_hir(src).is_err(),
        "expected compile to fail, but it succeeded"
    );
}

/// Run source code through full HIR compilation and interpret in HIR
/// interpreter
pub fn run_hir(src: &str) -> Expression {
    let (ctx, prog) = compile_hir_ctx(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    prog.interpret(&ctx)
        .unwrap_or_else(|e| panic!("HIR interpret failed: {e}"))
}

/// Run source code through full HIR compilation and expect HIR interpretation
/// to fail
pub fn run_hir_fails(src: &str) {
    let (ctx, prog) = compile_hir_ctx(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    assert!(
        prog.interpret(&ctx).is_err(),
        "expected HIR interpret to fail, but it succeeded"
    );
}

/// Run source code through full HIR compilation, lower to MIR, and interpret in
/// MIR interpreter
pub fn run_mir(src: &str) -> MirValue {
    let (ctx, ast) = compile_hir_ctx(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    let mir = MirProgram::from_typed_program(&ast);
    mir.interpret(&ctx)
        .unwrap_or_else(|e| panic!("MIR interpret failed: {e}"))
}

/// Run source code through full HIR compilation, lower to MIR, and interpret in
/// MIR interpreter, returning the Result to allow testing for expected failures
pub fn run_mir_result(src: &str) -> Result<MirValue, lang::interpreter::mir::MirInterpError> {
    let (ctx, ast) = compile_hir_ctx(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    let mir = MirProgram::from_typed_program(&ast);
    mir.interpret(&ctx)
}

/// Load an example program from the examples/ directory and parse it
pub fn open_example_from_file(name: &str) -> String {
    let path = format!("../examples/{}.sand", name);
    std::fs::read_to_string(path).expect("failed to read example file")
}

/// Load, compile, and interpret an example program in the HIR interpreter
pub fn interpret_example(name: &str) -> anyhow::Result<Expression> {
    let src = open_example_from_file(name);
    let (ctx, program) = compile_hir_ctx(&src)?;
    let hir_result = program.interpret(&ctx)?;
    // Verify agreement with MIR interpreter
    assert_eq!(hir_result, interpret_mir_example(name)?);
    Ok(hir_result)
}

/// Load, compile, and interpret an example program in the MIR interpreter
pub fn interpret_mir_example(name: &str) -> anyhow::Result<Expression> {
    let src = open_example_from_file(name);
    let (ctx, ast) = compile_hir_ctx(&src)?;
    let mir = MirProgram::from_typed_program(&ast);
    let result = mir.interpret(&ctx).map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(match result {
        MirValue::Int(i) => Expression::Int(i),
        MirValue::Bool(b) => Expression::Bool(b),
        MirValue::Unit => Expression::Unit,
        MirValue::EnumVariant {
            enum_ref,
            variant_idx,
        } => Expression::Constructor {
            enum_ref,
            variant_idx,
        },
    })
}
