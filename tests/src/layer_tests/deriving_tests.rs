//! Memory Step C.1 — `deriving` + recursive-type legality (K-HeapedRec).
//!
//! A (mutually) recursive `type` must `deriving Heaped` (else its values
//! would be infinite-sized and leak); a non-recursive type must not; and only
//! registered derivables are accepted.

use lang::ir_types::typed_hir::Expression;

use crate::common::run_hir_and_mir;
use crate::common::typecheck;
use crate::common::typecheck_fails;

#[test]
fn recursive_type_without_deriving_is_rejected() {
    typecheck_fails("type L = E | C((Int, L)) \n def main(): Int := 0");
}

#[test]
fn recursive_type_with_deriving_is_accepted() {
    let (ctx, _p) = typecheck("type L = E | C((Int, L)) deriving Heaped \n def main(): Int := 0");
    std::mem::forget(ctx);
}

#[test]
fn generic_recursive_type_with_deriving_is_accepted() {
    let (ctx, _p) = typecheck(
        "type List<T> = Empty | Cons((T, List<T>)) deriving Heaped \n \
         def main(): Int := 0",
    );
    std::mem::forget(ctx);
}

#[test]
fn non_recursive_type_may_derive_heaped() {
    // A non-recursive type may opt onto the heap (e.g. a large payload) — it is
    // allowed, not required.
    let (ctx, _p) =
        typecheck("type Big = One((Int, Int, Int)) | Two deriving Heaped \n def main(): Int := 0");
    std::mem::forget(ctx);
}

#[test]
fn unknown_derivable_is_rejected() {
    typecheck_fails("type P = A | B deriving Frobnicate \n def main(): Int := 0");
}

#[test]
fn mutually_recursive_types_require_deriving() {
    // A → B → A: both are recursive, so both must derive (omitting it fails).
    typecheck_fails("type A = MkA(B) \n type B = MkB(A) \n def main(): Int := 0");
}

#[test]
fn mutually_recursive_types_with_deriving_are_accepted() {
    let (ctx, _p) = typecheck(
        "type A = MkA(B) deriving Heaped \n \
         type B = MkB(A) deriving Heaped \n \
         def main(): Int := 0",
    );
    std::mem::forget(ctx);
}

// ── C.4: the `Unique<T>` strategy functions (core-lib, over `Ptr`) ──────────
//
// `unique_alloc` moves a value onto the heap and `unique_release` drops it,
// using only the Step-A `Ptr`/FFI substrate (real `malloc`/`free` under
// codegen; cell-graph allocations under the interpreters). These are the sole
// allocation/deallocation sites that `deriving Heaped` lowers onto (C.5).

fn run_both(src: &str) -> Expression<'static> {
    let (hir, mir) = run_hir_and_mir(src);
    assert_eq!(hir, mir, "HIR and MIR disagree for:\n  {src}");
    hir
}

#[test]
fn unique_alloc_typechecks() {
    let (ctx, _p) = typecheck("def main(): Int := { let h: Unique<Int> = unique_alloc(7); 0 }");
    std::mem::forget(ctx);
}

#[test]
fn unique_alloc_then_release_runs() {
    // Allocate a handle and release it; the program completes and returns 99.
    assert_eq!(
        run_both(
            "def main(): Int := { \n \
               let h: Unique<Int> = unique_alloc(42); \n \
               unique_release(h); \n \
               99 \n \
             }",
        ),
        Expression::Int(99)
    );
}

#[test]
fn unique_handles_are_independent() {
    // Two distinct allocations release independently.
    assert_eq!(
        run_both(
            "def main(): Int := { \n \
               let h: Unique<Int> = unique_alloc(42); \n \
               let v: Unique<Int> = unique_alloc(7); \n \
               unique_release(h); \n \
               unique_release(v); \n \
               0 \n \
             }",
        ),
        Expression::Int(0)
    );
}

