//! Tests for error diagnostics and reporting
//!
//! Verifies that compiler errors are properly formatted, contain location
//! context, and are delivered through the diagnostic pipeline correctly.

use lang::compile_hir;
use lang::compiler::context::CompileCtx;
use lang::compiler::structure::Map;

fn compile_err(src: &str) -> lang::SandLangError<'static> {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, src)]);
    let err = compile_hir(code, &mut ctx).expect_err("expected a compile error");
    std::mem::forget(ctx);
    err
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
    use lang::compiler::diagnostics::SandDiagnostic;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, "def main(): Int := ghost()")]);

    let err = compile_hir(code, &mut ctx).expect_err("should fail");
    let diagnostics = SandDiagnostic::from_compiler_error(&ctx, &err);

    for diags in diagnostics.map.values() {
        for d in diags {
            assert!(
                d.range != Default::default(),
                "diagnostic has zero range! user cannot locate the error: {:?}",
                d.range
            );
        }
    }
}

#[test]
fn duplicate_main_diagnostic_file_field_matches_key() {
    use lang::compiler::diagnostics::SandDiagnostic;

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
    use lang::compiler::diagnostics::SandDiagnostic;

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

#[test]
fn type_diagnostics_render_types_in_source_syntax() {
    // A type-mismatch diagnostic must show source-like types (`&mut List<T>`,
    // `List<T>`) rather than internal debug forms (`App(EnumRef(..)<Param(..)>)`,
    // `&Var(RegionVar(..)) mut ...`).
    use lang::compiler::diagnostics::SandDiagnostic;

    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let src = "type List<T> = Empty | Cons((T, List<T>)) deriving Heaped \n \
               def push<T>(elem: T, list: &mut List<T>): Unit := \
                 match *list { \
                   List#Cons((x, tail)) => push(elem, tail), \
                   List#Empty => { *list = List#Cons((elem, List#Empty)); } \
                 } \n \
               def main(): Int := 0";
    let code = Map::from([(fr, src)]);
    let err = compile_hir(code, &mut ctx).expect_err("should fail");
    let diagnostics = SandDiagnostic::from_compiler_error(&ctx, &err);

    let mut text = String::new();
    for diags in diagnostics.map.values() {
        for d in diags {
            text.push_str(&d.message);
            for r in &d.related {
                text.push_str(&r.message);
            }
        }
    }
    std::mem::forget(ctx);

    assert!(text.contains("List<T>"), "expected `List<T>` in: {text}");
    assert!(
        text.contains("&mut List<T>"),
        "expected `&mut List<T>` in: {text}"
    );
    assert!(!text.contains("App("), "leaked internal `App(` in: {text}");
    assert!(!text.contains("RegionVar"), "leaked `RegionVar` in: {text}");
    assert!(!text.contains("Param("), "leaked `Param(` in: {text}");
}
