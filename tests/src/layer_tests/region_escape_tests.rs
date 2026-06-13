//! Step 8b — borrow escape check and the outlives solver (Calculus §1.1, §6.3).
//!
//! A block opens a lexical region scope; a borrow of a value bound inside the
//! block lives in that scope and may not be yielded out of it (a dangling
//! borrow). Borrows of parameters — which outlive the call — may be returned.
//! Region types are still erased by monomorphisation; the check is purely
//! static. The outlives relation (`'r ≥ 's`) is exercised directly.

use lang::lang::types::Region;
use lang::lang::types::RegionConstraint;

use crate::common::typecheck;
use crate::common::typecheck_fails;

// ── the escape check fires on borrows of locals
// ───────────────────────────────

#[test]
fn returning_a_borrow_of_a_local_is_rejected() {
    // `y` lives only for the block; `&y` would dangle once the block ends.
    typecheck_fails("def f(): &Int := { let y = 5; &y } \n def main(): Int := 0");
}

#[test]
fn returning_a_let_bound_borrow_of_a_local_is_rejected() {
    // the borrow is laundered through a binding, but still escapes.
    typecheck_fails("def f(): &Int := { let y = 5; let r = &y; r } \n def main(): Int := 0");
}

#[test]
fn returning_a_borrow_from_a_nested_block_is_rejected() {
    // `y` is bound in the outer block; the inner block's `&y` survives the inner
    // block but escapes the outer one when yielded.
    typecheck_fails("def f(): &Int := { let y = 5; { &y } } \n def main(): Int := 0");
}

// ── return-escape: a borrow of a by-value parameter or a local is rejected at
//    the frame boundary; a borrow tied to a *lifetime parameter* (`&'a`) is
//    returnable (see `region_inference_tests::id_ref` / `longest`) ────────────

#[test]
fn returning_a_borrow_of_a_by_value_parameter_is_rejected() {
    // a by-value parameter lives in the frame and is dropped when the call
    // returns, so a borrow of it would dangle (Calculus §6.3, frame boundary).
    typecheck_fails("def f(x: Int): &Int := { &x } \n def main(): Int := 0");
}

#[test]
fn returning_a_borrow_of_a_by_value_parameter_without_a_block_is_rejected() {
    typecheck_fails("def f(x: Int): &Int := &x \n def main(): Int := 0");
}

#[test]
fn returning_a_let_bound_borrow_of_a_by_value_parameter_is_rejected() {
    typecheck_fails("def f(x: Int): &Int := { let r = &x; r } \n def main(): Int := 0");
}

#[test]
fn borrowing_a_local_without_yielding_it_is_accepted() {
    // the borrow is used inside the block and not returned — no escape.
    typecheck("def f(): Int := { let y = 5; let r = &y; 0 } \n def main(): Int := 0");
}

// ── escape through a branch join (if / match)
// ─────────────────────────────────
//
// A branch join takes the *meet* of its branches' regions, so a borrow of a
// local escaping through *any* branch (not just the first/chosen one) surfaces
// in the result type and is caught. Taking one branch verbatim (the first arm,
// or the region-blind expected type) used to drop the other branches' regions.

#[test]
fn escape_through_a_match_arm_is_rejected() {
    // the `false` arm yields `&x`, a borrow of a local; it escapes via the
    // function's `&Int` return even though the `true` arm is a safe param borrow.
    typecheck_fails(
        "def pick(c: Bool, p: &Int): &Int := { let x = 5; match c { true => p, false => &x } } \n \
         def main(): Int := 0",
    );
}

#[test]
fn escape_through_an_if_branch_is_rejected() {
    // same hole via `if`: the `else` branch borrows a local.
    typecheck_fails(
        "def pick(c: Bool, p: &Int): &Int := { let x = 5; if c then p else &x } \n \
         def main(): Int := 0",
    );
}

#[test]
fn a_joined_borrow_used_in_scope_is_accepted() {
    // both branches borrow locals, but the joined borrow is consumed within the
    // same block (never yielded), so there is no escape.
    typecheck(
        "def f(): Int := { let mut a = 1; let mut b = 2; let r = if true then &a else &b; 0 } \n \
         def main(): Int := 0",
    );
}

#[test]
fn a_same_lifetime_parametric_branch_return_is_accepted() {
    // both arms borrow at the same lifetime parameter `'r`, so their meet is `'r`
    // (not a local scope), and returning the join is sound.
    typecheck(
        "def choose<'r>(c: Bool, p: &'r Int, q: &'r Int): &'r Int := if c then p else q \n \
         def main(): Int := 0",
    );
}