// ── C.5: heap lowering — recursive `deriving Heaped` enums end to end ────────
//
// After lowering, a recursive enum is a `Unique<Node>` handle: construction
// allocates, a consuming `match` takes the node and frees the husk, and a value
// that goes out of scope unconsumed is deep-released. The interpreters reclaim
// via `Rc` and codegen via real `free`, so observable values agree.

#[test]
fn recursive_list_builds_and_sums() {
    assert_eq!(
        run_both(
            "type Nums = Nil | Cons((Int, Nums)) deriving Heaped \n \
             def sum(l: Nums): Int := match l { \n \
                 Nums#Nil => 0, \n \
                 Nums#Cons((h, t)) => h + sum(t) \n \
             } \n \
             def main(): Int := \n \
                 sum(Nums#Cons((1, Nums#Cons((2, Nums#Cons((3, Nums#Nil)))))))",
        ),
        Expression::Int(6)
    );
}

#[test]
fn recursive_tree_depth() {
    // A multi-field, multi-variant heaped enum: build a small tree and measure
    // its depth (exercises the node switch in both match-take and drop glue).
    assert_eq!(
        run_both(
            "type Tree = Leaf | Node((Tree, Int, Tree)) deriving Heaped \n \
             def depth(t: Tree): Int := match t { \n \
                 Tree#Leaf => 0, \n \
                 Tree#Node((l, v, r)) => 1 + max(depth(l), depth(r)) \n \
             } \n \
             def main(): Int := depth( \n \
                 Tree#Node((Tree#Node((Tree#Leaf, 1, Tree#Leaf)), 2, Tree#Leaf)))",
        ),
        Expression::Int(2)
    );
}

#[test]
fn heaped_value_dropped_unconsumed() {
    // `x` is built but never consumed, so it is deep-released at scope exit.
    // The program still produces its value (and, compiled, frees every node).
    assert_eq!(
        run_both(
            "type L = E | C((Int, L)) deriving Heaped \n \
             def main(): Int := { \n \
                 let x: L = L#C((1, L#C((2, L#E)))); \n \
                 42 \n \
             }",
        ),
        Expression::Int(42)
    );
}

#[test]
fn match_bound_subtree_dropped_in_branch() {
    // A subtree moved out of a consuming match is dropped on the branch that
    // does not consume it (Step B completing drop → C.5 release). The result is
    // unaffected, and both interpreters agree (see `examples/expr.sand` for the
    // full, compiled, leak-checked version).
    assert_eq!(
        run_both(
            "type E = Lit(Int) | Cond((E, E, E)) deriving Heaped \n \
             def ev(e: E): Int := match e { \n \
                 E#Lit(n) => n, \n \
                 E#Cond((c, t, f)) => if ev(c) != 0 then ev(t) else ev(f) \n \
             } \n \
             def main(): Int := ev(E#Cond((E#Lit(0), E#Lit(7), E#Lit(9))))",
        ),
        Expression::Int(9)
    );
}

#[test]
fn unique_take_moves_the_node_out() {
    // `unique_take` moves the node onto the stack and frees only the cell; the
    // returned value is the original payload (this backs a consuming match).
    assert_eq!(
        run_both(
            "def main(): Int := { \n \
               let h: Unique<Int> = unique_alloc(42); \n \
               unique_take(h) \n \
             }",
        ),
        Expression::Int(42)
    );
}

#[test]
fn unique_take_over_an_aggregate_payload() {
    assert_eq!(
        run_both(
            "def main(): Int := { \n \
               let h: Unique<(Int, Int)> = unique_alloc((40, 2)); \n \
               match unique_take(h) { (a, b) => a + b } \n \
             }",
        ),
        Expression::Int(42)
    );
}

#[test]
fn unique_alloc_over_an_aggregate_payload() {
    // The payload type need not be a primitive — a tuple round-trips too.
    assert_eq!(
        run_both(
            "def main(): Int := { \n \
               let h: Unique<(Int, Bool)> = unique_alloc((5, true)); \n \
               unique_release(h); \n \
               1 \n \
             }",
        ),
        Expression::Int(1)
    );
}
