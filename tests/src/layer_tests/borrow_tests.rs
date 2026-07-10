//! Shared borrows: the `Borrowed` kind, `&'r T` reference types, and
//! `&e` borrow expressions (Calculus: Kinds, Types, Terms).
//!
//! Borrows are immutable and have no distinct runtime representation yet, so
//! monomorphisation erases `&'r T` to `T` and borrows lower transparently.
//! The block-region escape check is exercised separately.

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

// --- kind lattice: Owned <: Borrowed// --- ---
#[test]
fn owned_is_subkind_of_borrowed() {
    assert!(Kind::Owned.is_subkind(Kind::Borrowed));
    assert!(Kind::Never.is_subkind(Kind::Borrowed));
    assert!(!Kind::Borrowed.is_subkind(Kind::Owned));
}

// --- reference types and borrow expressions parse and type-check// --- ---
#[test]
fn reference_type_parameter_parses() {
    parse("def f(r: &Int): Int := 0");
}

#[test]
fn explicit_lifetime_reference_parses() {
    parse("def f<'r>(r: &'r Int): Int := 0");
}

#[test]
fn borrow_expression_type_checks() {
    typecheck("def f(x: Int): Int := { let r = &x; x } \n def main(): Int := 0");
}

#[test]
fn passing_a_borrow_to_a_reference_parameter_type_checks() {
    typecheck("def takes(r: &Int): Int := 0 \n def main(): Int := takes(&5)");
}

#[test]
fn borrowing_an_int_then_a_bool() {
    typecheck("def f(b: Bool): Int := { let r = &b; 0 } \n def main(): Int := 0");
}

// --- borrows do not consume their referent (Var-Borrow)// --- ---
#[test]
fn borrowing_does_not_move_a_non_copy_value() {
    // `&e` borrows `e` without moving it: once the borrow's scope ends, `e` is
    // still owned and may be consumed. The borrow is scoped to an inner block so
    // it is released before `e` is matched (a value may not be moved *while*
    // borrowed).
    typecheck(
        "type E = A | B \n \
         def f(e: E): Int := { { let r = &e; 0 }; match e { E#A => 1, E#B => 2 } } \n \
         def main(): Int := 0",
    );
}

#[test]
fn move_while_borrowed_is_rejected() {
    // a value may not be moved while a borrow of it is *still live*: `match e`
    // consumes `e` while `r` still borrows it; `r` is used (via `g(r)`) after
    // the match, so the loan spans the move.
    typecheck_fails(
        "type E = A | B \n \
         def g(r: &E): Int := 0 \n \
         def f(e: E): Int := { let r = &e; let m = match e { E#A => 1, E#B => 2 }; g(r) } \n \
         def main(): Int := 0",
    );
}

#[test]
fn borrow_passed_to_a_call_stays_lexical_conservatively() {
    // A reference passed to a call is treated as *escaping* (a callee could
    // stash it via a `&mut` out-param, which the tree analysis can't rule out),
    // so its loan stays lexical and the move of `e` is still rejected. Precise
    // NLL release for this case awaits a future region-dataflow analysis. (The
    // sound NLL win is for borrows used only by dereference; see
    // `mut_borrow_tests`.)
    typecheck_fails(
        "type E = A | B \n \
         def g(r: &E): Int := 0 \n \
         def f(e: E): Int := { let r = &e; let u = g(r); match e { E#A => 1, E#B => 2 } } \n \
         def main(): Int := 0",
    );
}

#[test]
fn double_move_without_borrow_still_fails() {
    // the control case: moving a non-copy value twice is still an ownership
    // error (the borrow above is what makes the difference).
    typecheck_fails(
        "type E = A | B \n \
         def f(e: E): Int := { let a = e; let b = e; 0 } \n \
         def main(): Int := 0",
    );
}

#[test]
fn multiple_borrows_of_the_same_value() {
    // several shared borrows of the same value coexist; none consumes it.
    typecheck(
        "type E = A | B \n \
         def f(e: E): Int := { let a = &e; let b = &e; 0 } \n \
         def main(): Int := 0",
    );
}

// --- borrows compile and run (erased transparently)// --- ---
#[test]
fn borrow_program_runs() {
    assert_eq!(
        run_both("def takes(r: &Int): Int := 0 \n def main(): Int := takes(&7)"),
        Expression::Int(0)
    );
}

#[test]
fn borrowed_value_still_usable_at_runtime() {
    // borrow `x`, then return `x`: the borrow is transparent, so the value is
    // unaffected.
    assert_eq!(
        run_both("def f(x: Int): Int := { let r = &x; x } \n def main(): Int := f(42)"),
        Expression::Int(42)
    );
}

// --- `let &x` borrow binding (desugars to `let x = &e`)// --- ---
#[test]
fn let_borrow_binding_type_checks() {
    typecheck("def f(x: Int): Int := { let &r = x; x } \n def main(): Int := 0");
}

#[test]
fn let_borrow_binding_does_not_consume() {
    // `let &r = e` borrows `e`; once its (inner-block) scope ends, a non-copy `e`
    // is still usable. Scoped so the borrow is released before the match.
    typecheck(
        "type E = A | B \n \
         def f(e: E): Int := { { let &r = e; 0 }; match e { E#A => 1, E#B => 2 } } \n \
         def main(): Int := 0",
    );
}

#[test]
fn let_borrow_binding_runs() {
    assert_eq!(
        run_both("def f(x: Int): Int := { let &r = x; x } \n def main(): Int := f(9)"),
        Expression::Int(9)
    );
}

// --- borrowing match: destructuring through a shared reference// --- ---
//
// `match` on a `&T` matches the pointee and binds each payload field as a `&`
// borrow (so the field is read, not moved). This is what makes `Clone`
// implementable for non-`Copy` aggregates (see
// `examples/ownership/clone_impl.sand`).

