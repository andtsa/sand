//! Concurrency scaffolding: the `Send` / `Sync` marker typeclasses and the
//! `spawn` / `join` core definitions.
//!
//! This first cut runs `spawn` *synchronously* (it evaluates `f(arg)` eagerly),
//! so the three execution backends agree; the load-bearing part exercised here
//! is the **type-level safety**: `spawn`'s `where T : Send, R : Send` bounds,
//! enforced structurally (primitives, tuples, references) and via opt-in
//! `impl Send` for aggregate types. Real OS threads are a follow-up that only
//! swaps `spawn`/`join`'s bodies.

use lang::ir_types::typed_hir::Expression;

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

// --- spawn / join run on every backend ---

#[test]
fn spawn_join_primitive() {
    assert_eq!(
        run_both("def main(): Int := { let t = spawn(fn (n: Int) -> n * n, 5); join(t) }"),
        Expression::Int(25)
    );
}

#[test]
fn spawn_join_tuple_is_structurally_send() {
    // `(Int, Int)` is `Send` because every element is, with no `impl` needed.
    assert_eq!(
        run_both(
            "def main(): Int := { \
                 let t = spawn(fn (xy: (Int, Int)) -> match xy { (a, b) => a * b }, (5, 6)); \
                 join(t) \
             }"
        ),
        Expression::Int(30)
    );
}

#[test]
fn spawn_join_user_type_with_send_impl() {
    assert_eq!(
        run_both(
            "type Pair = P(Int, Int) \n \
             impl Send for Pair { } \n \
             def main(): Int := { \
                 let t = spawn(fn (p: Pair) -> match p { Pair#P(a, b) => a + b }, Pair#P(3, 4)); \
                 join(t) \
             }"
        ),
        Expression::Int(7)
    );
}

// --- the `Send` bound is enforced ---

#[test]
fn spawn_rejects_non_send_user_type() {
    // A user aggregate is not `Send` without an explicit `impl Send`.
    typecheck_fails(
        "type Pair = P(Int, Int) \n \
         def main(): Int := { \
             let t = spawn(fn (p: Pair) -> match p { Pair#P(a, b) => a + b }, Pair#P(3, 4)); \
             join(t) \
         }",
    );
}

#[test]
fn spawn_rejects_raw_pointer_argument() {
    // `Ptr<T>` is outside the ownership discipline, so it is not `Send`.
    typecheck_fails(
        "extern def malloc(size: Int): Ptr<Unit>; \n \
         def main(): Int := { \
             let p = malloc(8); \
             let t = spawn(fn (q: Ptr<Unit>) -> 0, p); \
             join(t) \
         }",
    );
}

// --- marker typeclasses are usable as ordinary bounds ---

#[test]
fn send_bound_is_satisfied_structurally() {
    // A `where T : Send` bound resolves for a primitive with no surface `impl`.
    typecheck(
        "def on_thread<T>(x: T): T where T : Send := x \n \
         def main(): Int := on_thread(7)",
    );
}

#[test]
fn shared_reference_is_send_when_referent_is_sync() {
    // `&Int : Send` because `Int : Sync` (structural). A `where T : Send` bound
    // on `&Int` therefore resolves.
    typecheck(
        "def needs_send<T>(x: T): Int where T : Send := 0 \n \
         def main(): Int := { let n = 7; needs_send(&n) }",
    );
}
