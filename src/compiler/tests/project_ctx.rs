//! Tests for ProjectCtx initialization, file registration, and consistency
//!
//! Covers ProjectCtx file management and ref consistency across contexts.

use crate::compile_hir;
use crate::compiler::context::CompileCtx;
use crate::compiler::context::ProjectCtx;
use crate::compiler::structure::{FileRef, Map};
use url::Url;

/// [GUARD] FileRefs produced by ProjectCtx must be usable as keys in the
/// Map passed to compile_hir, and the resulting CompileCtx module registry
/// must reference those same FileRefs (so file_of_module is consistent).
#[test]
fn compile_hir_file_ref_consistent_with_project_ctx() {
    let mut project_ctx = ProjectCtx::initial();
    let uri = Url::parse("file:///project/src/main.sand").unwrap();
    let fr = project_ctx.register_file(uri.clone()).expect("register ok");

    let mut compile_ctx = CompileCtx::initial();
    let mr = compile_ctx.create_default_module(fr, "main");

    // The module's file should be exactly the FileRef we got from ProjectCtx.
    assert_eq!(
        compile_ctx.file_of_module(mr),
        fr,
        "file_of_module should return the FileRef that was used at registration"
    );

    // Compile a trivial program using the real FileRef.
    let code = Map::from([(fr, "def main(): Int := 42")]);
    let result = compile_hir(code, &mut compile_ctx);
    assert!(
        result.is_ok(),
        "compile_hir should succeed with a consistently-registered FileRef"
    );
}

/// [BUG] When the same source is compiled twice through separate
/// CompileCtx instances (as the LSP does on each key-stroke), the
/// FileRefs from ProjectCtx carry over into both CompileCtx instances.
/// If module names collide (both default to "mAin"), the qualify pass
/// should detect a DuplicateModule — but because create_dummy_module's
/// guard is broken, it currently does not.
#[test]
fn second_compilation_with_same_file_ref_does_not_duplicate_modules() {
    let fr = FileRef(0);
    let src = "def main(): Int := 1";

    // First compilation
    let mut ctx1 = CompileCtx::initial();
    let code1 = Map::from([(fr, src)]);
    let r1 = compile_hir(code1, &mut ctx1);
    assert!(r1.is_ok(), "first compilation should succeed");

    // Second compilation with a fresh context (LSP pattern)
    let mut ctx2 = CompileCtx::initial();
    let code2 = Map::from([(fr, src)]);
    let r2 = compile_hir(code2, &mut ctx2);
    assert!(
        r2.is_ok(),
        "second compilation with a fresh ctx should also succeed"
    );
}
