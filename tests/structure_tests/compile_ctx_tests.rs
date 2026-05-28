//! Tests for CompileCtx functionality
//!
//! Verifies compilation context behavior for stub file handling and
//! module registration edge cases not covered by the module-level tests
//! in src/compiler/tests/.

use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::structure::Map;

/// [GUARD] stub_file() panics if modules are already registered.
#[test]
fn stub_file_panics_after_module_registration() {
    let result = std::panic::catch_unwind(|| {
        let mut ctx = CompileCtx::initial();
        let fr = ctx.stub_file();
        ctx.register_module("foo", fr);
        ctx.stub_file()
    });
    assert!(
        result.is_err(),
        "stub_file() should panic when modules are already registered"
    );
}

/// [GUARD] After a failed compile, entrypoint remains None.
#[test]
fn entrypoint_is_none_after_failed_compile() {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, "def main(): Int := undefined_var")]);
    let _ = compile_hir(code, &mut ctx);
    assert!(
        ctx.entrypoint.is_none(),
        "entrypoint should remain None after a failed compile"
    );
}

/// [GUARD] A normal single-file compile populates entrypoint.
#[test]
fn entrypoint_is_set_after_successful_compile() {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, "def main(): Int := 42")]);
    let _prog = compile_hir(code, &mut ctx).expect("compile ok");
    assert!(
        ctx.entrypoint.is_some(),
        "entrypoint should be set after compiling a program with a main function"
    );
}

/// [GUARD] is_main returns true for the entrypoint and false for helpers.
#[test]
fn is_main_correct_after_compilation() {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, "def helper(): Int := 1  def main(): Int := helper()")]);
    compile_hir(code, &mut ctx).expect("compile ok");

    let entrypoint = ctx.entrypoint.expect("entrypoint set");
    assert!(ctx.is_main(entrypoint), "is_main(entrypoint) should be true");

    let any_non_main = ctx.all_functions().any(|fr| !ctx.is_main(fr));
    assert!(any_non_main, "at least one function should not be main");
}

/// [GUARD] A program with no `main` function should fail qualification.
#[test]
fn program_without_main_fails_to_compile() {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, "def helper(): Int := 1")]);
    let result = compile_hir(code, &mut ctx);
    assert!(
        result.is_err() || ctx.entrypoint.is_none(),
        "a program without main should either fail or leave entrypoint unset"
    );
}
