//! Step 1 — type parameter syntax and structural plumbing.
//!
//! These tests cover *declaring* generic functions and enums: the grammar
//! accepts type parameters, they are threaded through the IRs, and a `T` in a
//! signature/body resolves to an opaque `Ty::Param`. Generic *instantiation*
//! (calling a generic function with concrete types) is Step 2, so these tests
//! only define generic items — calls are not exercised here.

use crate::common::parse;
use crate::common::typecheck;
use crate::common::typecheck_fails;

// ── parsing: type parameters are captured on the declaration ──────────────────

#[test]
fn parse_single_type_param() {
    let pm = parse("def id<T>(x: T): T := x");
    let f = &pm.functions[0];
    let names: Vec<&str> = f.type_params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["T"]);
}

#[test]
fn parse_multiple_type_params() {
    let pm = parse("def pair<A, B>(a: A, b: B): A := a");
    let f = &pm.functions[0];
    let names: Vec<&str> = f.type_params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["A", "B"]);
}

#[test]
fn distinct_params_get_distinct_ids() {
    let pm = parse("def pair<A, B>(a: A, b: B): A := a");
    let f = &pm.functions[0];
    assert_ne!(f.type_params[0].id, f.type_params[1].id);
}

#[test]
fn non_generic_function_has_no_type_params() {
    let pm = parse("def main(): Int := 0");
    assert!(pm.functions[0].type_params.is_empty());
}

// ── type checking: a parameter is opaque but self-consistent ──────────────────

#[test]
fn generic_identity_type_checks() {
    // body `x : T` must match the declared return type `T`.
    typecheck("def id<T>(x: T): T := x \n def main(): Int := 0");
}

#[test]
fn generic_first_of_two_type_checks() {
    typecheck("def first<A, B>(a: A, b: B): A := a \n def main(): Int := 0");
}

#[test]
fn generic_param_mixed_with_concrete_type_checks() {
    typecheck("def snd<T>(x: T, y: Int): Int := y \n def main(): Int := 0");
}

#[test]
fn type_param_inside_tuple_type_checks() {
    typecheck("def f<T>(x: (T, Int)): Int := 0 \n def main(): Int := 0");
}

#[test]
fn type_param_in_body_annotation_type_checks() {
    // a `let y: T` inside the body must resolve `T` to the same parameter, so
    // the scope has to remain active while the body is built.
    typecheck("def id<T>(x: T): T := { let y: T = x; y } \n def main(): Int := 0");
}

// ── type checking: distinct parameters are distinct opaque types ──────────────

#[test]
fn returning_wrong_param_fails() {
    // `a : A` cannot satisfy a declared return type of `B`.
    typecheck_fails("def bad<A, B>(a: A, b: B): B := a \n def main(): Int := 0");
}

#[test]
fn returning_param_where_concrete_expected_fails() {
    // `x : T` cannot satisfy a declared return type of `Int`.
    typecheck_fails("def bad<T>(x: T): Int := x \n def main(): Int := 0");
}

// ── resolution: the declaration is what makes `T` a type ──────────────────────

#[test]
fn undeclared_type_param_is_unknown_type() {
    // without `<T>`, `T` is an unknown enum type, not a parameter.
    typecheck_fails("def f(x: T): T := x \n def main(): Int := 0");
}

#[test]
fn type_param_shadows_same_named_enum() {
    // inside `f`, `T` is the parameter, not the enum `T`; `x : T(param)` matches
    // the return `T(param)`, so this type-checks.
    typecheck(
        "type T = A | B \n def f<T>(x: T): T := x \n def main(): Int := 0",
    );
}

// ── generic enum declarations ─────────────────────────────────────────────────

#[test]
fn generic_enum_declaration_compiles() {
    typecheck("type Option<T> = None | Some(T) \n def main(): Int := 0");
}

#[test]
fn generic_enum_multi_param_compiles() {
    typecheck("type Either<A, B> = Left(A) | Right(B) \n def main(): Int := 0");
}

#[test]
fn generic_enum_recursive_payload_compiles() {
    // the payload references the parameter nested inside a tuple.
    typecheck("type Pair<T> = Wrap((T, T)) \n def main(): Int := 0");
}
