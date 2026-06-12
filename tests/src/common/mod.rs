//! Shared test utilities for all test suites
//!
//! This module provides common helper functions for:
//! - Parsing and compilation at various IR layers
//! - Running programs in HIR and MIR interpreters
//! - Loading example programs from files

#![allow(dead_code)]

use std::path::Path;
use std::path::PathBuf;

use lang::SandLangError;
use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::interpreter::mir::MirValue;
use lang::ir_types::hhir::ProgramModule;
use lang::ir_types::mir::MirProgram;
use lang::ir_types::qhir;
use lang::ir_types::typed_hir::Expr;
use lang::ir_types::typed_hir::Expression;
use lang::ir_types::typed_hir::TypedProgram;

/// Wrap a bare `Expression` into an `Expr` with placeholder `ty`/`range` —
/// only the `expr` field participates in `Expr`'s `PartialEq` impl, so these
/// placeholders are sound when the result is solely used for comparison
/// against the HIR interpreter's output (see `mir_value_to_expr`).
fn dummy_expr(expr: Expression<'static>, ctx: &CompileCtx<'static>) -> Expr<'static> {
    Expr {
        expr,
        ty: ctx.types.unit,
        range: Default::default(),
    }
}

/// Recursively translate a runtime `MirValue` into the `Expression` shape
/// produced by the HIR interpreter, so the two interpreters' results can be
/// compared with `assert_eq!`.
pub fn mir_value_to_expr(v: MirValue<'static>, ctx: &CompileCtx<'static>) -> Expression<'static> {
    match v {
        MirValue::Int(i) => Expression::Int(i),
        MirValue::Bool(b) => Expression::Bool(b),
        MirValue::Unit => Expression::Unit,
        MirValue::EnumVariant {
            enum_ref,
            variant_idx,
            payload,
        } => Expression::Constructor {
            enum_ref,
            variant_idx,
            payload: payload.map(|p| Box::new(dummy_expr(mir_value_to_expr(*p, ctx), ctx))),
        },
        MirValue::Tuple(elems) => Expression::Tuple(
            elems
                .into_iter()
                .map(|e| dummy_expr(mir_value_to_expr(e, ctx), ctx))
                .collect(),
        ),
    }
}

pub fn compile_hir_ctx(src: &str) -> anyhow::Result<(CompileCtx<'static>, TypedProgram<'static>)> {
    let mut proj = Project::empty();
    proj.create_virtual_file(src.to_string(), &std::panic::Location::caller().to_string());
    Ok(proj.check().result()?)
}

pub fn compile_hir(src: &str) -> anyhow::Result<TypedProgram<'static>> {
    let (ctx, ast) = compile_hir_ctx(src)?;
    // The returned `TypedProgram<'static>` borrows from `ctx`'s arena. Leak
    // `ctx` so the arena outlives it (test helper — process is short-lived).
    std::mem::forget(ctx);
    Ok(ast)
}

pub fn compile_err_ctx(src: &str) -> (CompileCtx<'static>, SandLangError<'static>) {
    let mut proj = Project::empty();
    proj.create_virtual_file(src.to_string(), &std::panic::Location::caller().to_string());
    proj.check().ctx_err().expect("expected compile error")
}

pub fn compile_err(src: &str) -> SandLangError<'static> {
    let (ctx, err) = compile_err_ctx(src);
    // `err` borrows from `ctx`'s arena (e.g. `Ty`/`ModuleRef`); leak `ctx`.
    std::mem::forget(ctx);
    err
}

/// Parse source code into an HHIR ProgramModule (no qualification or type
/// checking)
pub fn parse(src: &str) -> ProgramModule<'static> {
    let mut ctx = CompileCtx::initial();
    let pm = ProgramModule::parse_stub(&mut ctx, src).expect("parse failed");
    std::mem::forget(ctx);
    pm
}

/// Parse source code and expect it to fail at the parse stage
pub fn parse_fails(src: &str) {
    let mut ctx = CompileCtx::initial();
    assert!(
        ProgramModule::parse_stub(&mut ctx, src).is_err(),
        "expected parse to fail, but it succeeded"
    );
}

/// Parse -> qualify source code into a QHIR Program (no type checking)
pub fn qualify(src: &str) -> qhir::Program<'static> {
    let mut ctx = CompileCtx::initial();
    let pm = ProgramModule::parse_stub(&mut ctx, src).expect("parse failed");
    let prog = qhir::Program::combine(&mut ctx, vec![pm]).expect("qualify failed");
    std::mem::forget(ctx);
    prog
}

