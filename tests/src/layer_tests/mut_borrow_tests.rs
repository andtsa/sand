//! Exclusive (mutable) borrows: the `BorrowedMut` kind, `&'r mut T`
//! reference types, `&mut e` borrow expressions, and `let &mut x = e` bindings
//! (Calculus: Kinds, Types, Terms).
//!
//! This layer is structural: mutable borrows parse, type-check, and (like
//! shared borrows) are erased by monomorphisation, so they lower transparently.
//! The exclusivity invariant is enforced by the ownership pass.

use lang::ir_types::typed_hir::Expression;
use lang::lang::types::Kind;

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

// --- kind lattice: Owned <: BorrowedMut, incomparable to Borrowed// --- ---
#[test]
fn owned_is_subkind_of_borrowed_mut() {
    assert!(Kind::Owned.is_subkind(Kind::BorrowedMut));
    assert!(Kind::Never.is_subkind(Kind::BorrowedMut));
    assert!(!Kind::BorrowedMut.is_subkind(Kind::Owned));
}

#[test]
fn borrow_modes_are_incomparable() {
    // Borrowed and BorrowedMut are distinct, mutually-incomparable branches.
    assert!(!Kind::Borrowed.is_subkind(Kind::BorrowedMut));
    assert!(!Kind::BorrowedMut.is_subkind(Kind::Borrowed));
}

#[test]
fn borrow_modes_join_to_owned() {
    assert_eq!(Kind::Borrowed.join(Kind::BorrowedMut), Kind::Owned);
    assert_eq!(Kind::BorrowedMut.join(Kind::BorrowedMut), Kind::BorrowedMut);
}

// --- reference types and borrow expressions parse and type-check// --- ---
#[test]
fn mut_reference_type_parses() {
    parse("def f(r: &mut Int): Int := 0");
}

#[test]
fn explicit_lifetime_mut_reference_parses() {
    parse("def f<'r>(r: &'r mut Int): Int := 0");
}

#[test]
fn mut_borrow_expression_type_checks() {
    typecheck("def f(mut x: Int): Int := { let r = &mut x; x } \n def main(): Int := 0");
}

#[test]
fn passing_a_mut_borrow_to_a_mut_reference_parameter_type_checks() {
    typecheck("def takes(r: &mut Int): Int := 0 \n def main(): Int := takes(&mut 5)");
}

// --- mutable borrows do not consume their referent// --- ---
#[test]
fn mut_borrowing_does_not_move_a_non_copy_value() {
    // `&mut e` borrows `e` without moving it; once the borrow's (inner-block)
    // scope ends, `e` is still owned and may be consumed (a value may not be
    // moved while borrowed).
    typecheck(
        "type E = A | B \n \
         def f(mut e: E): Int := { { let r = &mut e; 0 }; match e { E#A => 1, E#B => 2 } } \n \
         def main(): Int := 0",
    );
}

// --- `let &mut x` binding (desugars to `let x : &mut T = &mut e`)// --- ---
#[test]
fn let_mut_borrow_binding_type_checks() {
    typecheck("def f(mut x: Int): Int := { let &mut r = x; x } \n def main(): Int := 0");
}

// --- mutable borrows compile and run (erased transparently)// --- ---
#[test]
fn mut_borrow_program_runs() {
    assert_eq!(
        run_both("def takes(r: &mut Int): Int := 0 \n def main(): Int := takes(&mut 7)"),
        Expression::Int(0)
    );
}

#[test]
fn mut_borrowed_value_still_usable_at_runtime() {
    assert_eq!(
        run_both("def f(mut x: Int): Int := { let r = &mut x; x } \n def main(): Int := f(42)"),
        Expression::Int(42)
    );
}

// --- escape check applies to mutable borrows too// --- ---
#[test]
fn returning_a_mut_borrow_of_a_local_is_rejected() {
    typecheck_fails("def f(): &mut Int := { let mut y = 5; &mut y } \n def main(): Int := 0");
}

