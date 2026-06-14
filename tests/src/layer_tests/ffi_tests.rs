//! Memory Step A — minimal FFI (`extern def`) and the `Ptr<T>` substrate.
//!
//! An `extern def` declares a bodyless C-ABI function bound to a symbol of the
//! same name. It is registered with a real signature so calls resolve through
//! the normal path, but carries no body in any IR: the front end type-checks
//! the call against the declared signature, monomorphisation passes the callee
//! through unchanged, and codegen declares the C symbol. Boundary types must be
//! FFI-safe (`Int`, `Unit`, `Ptr<T>`).
//!
//! Runtime behaviour (actually calling `malloc`/`free`) arrives in A.3 with the
//! simulated-heap interpreter dispatch, so these tests stop at compile/lower.

use lang::ir_types::mir::MirProgram;
use lang::ir_types::typed_hir::Expression;

use crate::common::compile_hir;
use crate::common::run_hir_and_mir;
use crate::common::typecheck;
use crate::common::typecheck_fails;

fn run_both(src: &str) -> Expression<'static> {
    let (hir, mir) = run_hir_and_mir(src);
    assert_eq!(hir, mir, "HIR and MIR disagree for:\n  {src}");
    hir
}

// ── declaration + call resolution ────────────────────────────────────────

#[test]
fn extern_decl_and_call_typecheck() {
    // The extern resolves like an ordinary function; the call checks against
    // its declared signature (`Int -> Ptr<Unit>`).
    let (ctx, _prog) = typecheck(
        "extern def malloc(size: Int): Ptr<Unit>; \n \
         def grab(): Ptr<Unit> := malloc(8) \n \
         def main(): Int := 0",
    );
    std::mem::forget(ctx);
}

#[test]
fn extern_ptr_roundtrips_as_a_value() {
    // A `Ptr<Unit>` flows as an ordinary (Copy) value through a binding.
    let (ctx, _prog) = typecheck(
        "extern def malloc(size: Int): Ptr<Unit>; \n \
         def main(): Int := { let p: Ptr<Unit> = malloc(8); 0 }",
    );
    std::mem::forget(ctx);
}

// ── monomorphisation passes extern callees through (the mono guard) ───────

#[test]
fn reachable_extern_call_lowers_to_mir() {
    // `malloc` is reachable from `main`, so monomorphisation visits the call.
    // The extern guard must return the callee unchanged (it has no body to
    // specialise) and explication must emit the call. We lower to MIR but do
    // not interpret — running `malloc` is A.3.
    let (ctx, ast) = compile_hir(
        "extern def malloc(size: Int): Ptr<Unit>; \n \
         def main(): Int := { let p: Ptr<Unit> = malloc(8); 0 }",
    )
    .expect("compile failed");
    let _mir = MirProgram::from_typed_program(&ast, &ctx);
    std::mem::forget(ctx);
}

// ── end-to-end: alloc / write / read / free round-trip ───────────────────

#[test]
fn alloc_write_read_free_roundtrips() {
    // The Step A acceptance shape: allocate a cell, cast the raw pointer to a
    // typed one, store a value, read it back, and free. Both interpreters must
    // agree on the recovered value.
    assert_eq!(
        run_both(
            "extern def malloc(size: Int): Ptr<Unit>; \n \
             extern def free(p: Ptr<Unit>): Unit; \n \
             def main(): Int := { \n \
               let raw: Ptr<Unit> = malloc(8); \n \
               let p: Ptr<Int> = __ptr_cast(raw); \n \
               __ptr_write(p, 42); \n \
               let x: Int = __ptr_read(p); \n \
               free(raw); \n \
               x \n \
             }"
        ),
        Expression::Int(42)
    );
}

#[test]
fn ptr_write_then_read_returns_stored_value() {
    assert_eq!(
        run_both(
            "extern def malloc(size: Int): Ptr<Unit>; \n \
             def store_load(n: Int): Int := { \n \
               let p: Ptr<Int> = __ptr_cast(malloc(8)); \n \
               __ptr_write(p, n); \n \
               __ptr_read(p) \n \
             } \n \
             def main(): Int := store_load(7) + store_load(35)"
        ),
        Expression::Int(42)
    );
}

// ── drop_in_place (no-op substrate, Step A) ──────────────────────────────

#[test]
fn drop_in_place_is_a_noop() {
    // `__drop_in_place` accepts any type and yields unit; it is inert until
    // types acquire destructors in Step C. Here it consumes an `Int`.
    assert_eq!(
        run_both("def main(): Int := { let n: Int = 42; __drop_in_place(n); n }"),
        Expression::Int(42)
    );
}

// ── FFI-safety of boundary types ─────────────────────────────────────────

#[test]
fn non_ffi_safe_param_is_rejected() {
    // A user enum may not cross the C boundary.
    typecheck_fails(
        "type Color = Red | Green \n \
         extern def paint(c: Color): Int; \n \
         def main(): Int := 0",
    );
}

#[test]
fn non_ffi_safe_return_is_rejected() {
    typecheck_fails(
        "type Color = Red | Green \n \
         extern def make(): Color; \n \
         def main(): Int := 0",
    );
}
