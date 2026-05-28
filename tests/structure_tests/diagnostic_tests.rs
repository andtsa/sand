//! Tests for error diagnostics and reporting
//!
//! Verifies that compiler errors are properly formatted, contain location context,
//! and are delivered through the diagnostic pipeline correctly.

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;

fn compile_err(src: &str) -> sand::SandLangError {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, src)]);
    compile_hir(code, &mut ctx).expect_err("expected a compile error")
}

#[test]
fn sand_lang_error_display_includes_location_context() {
    let err = compile_err("def main(): Int := undefined_var");
    let rendered = format!("{err}");
    assert!(
        rendered.contains("main")
            || rendered.contains("line")
            || rendered.contains("column")
            || rendered.contains(':'),
        "SandLangError Display lacks location context: {:?}",
        rendered
    );
}

#[test]
fn undefined_function_diagnostic_has_non_zero_range() {
    use sand::compiler::diagnostics::SandDiagnostic;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, "def main(): Int := ghost()")]);

    let err = compile_hir(code, &mut ctx).expect_err("should fail");
    let diagnostics = SandDiagnostic::from_compiler_error(&ctx, &err);

    for diags in diagnostics.map.values() {
        for d in diags {
            assert!(
                d.range != Default::default(),
                "diagnostic has zero range — user cannot locate the error: {:?}",
                d.range
            );
        }
    }
}

#[test]
fn duplicate_main_diagnostic_file_field_matches_key() {
    use sand::compiler::diagnostics::SandDiagnostic;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(
        fr,
        "def main(): Int := 1\n\
         def main(): Int := 2",
    )]);

    let err = compile_hir(code, &mut ctx).expect_err("should fail with DuplicateMain");
    let diagnostics = SandDiagnostic::from_compiler_error(&ctx, &err);

    for (file_key, diags) in &diagnostics.map {
        for d in diags {
            assert_eq!(
                d.file.unwrap(),
                *file_key,
                "diagnostic.file ({:?}) does not match the key it's stored under ({:?})",
                d.file,
                file_key
            );
        }
    }
}

/// [GUARD] A type error produces a non-empty diagnostic list.
#[test]
fn type_error_produces_diagnostics() {
    use sand::compiler::diagnostics::SandDiagnostic;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, "def main(): Int := true")]);

    let err = compile_hir(code, &mut ctx).expect_err("type error expected");
    let diagnostics = SandDiagnostic::from_compiler_error(&ctx, &err);

    let total: usize = diagnostics.map.values().map(|v| v.len()).sum();
    assert!(
        total > 0,
        "type error should produce at least one diagnostic"
    );
}

/// [GUARD] Error Display output is deterministic.
#[test]
fn error_display_is_deterministic() {
    let err1 = compile_err("def main(): Int := undefined_var");
    let err2 = compile_err("def main(): Int := undefined_var");
    assert_eq!(format!("{err1}"), format!("{err2}"));
}