#[test]
fn returning_a_mut_borrow_of_a_by_value_parameter_is_rejected() {
    // a by-value parameter lives in the frame, so a `&mut` of it would dangle
    // when the call returns (Calculus: The Escape Check). A `&'a mut` tied to
    // a lifetime parameter is returnable.
    typecheck_fails("def f(mut x: Int): &mut Int := { &mut x } \n def main(): Int := 0");
}

// --- the exclusivity invariant// --- ---
#[test]
fn a_single_mut_borrow_is_accepted() {
    typecheck("def f(mut x: Int): Int := { let a = &mut x; 0 } \n def main(): Int := 0");
}

#[test]
fn two_mut_borrows_of_the_same_place_conflict() {
    // both `a` and `b` are used in the tail, so the loans overlap (NLL).
    typecheck_fails(
        "def f(mut x: Int): Int := { let a = &mut x; let b = &mut x; *a + *b } \n \
         def main(): Int := 0",
    );
}

#[test]
fn a_mut_borrow_after_a_live_shared_borrow_conflicts() {
    typecheck_fails(
        "def f(mut x: Int): Int := { let a = &x; let b = &mut x; *a + *b } \n \
         def main(): Int := 0",
    );
}

#[test]
fn a_shared_borrow_after_a_live_mut_borrow_conflicts() {
    typecheck_fails(
        "def f(mut x: Int): Int := { let a = &mut x; let b = &x; *a + *b } \n \
         def main(): Int := 0",
    );
}

// non-lexical lifetimes: a loan ends at its holder's last use, not block end
#[test]
fn nll_sequential_mut_borrows_are_accepted() {
    // `a`'s last use precedes `b`, so the two `&mut` loans never overlap.
    typecheck(
        "def f(mut x: Int): Int := { let a = &mut x; let _ = *a; let b = &mut x; *b } \n \
         def main(): Int := 0",
    );
}

#[test]
fn nll_mut_after_a_finished_shared_borrow_is_accepted() {
    typecheck(
        "def f(mut x: Int): Int := { let a = &x; let _ = *a; let b = &mut x; *b } \n \
         def main(): Int := 0",
    );
}

#[test]
fn aliased_mut_borrow_keeps_the_loan_live() {
    // PROBE for the holder-aliasing soundness hole. `a = &mut x`; `let b = a`
    // *moves* the reference (`&mut` is not Copy) so `b` now aliases `x`. The loan
    // on `x` records its holder as `a`, whose last use is `let b = a`, so a later
    // `&mut x` (`c`) prunes the loan and is admitted, yet `*b` and `*c` then both
    // read through *live* exclusive borrows of `x`. This MUST be rejected; if the
    // compile succeeds, the loan was released at the wrong holder's last use.
    typecheck_fails(
        "def f(mut x: Int): Int := { let a = &mut x; let b = a; let c = &mut x; *b + *c } \n \
         def main(): Int := 0",
    );
}

#[test]
fn borrow_escaping_into_a_tuple_keeps_the_loan_live() {
    // `h` flows into a tuple (a non-deref use), so it escapes: the loan on `x`
    // stays live and the second `&mut x` conflicts.
    typecheck_fails(
        "def f(mut x: Int): Int := { let h = &mut x; let _p = (h, 0); let c = &mut x; *c } \n \
         def main(): Int := 0",
    );
}

#[test]
fn borrow_stored_through_a_pointer_keeps_the_loan_live() {
    // `h` stored through `*p` (as the written *value*, not the deref target) is a
    // non-deref use, so it escapes and the loan stays live.
    typecheck_fails(
        "def f(mut x: Int): Int := \
           { let h = &x; let mut q = &x; let p = &mut q; *p = h; let c = &mut x; *c } \n \
         def main(): Int := 0",
    );
}