/// Parse -> qualify and expect qualification to fail
pub fn qualify_fails(src: &str) {
    let mut ctx = CompileCtx::initial();
    let pm = ProgramModule::parse_stub(&mut ctx, src).expect("parse ok");
    assert!(
        qhir::Program::combine(&mut ctx, vec![pm]).is_err(),
        "expected qualify to fail, but it succeeded"
    );
}

/// Parse -> qualify -> type-check source code into a TypedProgram
pub fn typecheck(src: &str) -> TypedProgram<'static> {
    compile_hir(src).expect("compile failed")
}

/// Parse -> qualify -> type-check and expect type checking to fail
pub fn typecheck_fails(src: &str) {
    assert!(
        compile_hir(src).is_err(),
        "expected compile to fail, but it succeeded"
    );
}

/// Run source code through full HIR compilation and interpret in HIR
/// interpreter
pub fn run_hir(src: &str) -> Expression<'static> {
    let (ctx, prog) = compile_hir_ctx(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    let result = prog
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("HIR interpret failed: {e}"));
    // `result` borrows from `ctx`'s arena; leak `ctx` (test helper).
    std::mem::forget(ctx);
    result
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

/// Compile `src` **once** and run both the HIR and MIR interpreters against
/// the same `CompileCtx`, returning `(hir_result, mir_result)`.
///
/// This shared-compilation form is required for any comparison that inspects
/// `EnumRef` identity: `EnumRef` is now an arena pointer, so the "same" enum
/// compiled in two separate `CompileCtx`s yields *different* handles. Running
/// both interpreters over one compilation keeps enum handles comparable.
pub fn run_hir_and_mir(src: &str) -> (Expression<'static>, Expression<'static>) {
    let (ctx, ast) = compile_hir_ctx(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    let hir = ast
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("HIR interpret failed: {e}"));
    let mir = MirProgram::from_typed_program(&ast, &ctx);
    let val = mir
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("MIR interpret failed: {e}"));
    let mir_expr = mir_value_to_expr(val, &ctx);
    std::mem::forget(ctx);
    (hir, mir_expr)
}

/// Run source code through full HIR compilation, lower to MIR, interpret in
/// MIR interpreter, and convert the result to `Expression` for comparison.
pub fn run_mir_as_expr(src: &str) -> Expression<'static> {
    let (ctx, ast) = compile_hir_ctx(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    let mir = MirProgram::from_typed_program(&ast, &ctx);
    let val = mir
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("MIR interpret failed: {e}"));
    let expr = mir_value_to_expr(val, &ctx);
    std::mem::forget(ctx);
    expr
}

/// Run source code through full HIR compilation, lower to MIR, and interpret in
/// MIR interpreter
pub fn run_mir(src: &str) -> MirValue<'static> {
    let (ctx, ast) = compile_hir_ctx(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    let mir = MirProgram::from_typed_program(&ast, &ctx);
    let val = mir
        .interpret(&ctx)
        .unwrap_or_else(|e| panic!("MIR interpret failed: {e}"));
    std::mem::forget(ctx);
    val
}

/// Run source code through full HIR compilation, lower to MIR, and interpret in
/// MIR interpreter, returning the Result to allow testing for expected failures
pub fn run_mir_result(
    src: &str,
) -> Result<MirValue<'static>, lang::interpreter::mir::MirInterpError> {
    let (ctx, ast) = compile_hir_ctx(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    let mir = MirProgram::from_typed_program(&ast, &ctx);
    let result = mir.interpret(&ctx);
    std::mem::forget(ctx);
    result
}

/// Load an example program from the examples/ directory and parse it
pub fn open_example_from_file(name: &str) -> String {
    let path = format!("../examples/{}.sand", name);
    std::fs::read_to_string(path).expect("failed to read example file")
}

/// Load, compile, and interpret an example program in the HIR interpreter
pub fn interpret_example(name: &str) -> anyhow::Result<Expression<'static>> {
    let src = open_example_from_file(name);
    let (ctx, program) = compile_hir_ctx(&src)?;
    let hir_result = program.interpret(&ctx)?;
    std::mem::forget(ctx);
    // Verify agreement with MIR interpreter
    assert_eq!(hir_result, interpret_mir_example(name)?);
    Ok(hir_result)
}

/// parse -> qualify -> type-check -> ownership-check and expect the program to
/// pass ownership checking (i.e., compile successfully)
pub fn ownership_ok(src: &str) {
    compile_hir(src).unwrap_or_else(|e| panic!("expected ownership check to pass, got: {e}"));
}

