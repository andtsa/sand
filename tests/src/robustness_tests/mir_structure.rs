//! Tests for MIR structural invariants
//!
//! Verifies that the MIR lowering produces valid IR structures with correct
//! function/block counts, terminators, parameter-to-local mappings, and
//! let-binding local creation.

use lang::compile_hir;
use lang::compiler::context::CompileCtx;
use lang::compiler::structure::Map;
use lang::ir_types::mir::MirProgram;
use lang::ir_types::mir::Terminator;

fn lower(src: &str) -> (MirProgram, CompileCtx<'static>) {
    let mut ctx = CompileCtx::initial();
    let fr = ctx.stub_file();
    let code = Map::from([(fr, src)]);
    let ast = compile_hir(code, &mut ctx).unwrap_or_else(|e| panic!("compile failed:\n  {e}"));
    (MirProgram::from_typed_program(&ast), ctx)
}

#[test]
fn function_count_matches_source() {
    let (mir, _ctx) = lower("def main(): Int := 42");
    assert_eq!(mir.functions.len(), 1);

    let (mir, _ctx) = lower(
        "def helper(): Int := 1
         def main(): Int := helper()",
    );
    assert_eq!(mir.functions.len(), 2);

    let (mir, _ctx) = lower(
        "def a(): Int := 1
         def b(): Int := 2
         def c(): Int := 3
         def main(): Int := a()",
    );
    assert_eq!(mir.functions.len(), 4);
}

#[test]
fn simple_literal_has_at_least_one_block() {
    let (mir, ctx) = lower("def main(): Int := 99");
    for func in mir.functions.values() {
        assert!(
            !func.blocks.is_empty(),
            "function {} has no blocks",
            ctx.original_fun_name(func.name)
        );
    }
}

#[test]
fn all_blocks_have_a_terminator() {
    let cases = [
        "def main(): Int := 1 + 2",
        "def main(): Bool := if true then false else true",
        "def main(): Int := {
            let i: Int = 0;
            while i < 5 do { i = i + 1; };
            i
        }",
        "def f(x: Int): Int := x * 2
         def main(): Int := f(21)",
    ];
    for src in cases {
        let (mir, _ctx) = lower(src);
        for func in mir.functions.values() {
            for block in &func.blocks {
                assert!(
                    !matches!(block.terminator, Terminator::Unreachable),
                    "block {} in function {:?} has Unreachable terminator for:\n  {src}",
                    block.id.0,
                    func.name
                );
            }
        }
    }
}

#[test]
fn parameters_have_corresponding_locals() {
    let (mir, _ctx) = lower(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(1, 2)",
    );
    for func in mir.functions.values() {
        for param in &func.params {
            assert!(
                func.locals.iter().any(|l| l.id == param.local),
                "param local {:?} not found in locals for {:?}",
                param.local,
                func.name
            );
        }
    }
}

#[test]
fn if_expression_produces_branch_terminator() {
    let (mir, _ctx) = lower(
        "def main(): Int := {
        let b: Bool = true;
        if b then 1 else 2
    }",
    );
    let func = mir.functions.values().next().unwrap();
    let has_branch = func
        .blocks
        .iter()
        .any(|b| matches!(b.terminator, Terminator::Branch { .. }));
    assert!(
        has_branch,
        "expected at least one Branch terminator for if expression"
    );
}

#[test]
fn while_loop_produces_branch_and_goto() {
    let src = "def main(): Int := {
        let i: Int = 0;
        while i < 3 do { i = i + 1; };
        i
    }";
    let (mir, _ctx) = lower(src);
    let func = mir.functions.values().next().unwrap();
    let has_branch = func
        .blocks
        .iter()
        .any(|b| matches!(b.terminator, Terminator::Branch { .. }));
    let has_goto = func
        .blocks
        .iter()
        .any(|b| matches!(b.terminator, Terminator::Goto { .. }));
    assert!(has_branch, "while loop should produce a Branch terminator");
    assert!(
        has_goto,
        "while loop should produce a Goto (back-edge) terminator"
    );
}

#[test]
fn no_function_has_zero_locals_when_it_has_params() {
    let (mir, _ctx) = lower(
        "def f(x: Int, y: Bool): Int := 0
         def main(): Int := f(1, true)",
    );
    for func in mir.functions.values() {
        if !func.params.is_empty() {
            assert!(
                !func.locals.is_empty(),
                "function {:?} has params but no locals",
                func.name
            );
        }
    }
}

#[test]
fn let_bindings_produce_locals() {
    let src = "def main(): Int := {
        let a: Int = 1;
        let b: Int = 2;
        a + b
    }";
    let (mir, _ctx) = lower(src);
    let func = mir.functions.values().next().unwrap();
    let user_locals = func
        .locals
        .iter()
        .filter(|l| matches!(l.name, lang::ir_types::mir::LocalName::User(_)))
        .count();
    assert!(
        user_locals >= 2,
        "expected at least 2 user locals, got {user_locals}"
    );
}
