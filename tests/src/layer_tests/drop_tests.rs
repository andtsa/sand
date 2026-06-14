//! Memory Step B — drop / RAII placement (Calculus §6.11).
//!
//! Drops are *observationally inert* until Step C (`__drop_in_place` is a
//! no-op), so these tests assert drop *placement* structurally: they lower a
//! program to MIR and inspect the first-class `Statement::Drop`s. An owned,
//! non-`Copy` binding is dropped at scope exit in reverse declaration order; a
//! value moved out (returned/consumed) or a `Copy` value is not dropped; and a
//! value owned on one `if` branch but moved on the other gets a *completing*
//! drop on the owning branch.

use lang::ir_types::mir::LocalName;
use lang::ir_types::mir::MirProgram;
use lang::ir_types::mir::Statement;

use crate::common::compile_hir;

/// The source names of every variable dropped (via MIR `Statement::Drop`) in
/// the function named `func_name`, in MIR emission order.
fn dropped_vars_in(src: &str, func_name: &str) -> Vec<String> {
    let (ctx, ast) = compile_hir(src).unwrap_or_else(|e| panic!("compile failed: {e}"));
    let mir = MirProgram::from_typed_program(&ast, &ctx);
    let mut names = Vec::new();
    for (fref, func) in mir.functions.iter() {
        if ctx.original_fun_name(*fref) != func_name {
            continue;
        }
        for block in &func.blocks {
            for stmt in &block.statements {
                if let Statement::Drop { place, .. } = stmt {
                    match func.locals[place.local.0].name {
                        LocalName::User(uv) => names.push(ctx.uniq_variable_name(&uv)),
                        LocalName::Temp(i, hint) => names.push(format!("{hint}{i}")),
                    }
                }
            }
        }
    }
    std::mem::forget(ctx);
    names
}

const BOX: &str = "type Box = Mk(Int) \n";

// ── scope-exit drops ──────────────────────────────────────────────────────

#[test]
fn owned_local_is_dropped_at_scope_exit() {
    // `b` owns a non-Copy value never moved out → dropped before the block
    // yields `0`.
    let drops = dropped_vars_in(
        &format!("{BOX} def main(): Int := {{ let b: Box = Box#Mk(1); 0 }}"),
        "main",
    );
    assert_eq!(drops, vec!["b"]);
}

#[test]
fn returned_value_is_not_dropped() {
    // `b` is moved out as the block's value → not dropped.
    let drops = dropped_vars_in(
        &format!(
            "{BOX} def make(): Box := {{ let b: Box = Box#Mk(1); b }} \n \
             def main(): Int := 0"
        ),
        "make",
    );
    assert!(drops.is_empty(), "expected no drops, got {drops:?}");
}

#[test]
fn copy_local_is_not_dropped() {
    // `Int` is `Copy`, so a copy local has no drop.
    let drops = dropped_vars_in("def main(): Int := { let n: Int = 5; 0 }", "main");
    assert!(drops.is_empty(), "expected no drops, got {drops:?}");
}

#[test]
fn locals_drop_in_reverse_declaration_order() {
    let drops = dropped_vars_in(
        &format!(
            "{BOX} def main(): Int := {{ let a: Box = Box#Mk(1); let b: Box = Box#Mk(2); 0 }}"
        ),
        "main",
    );
    assert_eq!(
        drops,
        vec!["b", "a"],
        "drops must be reverse-declaration order"
    );
}

// ── parameters dropped at function exit ───────────────────────────────────

#[test]
fn unused_owned_param_is_dropped_at_function_exit() {
    let drops = dropped_vars_in(
        &format!(
            "{BOX} def consume(b: Box): Int := 0 \n \
             def main(): Int := consume(Box#Mk(1))"
        ),
        "consume",
    );
    // `b` (in `consume`) is owned and never moved → dropped at function exit.
    assert_eq!(drops, vec!["b"]);
}

#[test]
fn moved_param_is_not_dropped() {
    let drops = dropped_vars_in(
        &format!(
            "{BOX} def id(b: Box): Box := b \n \
             def main(): Int := {{ let x: Box = id(Box#Mk(1)); 0 }}"
        ),
        "main",
    );
    // `x` in main is owned → dropped (and `b` is moved out of `id`, not dropped).
    assert_eq!(drops, vec!["x"]);
}

// ── completing drops at branch merges ─────────────────────────────────────

#[test]
fn completing_drop_on_the_branch_that_keeps_the_value() {
    // `b` is moved on the `else` branch (passed to `sink`) but not the `then`
    // branch, so the merge makes it `Moved`; the `then` branch gets a
    // completing drop so `b` is uniformly consumed at the join.
    let drops = dropped_vars_in(
        &format!(
            "{BOX} def sink(b: Box): Int := 0 \n \
             def choose(c: Bool, b: Box): Int := if c then 1 else sink(b) \n \
             def main(): Int := choose(true, Box#Mk(1))"
        ),
        "choose",
    );
    // exactly one completing drop of `b` (on the `then` branch).
    assert_eq!(drops, vec!["b"]);
}

#[test]
fn no_completing_drop_when_both_branches_consume() {
    let drops = dropped_vars_in(
        &format!(
            "{BOX} def sink(b: Box): Int := 0 \n \
             def choose(c: Bool, b: Box): Int := if c then sink(b) else sink(b) \n \
             def main(): Int := choose(true, Box#Mk(1))"
        ),
        "choose",
    );
    // `b` is moved on both branches → consumed everywhere, no drop inserted.
    assert!(drops.is_empty(), "expected no drops, got {drops:?}");
}