/// parse -> qualify -> type-check -> ownership-check and expect an ownership
/// violation error
pub fn ownership_fails(src: &str) {
    assert!(
        compile_hir(src).is_err(),
        "expected ownership check to fail, but it succeeded"
    );
}

/// Load, compile, and interpret an example program in the MIR interpreter
pub fn interpret_mir_example(name: &str) -> anyhow::Result<Expression<'static>> {
    let src = open_example_from_file(name);
    let (ctx, ast) = compile_hir_ctx(&src)?;
    let mir = MirProgram::from_typed_program(&ast, &ctx);
    let result = mir.interpret(&ctx).map_err(|e| anyhow::anyhow!("{}", e))?;
    let expr = mir_value_to_expr(result, &ctx);
    std::mem::forget(ctx);
    Ok(expr)
}

pub fn project_root() -> PathBuf {
    let cargo_manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|e| panic!("could not get CARGO_MANIFEST_DIR: {e}"));
    Path::new(&cargo_manifest_dir)
        .canonicalize()
        .unwrap_or_else(|e| {
            panic!("could not canonicalise cargo_manifest_dir {cargo_manifest_dir:?}: {e}")
        })
        .parent()
        .unwrap()
        .to_path_buf()
}

pub fn target_dir() -> PathBuf {
    let project_root = project_root();
    project_root.join("target")
}

pub fn temp_dir() -> PathBuf {
    let temp_dir = target_dir().join("tmp");
    if !temp_dir.exists() {
        std::fs::create_dir_all(&temp_dir).expect("failed to create cargo temporary directory");
    }
    temp_dir
}

pub fn temp_file(from: &str) -> PathBuf {
    let output_name =
        from.replace(['/', ' ', ':'], "_") + &uuid::Uuid::new_v4().simple().to_string();
    let temp_path = temp_dir().join(&output_name);
    if temp_path.exists() {
        std::fs::remove_file(&temp_path).expect("failed to remove existing test.out");
    }
    temp_path
}

pub fn sand_cli() -> PathBuf {
    let cli_path = target_dir().join("debug").join("sand-cli");
    // assert that the compiler binary exists
    assert!(
        cli_path.canonicalize().is_ok_and(|p| p.is_file()),
        "sand-cli binary not found at {:?}",
        cli_path
    );

    cli_path
}

/// Use the compiler binary to compile and run an example program,
/// retrieving the main function's return value from the exit code,
/// and the output lines from stdout
pub fn run_compiled(paths: &[&str]) -> anyhow::Result<(i32, Vec<String>)> {
    // find the sand-cli binary from cargo output path
    let cli_path = sand_cli();

    // make a path for the resulting binary in a cargo temporary directory output
    // filename should have some randomness to avoid collisions when tests are ran
    // in parallel
    let temp_path = temp_file(&paths.join("_"));
    // compile the binary
    let compilation_output = std::process::Command::new(&cli_path)
        .arg("compile")
        .args(
            paths
                .iter()
                .map(|p| project_root().join("examples").join(format!("{p}.sand"))),
        )
        .arg("--output")
        .arg(&temp_path)
        .arg("-v")
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run compiler from {:?}: {}", cli_path, e))?;
    // the compilation should succeed. if it doesn't, panic with the compiler's
    // error message
    if !compilation_output.status.success() {
        panic!(
            "compilation failed[{}]: {}{}",
            compilation_output.status,
            String::from_utf8_lossy(&compilation_output.stdout),
            String::from_utf8_lossy(&compilation_output.stderr)
        );
    }
    // run the compiled binary
    let output = std::process::Command::new(&temp_path)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run compiled binary at {:?}: {}", temp_path, e))?;
    // get the output value from the exit code
    let exit_code = output
        .status
        .code()
        .expect("compiled programs should not be killed by the OS");
    // get the output lines from stdout
    let output_lines = String::from_utf8_lossy(&output.stdout)
        .to_string()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect::<Vec<String>>();
    Ok((exit_code, output_lines))
}

pub fn assert_compile_fail(paths: &[&str]) -> anyhow::Result<()> {
    let cli_path = sand_cli();
    let compilation_output = std::process::Command::new(&cli_path)
        .arg("compile")
        .args(
            paths
                .iter()
                .map(|p| project_root().join("examples").join(format!("{p}.sand"))),
        )
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run compiler from {:?}: {}", cli_path, e))?;
    // the compilation should fail. if it doesn't, panic with the compiler's
    // error message
    assert!(
        !compilation_output.status.success(),
        "compilation should fail for {:?}",
        paths
    );
    Ok(())
}
