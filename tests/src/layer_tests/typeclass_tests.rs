//! Step 10 — typeclass declarations, instances, and dispatch (Calculus §7,
//! §8.8–8.9).
//!
//! 10a covers declaration + registration + the coherence/orphan/superclass/
//! completeness checks (no method dispatch yet); 10b covers calling methods.

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

// ── 10a: declarations register and validate
// ───────────────────────────────────

#[test]
fn typeclass_and_impl_declare() {
    typecheck(
        "typeclass Eq<T> { def eq(a: T, b: T): Bool := true } \n \
         impl Eq for Int { def eq(a: Int, b: Int): Bool := true } \n \
         def main(): Int := 0",
    );
}

#[test]
fn impl_for_a_user_enum_declares() {
    typecheck(
        "type Color = Red | Green \n \
         typeclass Eq<T> { def eq(a: T, b: T): Bool } \n \
         impl Eq for Color { def eq(a: Color, b: Color): Bool := true } \n \
         def main(): Int := 0",
    );
}

#[test]
fn a_default_method_may_be_omitted_by_an_impl() {
    typecheck(
        "typeclass Eq<T> { def eq(a: T, b: T): Bool := true } \n \
         impl Eq for Int { } \n \
         def main(): Int := 0",
    );
}

// ── coherence / completeness / orphan-ish rejections
// ──────────────────────────

#[test]
fn duplicate_instance_is_rejected() {
    typecheck_fails(
        "typeclass Eq<T> { def eq(a: T, b: T): Bool := true } \n \
         impl Eq for Int { def eq(a: Int, b: Int): Bool := true } \n \
         impl Eq for Int { def eq(a: Int, b: Int): Bool := false } \n \
         def main(): Int := 0",
    );
}

#[test]
fn missing_non_default_method_is_rejected() {
    typecheck_fails(
        "typeclass Eq<T> { def eq(a: T, b: T): Bool } \n \
         impl Eq for Int { } \n \
         def main(): Int := 0",
    );
}

#[test]
fn unknown_method_in_impl_is_rejected() {
    typecheck_fails(
        "typeclass Eq<T> { def eq(a: T, b: T): Bool := true } \n \
         impl Eq for Int { def neq(a: Int, b: Int): Bool := true } \n \
         def main(): Int := 0",
    );
}

#[test]
fn impl_of_unknown_typeclass_is_rejected() {
    typecheck_fails("impl Nope for Int { } \n def main(): Int := 0");
}

#[test]
fn impl_for_a_reference_type_is_rejected() {
    // a reference is not a coherence head — cannot carry an instance.
    typecheck_fails(
        "typeclass Eq<T> { def eq(a: T, b: T): Bool := true } \n \
         impl Eq for &Int { def eq(a: Int, b: Int): Bool := true } \n \
         def main(): Int := 0",
    );
}

// ── classes: arity, method names, superclasses
// ────────────────────────────────

#[test]
fn typeclass_must_have_exactly_one_type_parameter() {
    typecheck_fails("typeclass Bad<T, U> { def f(a: T): Bool := true } \n def main(): Int := 0");
}

#[test]
fn a_method_name_belongs_to_one_class() {
    typecheck_fails(
        "typeclass A<T> { def f(a: T): Bool := true } \n \
         typeclass B<T> { def f(a: T): Bool := true } \n \
         def main(): Int := 0",
    );
}

#[test]
fn superclass_instance_is_required() {
    typecheck_fails(
        "typeclass A<T> { def fa(a: T): Bool := true } \n \
         typeclass B<T> requires A { def fb(a: T): Bool := true } \n \
         impl B for Int { def fb(a: Int): Bool := true } \n \
         def main(): Int := 0",
    );
}

#[test]
fn superclass_instance_present_is_accepted() {
    typecheck(
        "typeclass A<T> { def fa(a: T): Bool := true } \n \
         typeclass B<T> requires A { def fb(a: T): Bool := true } \n \
         impl A for Int { def fa(a: Int): Bool := true } \n \
         impl B for Int { def fb(a: Int): Bool := true } \n \
         def main(): Int := 0",
    );
}