const PAIR: &str = "type Pair = P(Int, Int)\n";

#[test]
fn borrowing_match_binds_fields_as_references() {
    // `a` and `b` are used as `&Int` (dereferenced), proving the bindings are
    // borrows of the fields rather than owned moves out of the borrow.
    typecheck(&format!(
        "{PAIR} def sum(p: &Pair): Int := match p {{ Pair#P(a, b) => *a + *b }} \n \
         def main(): Int := 0"
    ));
}

#[test]
fn borrowing_match_runs() {
    assert_eq!(
        run_both(&format!(
            "{PAIR} def sum(p: &Pair): Int := match p {{ Pair#P(a, b) => *a + *b }} \n \
             def main(): Int := sum(&Pair#P(3, 4))"
        )),
        Expression::Int(7)
    );
}

#[test]
fn borrowing_match_does_not_consume_scrutinee() {
    // `p` is borrow-matched twice: a borrowing match must not move the scrutinee.
    assert_eq!(
        run_both(&format!(
            "{PAIR} def sum(p: &Pair): Int := match p {{ Pair#P(a, b) => *a + *b }} \n \
             def main(): Int := {{ let p = Pair#P(3, 4); sum(&p) + sum(&p) }}"
        )),
        Expression::Int(14)
    );
}

#[test]
fn borrowing_match_clones_non_copy_aggregate() {
    // The motivating case: `Clone` for a non-`Copy` type, with no `Copy` impl.
    assert_eq!(
        run_both(&format!(
            "{PAIR} \
             impl Clone for Pair {{ \
                 def clone(x: &Pair): Pair := match x {{ Pair#P(a, b) => Pair#P(clone(a), clone(b)) }} \
             }} \n \
             def fst(p: &Pair): Int := match p {{ Pair#P(a, b) => *a }} \n \
             def main(): Int := {{ let p = Pair#P(5, 9); let q = clone(&p); fst(&q) + fst(&p) }}"
        )),
        Expression::Int(10)
    );
}

#[test]
fn borrowing_match_nested_aggregate_runs() {
    // Variant -> Tuple -> &field, exercising multi-level field projections.
    assert_eq!(
        run_both(&format!(
            "{PAIR} type Shape = Dot | Box(Pair, Pair) \n \
             def area(s: &Shape): Int := match s {{ \
                 Shape#Dot => 0, \
                 Shape#Box(p, q) => match p {{ Pair#P(a, b) => match q {{ Pair#P(c, d) => *a + *b + *c + *d }} }}, \
             }} \n \
             def main(): Int := area(&Shape#Box(Pair#P(1, 2), Pair#P(3, 4)))"
        )),
        Expression::Int(10)
    );
}

// --- &mut destructuring ---
// each field binds as `&mut`, write-through mutates the live referent

#[test]
fn mut_borrowing_match_binds_fields_as_mut_references() {
    // `*a = ...` write-through requires `a : &mut Int`.
    typecheck(&format!(
        "{PAIR} def f(p: &mut Pair): Unit := {{ match p {{ Pair#P(a, b) => {{ *a = *b; }} }} }} \n \
         def main(): Int := 0"
    ));
}

#[test]
fn mut_borrowing_match_writes_through_within_an_arm() {
    // Mutate both fields, then read them back in the same arm.
    assert_eq!(
        run_both(&format!(
            "{PAIR} def bump(p: &mut Pair): Int := \
                 match p {{ Pair#P(a, b) => {{ *a = *a + 10; *b = *b + 20; *a + *b }} }} \n \
             def main(): Int := {{ let mut p = Pair#P(1, 2); bump(&mut p) }}"
        )),
        Expression::Int(33)
    );
}

#[test]
fn mut_borrowing_match_mutation_persists_to_referent() {
    // After a scoped `&mut` destructure writes the fields, an owned read of `p`
    // observes the new values, proving the write hit the original, not a copy.
    assert_eq!(
        run_both(&format!(
            "{PAIR} def main(): Int := {{ \
                 let mut p = Pair#P(1, 2); \
                 {{ match &mut p {{ Pair#P(a, b) => {{ *a = 100; *b = 200; }} }}; }}; \
                 match p {{ Pair#P(a, b) => a + b }} \
             }}"
        )),
        Expression::Int(300)
    );
}

#[test]
fn mut_borrowing_match_disjoint_fields_used_together() {
    // Write one field using a read of the other; disjoint `&mut`s coexist.
    assert_eq!(
        run_both(&format!(
            "{PAIR} def f(p: &mut Pair): Int := \
                 match p {{ Pair#P(a, b) => {{ *a = *a + *b; *a }} }} \n \
             def main(): Int := {{ let mut p = Pair#P(3, 4); f(&mut p) }}"
        )),
        Expression::Int(7)
    );
}

#[test]
fn mut_borrowing_match_through_mut_reference_is_rejected() {
    // A heaped `&mut` is still unsupported (needs `unique_borrow`).
    typecheck_fails(
        "type List = Cons(Int, List) | Nil deriving Heaped \n \
         def f(l: &mut List): Int := match l { List#Cons(h, t) => *h, List#Nil => 0 } \n \
         def main(): Int := 0",
    );
}

#[test]
fn borrowing_match_on_heaped_pointee_is_rejected() {
    // A `&Heaped` references a `Unique` handle; reading its fields needs a
    // `unique_borrow` indirection that does not exist yet.
    typecheck_fails(
        "type List = Cons(Int, List) | Nil deriving Heaped \n \
         def f(l: &List): Int := match l { List#Cons(h, t) => *h, List#Nil => 0 } \n \
         def main(): Int := 0",
    );
}
