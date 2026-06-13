//! region variables and `T @ 'r` ascription.
//!
//! This step is structural plumbing (Calculus §1.1, §2.3): lifetime syntax
//! parses and round-trips, `T @ 'r` is a distinct interned type, and
//! monomorphisation erases regions so codegen is unaffected. There are no
//! borrow semantics yet, so region-ascribed values cannot be *created* from
//! literals; they appear only in signatures.

use lang::ir_types::typed_hir::Expression;
use lang::lang::types::Region;
use lang::lang::types::TyKind;

use crate::common::parse;
use crate::common::run_hir;
use crate::common::run_mir_as_expr;
use crate::common::typecheck;
use crate::common::typecheck_fails;

fn run_both(src: &str) -> Expression<'static> {
    let hir = run_hir(src);
    let mir = run_mir_as_expr(src);
    assert_eq!(hir, mir, "HIR and MIR disagree for:\n  {src}");
    hir
}

// ── parsing / structure
// ───────────────────────────────────────────────────────

#[test]
fn region_ascription_parses_as_region_type() {
    let pm = parse("def f<'r>(x: Int @ 'r): Int := 0");
    let pty = pm.functions[0].parameters[0].ty;
    match pty.kind() {
        TyKind::Region(inner, Region::Var(_)) => {
            assert!(matches!(inner.kind(), TyKind::Int));
        }
        other => panic!("expected a region ascription, got {other:?}"),
    }
}

#[test]
fn static_region_parses_as_static() {
    let pm = parse("def f(x: Int @ 'static): Int := 0");
    let pty = pm.functions[0].parameters[0].ty;
    assert!(matches!(pty.kind(), TyKind::Region(_, Region::Static)));
}

// ── type checking
// ─────────────────────────────────────────────────────────────

#[test]
fn region_signature_type_checks() {
    // `x : Int @ 'r` is returned as `Int @ 'r`: same region, so it checks.
    typecheck("def f<'r>(x: Int @ 'r): Int @ 'r := x \n def main(): Int := 0");
}

#[test]
fn static_region_signature_type_checks() {
    typecheck("def f(x: Int @ 'static): Int @ 'static := x \n def main(): Int := 0");
}

#[test]
fn region_on_tuple_type_checks() {
    typecheck("def f<'r>(x: (Int, Bool) @ 'r): Int := 0 \n def main(): Int := 0");
}

#[test]
fn region_ascribed_type_is_distinct_from_bare_type() {
    // `Int @ 'r` is a distinct type from `Int`, so returning the bare inner
    // type does not match.
    typecheck_fails("def f<'r>(x: Int @ 'r): Int := x \n def main(): Int := 0");
}

// ── codegen: regions are erased, so region-typed functions compile and run

#[test]
fn region_typed_function_compiles_and_program_runs() {
    // `helper` carries region ascriptions; monomorphisation erases them to
    // `Int -> Int`, so the program lowers and runs (main never calls helper).
    let src = "def helper<'r>(x: Int @ 'r): Int @ 'r := x \n def main(): Int := 0";
    assert_eq!(run_both(src), Expression::Int(0));
}