#[test]
fn requires_unknown_superclass_is_rejected() {
    typecheck_fails(
        "typeclass B<T> requires Nope { def fb(a: T): Bool := true } \n def main(): Int := 0",
    );
}

// ── 10b: method dispatch runs (HIR + MIR agree)
// ───────────────────────────────

#[test]
fn concrete_method_call_runs() {
    assert_eq!(
        run_both(
            "typeclass ToInt<T> { def to_int(x: T): Int } \n \
             impl ToInt for Bool { def to_int(x: Bool): Int := if x then 1 else 0 } \n \
             def main(): Int := to_int(true)"
        ),
        Expression::Int(1)
    );
}

#[test]
fn generic_dispatch_runs_for_two_instantiations() {
    assert_eq!(
        run_both(
            "typeclass ToInt<T> { def to_int(x: T): Int } \n \
             impl ToInt for Bool { def to_int(x: Bool): Int := if x then 1 else 0 } \n \
             impl ToInt for Int { def to_int(x: Int): Int := x } \n \
             def use_it<T>(x: T): Int where T : ToInt := to_int(x) \n \
             def main(): Int := use_it(true) + use_it(7)"
        ),
        Expression::Int(8)
    );
}

#[test]
fn default_method_runs() {
    assert_eq!(
        run_both(
            "typeclass Eq<T> { \n \
               def eq(a: T, b: T): Bool \n \
               def neq(a: T, b: T): Bool := if eq(a, b) then false else true \n \
             } \n \
             impl Eq for Int { def eq(a: Int, b: Int): Bool := a == b } \n \
             def main(): Int := if neq(3, 4) then 1 else 0"
        ),
        Expression::Int(1)
    );
}

#[test]
fn default_method_override_runs() {
    assert_eq!(
        run_both(
            "typeclass Eq<T> { \n \
               def eq(a: T, b: T): Bool \n \
               def neq(a: T, b: T): Bool := if eq(a, b) then false else true \n \
             } \n \
             impl Eq for Int { def eq(a: Int, b: Int): Bool := a == b \n \
               def neq(a: Int, b: Int): Bool := false } \n \
             def main(): Int := if neq(3, 4) then 1 else 0"
        ),
        Expression::Int(0)
    );
}

#[test]
fn superclass_method_dispatch_runs() {
    // calling a superclass method through a subclass instance.
    assert_eq!(
        run_both(
            "typeclass Zero<T> { def zero(x: T): Int } \n \
             typeclass One<T> requires Zero { def one(x: T): Int } \n \
             impl Zero for Bool { def zero(x: Bool): Int := 0 } \n \
             impl One for Bool { def one(x: Bool): Int := 1 } \n \
             def main(): Int := zero(true) + one(false)"
        ),
        Expression::Int(1)
    );
}

// ── dispatch rejections
// ───────────────────────────────────────────────────────

#[test]
fn method_call_with_no_instance_is_rejected() {
    typecheck_fails(
        "typeclass ToInt<T> { def to_int(x: T): Int } \n \
         impl ToInt for Bool { def to_int(x: Bool): Int := 1 } \n \
         def main(): Int := to_int(5)",
    );
}

#[test]
fn method_on_unconstrained_type_parameter_is_rejected() {
    typecheck_fails(
        "typeclass ToInt<T> { def to_int(x: T): Int } \n \
         def bad<T>(x: T): Int := to_int(x) \n \
         def main(): Int := 0",
    );
}

#[test]
fn generic_call_with_unsatisfied_constraint_is_rejected() {
    typecheck_fails(
        "typeclass ToInt<T> { def to_int(x: T): Int } \n \
         impl ToInt for Int { def to_int(x: Int): Int := x } \n \
         def use_it<T>(x: T): Int where T : ToInt := to_int(x) \n \
         def main(): Int := use_it(true)",
    );
}
