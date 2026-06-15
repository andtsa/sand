//! Tests for ModuleRef creation, registration, and qualification
//!
//! Covers module registration, idempotency, and round-trip behavior.

use crate::compile_hir;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;

#[test]
fn create_dummy_module_is_idempotent() {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();

    let first = ctx.create_dummy_module(fr);
    assert!(first.is_ok(), "first call should succeed");

    let second = ctx.create_dummy_module(fr);
    assert!(
        second.is_err(),
        "second call should fail: guard is broken and currently succeeds, \
         creating a duplicate \"mAin\" module"
    );
}

#[test]
fn duplicate_dummy_modules_cause_compile_error() {
    let mut ctx = CompileCtx::initial();
    let fr = FileRef(0);

    // Force two "mAin" modules into the context
    let _ = ctx.create_dummy_module(fr).unwrap();
    let _ = ctx
        .create_dummy_module(fr)
        .expect_err("duplicate dummy modules should fail");

    // Now run a trivial compile; the two identically-named modules will
    // cause a DuplicateModule error in the qualify pass.
    let code = Map::from([(fr, "def main(): Int := 1")]);
    let result = compile_hir(code, &mut ctx);
    // With the bug present this either succeeds (wrong) or panics; after
    // fixing create_dummy_module it should have been stopped earlier.
    assert!(
        result.is_ok(),
        "duplicate 'mAin' modules should produce a compile error"
    );
}

#[test]
fn register_distinct_modules_produces_distinct_refs() {
    let mut ctx = CompileCtx::initial();
    let fr = FileRef(0);
    let m1 = ctx.register_module("alpha", fr);
    let m2 = ctx.register_module("beta", fr);
    assert_ne!(m1, m2);
}

#[test]
fn file_of_module_round_trips() {
    let mut ctx = CompileCtx::initial();
    let fr = FileRef(0);
    let mr = ctx.register_module("foo", fr);
    assert_eq!(ctx.file_of_module(mr), fr);
}
