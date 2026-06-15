//! Step 13 (milestone 1) — function *types* `A -> B`.
//!
//! This milestone adds the type former only — there are no lambda *values* yet,
//! so these tests exercise parsing, right-associativity, threading through
//! composite/generic types, and the application mismatch. Bare `->` is the
//! reusable arrow (Calculus §3.1 `→[Borrowed]`, ≈ Rust `Fn`).

use crate::common::parse;
use crate::common::typecheck;
use crate::common::typecheck_fails;

#[test]
fn function_typed_parameter_typechecks() {
    let (ctx, _p) = typecheck("def f(g: Int -> Bool): Int := 0 \n def main(): Int := 0");
    std::mem::forget(ctx);
}

#[test]
fn arrow_is_right_associative() {
    // `Int -> Bool -> Unit` parses as `Int -> (Bool -> Unit)` (a curried
    // codomain); it parses and type-checks in a signature.
    let (ctx, _p) = typecheck("def f(g: Int -> Bool -> Unit): Int := 0 \n def main(): Int := 0");
    std::mem::forget(ctx);
    parse("def f(g: Int -> Bool -> Unit): Int := 0");
}

#[test]
fn function_type_in_a_tuple_payload() {
    let (ctx, _p) = typecheck(
        "type Holder = H((Int -> Bool, Int)) \n \
         def f(h: Holder): Int := 0 \n \
         def main(): Int := 0",
    );
    std::mem::forget(ctx);
}

#[test]
fn function_type_as_a_generic_argument() {
    // `Box<Int -> Bool>` — a function type as a type argument; exercises the
    // `Fn` threading through `App` + monomorphisation/mangling.
    let (ctx, _p) = typecheck(
        "type Box<a> = B(a) \n \
         def f(x: Box<Int -> Bool>): Int := 0 \n \
         def main(): Int := 0",
    );
    std::mem::forget(ctx);
}

#[test]
fn function_type_with_a_generic_parameter() {
    let (ctx, _p) =
        typecheck("def f<T>(g: T -> Int): Int := 0 \n def main(): Int := 0");
    std::mem::forget(ctx);
}

#[test]
fn returning_a_non_function_where_a_function_type_is_expected_is_rejected() {
    // No function values exist yet, so `0` cannot inhabit `Int -> Bool`.
    typecheck_fails("def f(): Int -> Bool := 0 \n def main(): Int := 0");
}
