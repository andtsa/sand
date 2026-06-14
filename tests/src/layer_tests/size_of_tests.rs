//! Memory Step C.2 — `size_of::<T>()` and turbofish.
//!
//! `size_of` takes an explicit type argument (turbofish) and returns the byte
//! size of that type. Codegen emits the real, target-dependent LLVM size; the
//! interpreters use a layout-free approximation that happens to match for the
//! primitive/tuple cases tested here. Turbofish is wired only for `size_of` in
//! Step C; on any other call it is a clear error.

use lang::ir_types::typed_hir::Expression;

use crate::common::run_hir_and_mir;
use crate::common::typecheck;
use crate::common::typecheck_fails;

fn run_both(src: &str) -> Expression<'static> {
    let (hir, mir) = run_hir_and_mir(src);
    assert_eq!(hir, mir, "HIR and MIR disagree for:\n  {src}");
    hir
}

#[test]
fn size_of_int_typechecks() {
    let (ctx, _p) = typecheck("def main(): Int := size_of::<Int>()");
    std::mem::forget(ctx);
}

#[test]
fn size_of_int_is_eight() {
    assert_eq!(
        run_both("def main(): Int := size_of::<Int>()"),
        Expression::Int(8)
    );
}

#[test]
fn size_of_unit_is_zero() {
    assert_eq!(
        run_both("def main(): Int := size_of::<Unit>()"),
        Expression::Int(0)
    );
}

#[test]
fn size_of_tuple_sums_fields() {
    assert_eq!(
        run_both("def main(): Int := size_of::<(Int, Int)>()"),
        Expression::Int(16)
    );
}

#[test]
fn size_of_used_in_arithmetic() {
    assert_eq!(
        run_both("def main(): Int := size_of::<Int>() + size_of::<Bool>()"),
        Expression::Int(9)
    );
}

// ── turbofish is restricted to `size_of` in Step C ───────────────────────

#[test]
fn turbofish_on_regular_function_is_rejected() {
    typecheck_fails("def id<T>(x: T): T := x \n def main(): Int := id::<Int>(5)");
}

#[test]
fn size_of_without_turbofish_is_rejected() {
    // `size_of` requires a type argument.
    typecheck_fails("def main(): Int := size_of()");
}
