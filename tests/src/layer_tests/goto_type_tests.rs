//! The type/typeclass-reference table that drives LSP go-to-definition on type
//! positions (`CompileCtx::record_type_ref` / `type_ref_at`). The LSP wiring
//! itself lives in `sand-lsp`; here we check the table resolves the right
//! definition for type and typeclass *uses* in signatures / `where` / `impl`.

use lang::castles::project::Project;
use lang::compiler::context::DefTarget;
use lang::compiler::structure::Pos;

/// 1-based (line, col) of the start of the first occurrence of `needle`.
fn pos_of(src: &str, needle: &str) -> Pos {
    let idx = src.find(needle).expect("needle present");
    let prefix = &src[..idx];
    let line = prefix.matches('\n').count() + 1;
    let col = idx - prefix.rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
    Pos { line, col }
}

const SRC: &str = "\
typeclass Show<T> { def show(x: T): Int }
type Box<a> = B(a)
impl Show for Int { def show(x: Int): Int := x }
def f(b: Box<Int>): Int := 0
def g<T>(x: T): Int where T : Show := show(x)
";

#[test]
fn goto_resolves_type_use_in_signature() {
    let mut proj = Project::empty();
    let file = proj.create_virtual_file(SRC.to_string(), "m");
    let (ctx, _ast) = proj.check().result_leaked().expect("compile");

    // `Box<Int>` in `def f(b: Box<Int>)`: the *use*, not the declaration.
    let target = ctx
        .type_ref_at(file, pos_of(SRC, "Box<Int>"))
        .expect("type ref at Box use");
    match target {
        DefTarget::Adt(er) => assert_eq!(ctx.get_enum(er).name, "Box"),
        _ => panic!("expected Adt, got {target:?}"),
    }
    std::mem::forget(ctx);
}

#[test]
fn goto_resolves_typeclass_use_in_where_and_impl() {
    let mut proj = Project::empty();
    let file = proj.create_virtual_file(SRC.to_string(), "m");
    let (ctx, _ast) = proj.check().result_leaked().expect("compile");

    // `Show` in `where T : Show`.
    let in_where = ctx
        .type_ref_at(file, pos_of(SRC, "Show := show"))
        .expect("class ref in where");
    match in_where {
        DefTarget::Typeclass(tref) => assert_eq!(ctx.get_typeclass(tref).name, "Show"),
        _ => panic!("expected Typeclass, got {in_where:?}"),
    }

    // `Show` in `impl Show for Int`.
    let in_impl = ctx
        .type_ref_at(file, pos_of(SRC, "Show for Int"))
        .expect("class ref in impl");
    assert!(matches!(in_impl, DefTarget::Typeclass(_)));
    std::mem::forget(ctx);
}

#[test]
fn goto_at_a_primitive_or_plain_position_is_none() {
    let mut proj = Project::empty();
    let file = proj.create_virtual_file(SRC.to_string(), "m");
    let (ctx, _ast) = proj.check().result_leaked().expect("compile");
    // `Int` is a primitive: no user definition, so not recorded.
    assert!(ctx.type_ref_at(file, pos_of(SRC, "Int>")).is_none());
    std::mem::forget(ctx);
}
