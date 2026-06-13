//! region-parameterized ADTs (Calculus §2.3, §6.3).
//!
//! A reference stored in an ADT payload must be tied to a region parameter of
//! the type (`type Holder<'a> = H(&'a T)`); the region is threaded into the
//! type at instantiation (`Holder<'a>`), so `freeRegions` exposes it and the
//! escape check catches an ADT that holds a borrow of a local. This closes the
//! "escape via data" hole for enums/ADTs (tuples were closed earlier).

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

// ── declaration: a payload borrow must name a lifetime parameter
// ──────────────

#[test]
fn elided_borrow_in_a_payload_is_rejected() {
    // `&Int` in a payload has no trackable lifetime; require `<'a>` + `&'a Int`.
    typecheck_fails("type Holder = H(&Int) \n def main(): Int := 0");
}

#[test]
fn region_parametric_payload_is_accepted() {
    typecheck("type Holder<'r> = H(&'r Int) \n def main(): Int := 0");
}

#[test]
fn static_borrow_in_a_payload_is_accepted() {
    // a `'static` borrow never dangles, so it needs no region parameter.
    typecheck("type Holder = H(&'static Int) \n def main(): Int := 0");
}

// ── the hole: an ADT holding a borrow of a local cannot escape
// ────────────────

#[test]
fn returning_an_adt_holding_a_local_borrow_is_rejected() {
    typecheck_fails(
        "type Holder<'r> = H(&'r Int) \n \
         def f<'a>(): Holder<'a> := { let y = 5; Holder#H(&y) } \n \
         def main(): Int := 0",
    );
}

#[test]
fn returning_a_nested_adt_holding_a_local_borrow_is_rejected() {
    typecheck_fails(
        "type Holder<'r> = H(&'r Int) \n \
         type Outer<'r> = O(Holder<'r>) \n \
         def f<'a>(): Outer<'a> := { let y = 5; Outer#O(Holder#H(&y)) } \n \
         def main(): Int := 0",
    );
}

#[test]
fn extracting_a_local_borrow_from_an_adt_and_returning_it_is_rejected() {
    // R5d: the binding keeps the instantiation's region, so the extracted borrow
    // names the local scope and escapes.
    typecheck_fails(
        "type Holder<'r> = H(&'r Int) \n \
         def f(): &Int := { let y = 5; match Holder#H(&y) { Holder#H(r) => r } } \n \
         def main(): Int := 0",
    );
}

// ── accepted: borrows that outlive the call, or never leave their scope
// ────────

#[test]
fn returning_an_adt_holding_a_parameter_borrow_is_accepted() {
    typecheck(
        "type Holder<'r> = H(&'r Int) \n \
         def make<'a>(x: &'a Int): Holder<'a> := Holder#H(x) \n \
         def main(): Int := 0",
    );
}

#[test]
fn returning_a_nested_adt_holding_a_parameter_borrow_is_accepted() {
    typecheck(
        "type Holder<'r> = H(&'r Int) \n \
         type Outer<'r> = O(Holder<'r>) \n \
         def make<'a>(x: &'a Int): Outer<'a> := Outer#O(Holder#H(x)) \n \
         def main(): Int := 0",
    );
}

#[test]
fn an_adt_holding_a_local_borrow_used_in_scope_is_accepted() {
    assert_eq!(
        run_both(
            "type Holder<'r> = H(&'r Int) \n \
             def main(): Int := { let y = 5; match Holder#H(&y) { Holder#H(r) => *r } }"
        ),
        Expression::Int(5)
    );
}

#[test]
fn extracting_from_a_parameter_adt_and_returning_it_is_accepted() {
    typecheck(
        "type Holder<'r> = H(&'r Int) \n \
         def f<'a>(h: Holder<'a>): &'a Int := match h { Holder#H(r) => r } \n \
         def main(): Int := 0",
    );
}

#[test]
fn adt_region_flows_modularly_across_a_call() {
    // the callee's `Holder<'a>` parameter region is inferred from the actual
    // argument's region at the call site (signature-level, no body inspection).
    typecheck(
        "type Holder<'r> = H(&'r Int) \n \
         def use_it<'a>(h: Holder<'a>): &'a Int := match h { Holder#H(r) => r } \n \
         def f<'b>(x: &'b Int): &'b Int := use_it(Holder#H(x)) \n \
         def main(): Int := 0",
    );
}

// ── multiple independent lifetimes (A's advantage over a meet collapse)
// ────────

#[test]
fn a_pair_keeps_two_distinct_lifetimes() {
    assert_eq!(
        run_both(
            "type Pair<'a, 'b> = P((&'a Int, &'b Int)) \n \
             def main(): Int := { let x = 3; let y = 4; match Pair#P((&x, &y)) { Pair#P((a, b)) => *a } }"
        ),
        Expression::Int(3)
    );
}

// ── use-site syntax + arity
// ───────────────────────────────────────────────────

#[test]
fn region_parametric_enum_in_a_signature_type_checks() {
    typecheck(
        "type Holder<'r> = H(&'r Int) \n \
         def f<'a>(h: Holder<'a>): Int := 0 \n \
         def main(): Int := 0",
    );
}

#[test]
fn wrong_lifetime_arity_is_rejected() {
    typecheck_fails(
        "type Holder<'r> = H(&'r Int) \n \
         def f<'a, 'b>(h: Holder<'a, 'b>): Int := 0 \n \
         def main(): Int := 0",
    );
}

#[test]
fn lifetime_arguments_must_come_before_type_arguments() {
    typecheck_fails(
        "type Both<'r, T> = B(&'r T) \n \
         def f<'a>(h: Both<Int, 'a>): Int := 0 \n \
         def main(): Int := 0",
    );
}
