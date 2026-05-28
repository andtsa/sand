//! Integration tests for multi-file compilation
//!
//! Verifies that the compiler correctly handles multi-file projects with
//! cross-file module references and function calls.

use lang::compile_hir;
use lang::compiler::context::CompileCtx;
use lang::compiler::context::ProjectCtx;
use lang::compiler::structure::Map;
use url::Url;

fn uri(s: &str) -> Url {
    Url::parse(s).unwrap()
}

/// [GUARD] Two files where one calls a function from the other should
/// compile successfully when both are passed in the same Map.
#[test]
fn multi_file_cross_call_compiles() {
    let mut project_ctx = ProjectCtx::initial();
    let fr_a = project_ctx
        .register_file(uri("file:///project/lib.sand"))
        .expect("lib.sand ok");
    let fr_b = project_ctx
        .register_file(uri("file:///project/main.sand"))
        .expect("main.sand ok");

    let mut ctx = CompileCtx::initial();
    ctx.create_default_module(fr_a, "lib");
    ctx.create_default_module(fr_b, "main_mod");

    let code = Map::from([
        (fr_a, "def double(x: Int): Int := x * 2"),
        (fr_b, "def main(): Int := lib::double(21)"),
    ]);

    let result = compile_hir(code, &mut ctx);
    assert!(
        result.is_ok(),
        "cross-module call should compile: {:?}",
        result.err()
    );
}