#[test]
fn reborrow_through_a_deref_keeps_the_loan_live() {
    // `g = &(*h)` reborrows through `h`, aliasing `x`. The reborrow is a
    // non-extracting use that nonetheless creates a live alias, so `h` must
    // escape and the loan stay live, so the later `&mut x` then conflicts. (An
    // earlier deref-only rule wrongly pruned `h` here, admitting `g` and `c`
    // as simultaneous borrows of `x`.)
    typecheck_fails(
        "def f(mut x: Int): Int := { let h = &mut x; let g = &(*h); let c = &mut x; *g + *c } \n \
         def main(): Int := 0",
    );
}

#[test]
fn conflicting_borrows_inside_a_loop_still_conflict() {
    // liveness widens loop-body uses to the loop's end, so overlapping loans
    // inside a loop are still caught (soundness of the loop path).
    typecheck_fails(
        "def f(mut x: Int): Int := \
           { let c = false; while c { let a = &mut x; let b = &mut x; let _ = *a + *b; }; 0 } \n \
         def main(): Int := 0",
    );
}

#[test]
fn two_shared_borrows_coexist() {
    typecheck("def f(x: Int): Int := { let a = &x; let b = &x; 0 } \n def main(): Int := 0");
}

#[test]
fn mut_borrow_of_an_immutable_variable_is_rejected() {
    // `x` is not declared `mut`, so it cannot be borrowed exclusively.
    typecheck_fails("def f(x: Int): Int := { let a = &mut x; 0 } \n def main(): Int := 0");
}

#[test]
fn a_borrow_is_released_at_the_end_of_its_block() {
    // the first `&mut x` lives only for the inner block, so the second is fine.
    typecheck(
        "def f(mut x: Int): Int := { { let a = &mut x; 0 }; let b = &mut x; 0 } \n \
         def main(): Int := 0",
    );
}

// --- R3: write-through (`*r = e`)// --- ---
#[test]
fn write_through_a_mut_reference_type_checks() {
    // `*r = e` stores through a `&mut`. (Observable mutation is validated via LLVM
    // in `examples/write_through.sand`.)
    typecheck(
        "def incr(r: &mut Int): Unit := { *r = *r + 1; } \n \
         def main(): Int := { let mut x = 5; incr(&mut x); x }",
    );
}

#[test]
fn write_through_a_shared_reference_is_rejected() {
    // write-through requires `&mut`; writing through a shared `&T` is a type error.
    typecheck_fails(
        "def bad(r: &Int): Unit := { *r = 7; } \n \
         def main(): Int := 0",
    );
}

// --- R4: write-through is observable in both interpreters (cell-graph store)//
// --- ---
#[test]
fn write_through_mutates_the_callers_variable() {
    // `incr` writes through a `&mut Int` it received; the mutation lands in the
    // caller's `x` (5 -> 6). This is the interpreter counterpart of
    // `examples/write_through.sand`, which validates the same via LLVM. `run_both`
    // asserts the HIR and MIR interpreters agree.
    assert_eq!(
        run_both(
            "def incr(r: &mut Int): Unit := { *r = *r + 1; } \n \
             def main(): Int := { let mut x = 5; incr(&mut x); x }"
        ),
        Expression::Int(6)
    );
}

#[test]
fn write_through_a_local_mut_reference_is_observable() {
    // a `&mut` taken and written within the same function still threads through
    // shared storage: `*r = 9` updates `x`, read back as the block's result.
    assert_eq!(
        run_both("def main(): Int := { let mut x = 1; let r = &mut x; *r = 9; x }"),
        Expression::Int(9)
    );
}

#[test]
fn repeated_write_through_accumulates() {
    // two calls through the same `&mut` storage accumulate: 5 -> 6 -> 7. Each
    // borrow is scoped to its own block so it is released before the next (the
    // borrow checker releases `&mut` at block end, not after the call).
    assert_eq!(
        run_both(
            "def incr(r: &mut Int): Unit := { *r = *r + 1; } \n \
             def main(): Int := { let mut x = 5; { incr(&mut x); }; { incr(&mut x); }; x }"
        ),
        Expression::Int(7)
    );
}