#[test]
fn a_distinct_lifetime_branch_return_justified_by_a_where_is_accepted() {
    // distinct lifetimes, but `'a >= 'b` lets the `'a` arm coerce to `'b`, so the
    // join meets to `'b` (a parameter, not a local) and the return is sound.
    typecheck(
        "def choose<'a, 'b>(c: Bool, x: &'a Int, y: &'b Int): &'b Int where 'a >= 'b := if c then x else y \n \
         def main(): Int := 0",
    );
}

#[test]
fn a_distinct_lifetime_branch_return_without_a_where_is_rejected() {
    // with no constraint relating 'a and 'b, the join collapses to a local region
    // and the return escapes.
    typecheck_fails(
        "def choose<'a, 'b>(c: Bool, x: &'a Int, y: &'b Int): &'b Int := if c then x else y \n \
         def main(): Int := 0",
    );
}

// ── the outlives solver: `'r ≥ 's` (Calculus §1.1)
// ────────────────────────────
//
// These exercise `outlives` directly. Regions are allocated *through* the
// context (`enter_region_scope`) rather than fabricated, so their identities
// and depths are real and cannot collide with scopes opened during checking.

/// Allocate `n` distinct sibling regions (each opened and immediately closed,
/// so all share the same nesting depth and have no outlives relationship by
/// nesting — they relate only through explicit assumptions).
fn siblings(ctx: &mut lang::compiler::context::CompileCtx<'static>, n: usize) -> Vec<Region> {
    (0..n)
        .map(|_| {
            let r = ctx.enter_region_scope();
            ctx.exit_region_scope();
            r
        })
        .collect()
}

#[test]
fn outlives_is_reflexive_and_static_is_greatest() {
    let (mut ctx, _) = typecheck("def main(): Int := 0");
    let r = siblings(&mut ctx, 1)[0];
    // every region outlives itself
    assert!(ctx.outlives(r, r, &[]));
    // 'static outlives everything …
    assert!(ctx.outlives(Region::Static, r, &[]));
    // … and nothing (else) outlives 'static
    assert!(!ctx.outlives(r, Region::Static, &[]));
}

#[test]
fn outlives_follows_lexical_nesting() {
    let (mut ctx, _) = typecheck("def main(): Int := 0");
    // open two genuinely nested scopes: `outer` encloses `inner`.
    let outer = ctx.enter_region_scope();
    let inner = ctx.enter_region_scope();
    // a shallower scope outlives a deeper one, but not vice versa.
    assert!(ctx.outlives(outer, inner, &[]));
    assert!(!ctx.outlives(inner, outer, &[]));
    ctx.exit_region_scope();
    ctx.exit_region_scope();
}

#[test]
fn outlives_respects_and_closes_over_assumptions() {
    let (mut ctx, _) = typecheck("def main(): Int := 0");
    let r = siblings(&mut ctx, 3); // siblings: no nesting relationship
    let (r0, r1, r2) = (r[0], r[1], r[2]);
    let asm = [
        RegionConstraint {
            longer: r0,
            shorter: r1,
        },
        RegionConstraint {
            longer: r1,
            shorter: r2,
        },
    ];
    // direct edge
    assert!(ctx.outlives(r0, r1, &asm));
    // transitive closure: r0 ≥ r1 ≥ r2 ⟹ r0 ≥ r2
    assert!(ctx.outlives(r0, r2, &asm));
    // the relation is directional: r1 does not outlive r0
    assert!(!ctx.outlives(r1, r0, &asm));
    // without the assumptions, unrelated siblings do not outlive each other
    assert!(!ctx.outlives(r0, r1, &[]));
}

#[test]
fn satisfies_outlives_checks_a_constraint_set() {
    let (mut ctx, _) = typecheck("def main(): Int := 0");
    let r = siblings(&mut ctx, 2);
    let required = [RegionConstraint {
        longer: r[0],
        shorter: r[1],
    }];
    let assumptions = [RegionConstraint {
        longer: r[0],
        shorter: r[1],
    }];
    assert!(ctx.satisfies_outlives(&required, &assumptions));
    assert!(!ctx.satisfies_outlives(&required, &[]));
}

// ── assignment reseat-escape (item 11) ───────────────────────────────────────

#[test]
fn reseating_an_outer_reference_to_an_inner_borrow_is_rejected() {
    // re-pointing an outer reference at a borrow from an inner block would dangle
    // once the inner block closes (Calculus §6.3, item 11): the assignment's RHS
    // region must outlive the variable it is assigned into.
    typecheck_fails(
        "def f(): Int := { let a = 1; let mut o = &a; { let i = 2; o = &i; 0 }; *o } \n \
         def main(): Int := 0",
    );
}

#[test]
fn reseating_a_reference_within_the_same_scope_is_accepted() {
    // re-pointing a reference at another borrow from the *same* scope is fine —
    // both live equally long.
    typecheck(
        "def g(): Int := { let a = 1; let b = 2; let mut o = &a; o = &b; *o } \n \
         def main(): Int := 0",
    );
}
