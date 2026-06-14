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

/// A file with no `module` declaration becomes a module **named after the
/// file** (Step M, option 2): `geometry.sand`'s items live in module
/// `geometry`, and are reachable from another file via `geometry::…`. The
/// module name is *derived* from the file (`code_file(fr).module_name()`), not
/// a synthetic placeholder.
#[test]
fn a_files_default_module_is_named_after_the_file() {
    let mut project_ctx = ProjectCtx::initial();
    let fr_geo = project_ctx
        .register_file(uri("file:///project/geometry.sand"))
        .expect("geometry.sand ok");
    let fr_main = project_ctx
        .register_file(uri("file:///project/main.sand"))
        .expect("main.sand ok");

    // the default module name is the file stem, not a synthetic `mAin_<n>`.
    let geo_name = project_ctx.code_file(fr_geo).module_name();
    assert_eq!(geo_name, "geometry");

    let mut ctx = CompileCtx::initial();
    ctx.create_default_module(fr_geo, &geo_name);
    ctx.create_default_module(fr_main, &project_ctx.code_file(fr_main).module_name());

    let code = Map::from([
        (fr_geo, "def area(w: Int, h: Int): Int := w * h"),
        (fr_main, "def main(): Int := geometry::area(3, 4)"),
    ]);

    assert!(
        compile_hir(code, &mut ctx).is_ok(),
        "items of geometry.sand should be reachable as `geometry::…`"
    );
}
