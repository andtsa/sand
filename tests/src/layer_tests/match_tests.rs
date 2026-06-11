//! Tests for pattern matching expressions
//!
//! Covers: basic match on named enums, ad-hoc tag unions, wildcard arms,
//! bare-tag patterns, exhaustiveness checking, cross-module enums,
//! and all error cases.

use lang::interpreter::mir::MirValue;
use lang::ir_types::typed_hir::Expression;

use crate::common::*;

// ── basic named-enum match
// ────────────────────────────────────────────────────

/// A simple match on all variants compiles and runs.
#[test]
fn match_named_enum_all_variants() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def main(): Int :=
             match Light#Red {
                 Light#Red => 0,
                 Light#Yellow => 1,
                 Light#Green => 2,
             }",
    );
    assert_eq!(val, MirValue::Int(0));
}

/// Match on the second variant returns the correct arm.
#[test]
fn match_named_enum_second_variant() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def main(): Int :=
             match Light#Yellow {
                 Light#Red => 0,
                 Light#Yellow => 1,
                 Light#Green => 2,
             }",
    );
    assert_eq!(val, MirValue::Int(1));
}

/// Match on the last variant returns the correct arm.
#[test]
fn match_named_enum_last_variant() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def main(): Int :=
             match Light#Green {
                 Light#Red => 0,
                 Light#Yellow => 1,
                 Light#Green => 2,
             }",
    );
    assert_eq!(val, MirValue::Int(2));
}

// ── wildcard arm ─────────────────────────────────────────────────────────────

/// A wildcard arm catches any variant not matched earlier.
#[test]
fn match_wildcard_catches_remaining() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def main(): Int :=
             match Light#Green {
                 Light#Red => 1,
                 _ => 99,
             }",
    );
    assert_eq!(val, MirValue::Int(99));
}

/// A wildcard-only match expression compiles and runs.
#[test]
fn match_wildcard_only() {
    let val = run_mir(
        "type Dir = North | South | East | West
         def main(): Int :=
             match Dir#East { _ => 42 }",
    );
    assert_eq!(val, MirValue::Int(42));
}

/// A wildcard arm after partial coverage catches the rest.
#[test]
fn match_partial_then_wildcard() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def main(): Bool :=
             match Light#Yellow {
                 Light#Red => true,
                 _ => false,
             }",
    );
    assert_eq!(val, MirValue::Bool(false));
}

// ── bare-tag patterns
// ─────────────────────────────────────────────────────────

/// A match arm can use a bare `#tag` pattern when the scrutinee is an ad-hoc
/// tag-union type.
#[test]
fn match_bare_tag_pattern_adhoc() {
    let val = run_mir(
        "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
             if a > b then #gt else if a < b then #lt else #eq
         def main(): Int :=
             match cmp(3, 1) {
                 #gt => 1,
                 #lt => -1,
                 #eq => 0,
             }",
    );
    assert_eq!(val, MirValue::Int(1));
}

/// All three tags of a #gt|#lt|#eq union are reachable.
#[test]
fn match_bare_tag_all_arms() {
    let vals = [
        run_mir(
            "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
                 if a > b then #gt else if a < b then #lt else #eq
             def main(): Int :=
                 match cmp(2, 2) { #gt => 1, #lt => -1, #eq => 0 }",
        ),
        run_mir(
            "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
                 if a > b then #gt else if a < b then #lt else #eq
             def main(): Int :=
                 match cmp(5, 1) { #gt => 1, #lt => -1, #eq => 0 }",
        ),
        run_mir(
            "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
                 if a > b then #gt else if a < b then #lt else #eq
             def main(): Int :=
                 match cmp(1, 5) { #gt => 1, #lt => -1, #eq => 0 }",
        ),
    ];
    assert_eq!(vals[0], MirValue::Int(0)); // eq
    assert_eq!(vals[1], MirValue::Int(1)); // gt
    assert_eq!(vals[2], MirValue::Int(-1)); // lt
}

/// Bare-tag patterns work with a wildcard fallback.
#[test]
fn match_bare_tag_with_wildcard() {
    let val = run_mir(
        "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
             if a > b then #gt else if a < b then #lt else #eq
         def main(): Bool :=
             match cmp(3, 3) {
                 #gt => false,
                 _ => true,
             }",
    );
    assert_eq!(val, MirValue::Bool(true));
}

// ── match returns an enum value
// ───────────────────────────────────────────────

/// A match expression can return an enum variant.
#[test]
fn match_returns_enum_variant() {
    let val = run_mir(
        "type Color = Red | Blue | Green
         def flip(c: Color): Color :=
             match c {
                 Color#Red => Color#Blue,
                 Color#Blue => Color#Red,
                 Color#Green => Color#Green,
             }
         def main(): Color := flip(Color#Red)",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => assert_eq!(variant_idx, 1), // Blue
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

// ── check mode in match bodies
// ────────────────────────────────────────────────

/// Bare tags in match arm bodies are resolved by the return type annotation.
#[test]
fn match_bare_tag_in_arm_body_resolved_by_return_type() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def next(l: Light): Light :=
             match l {
                 Light#Red => #Yellow,
                 Light#Yellow => #Green,
                 Light#Green => #Red,
             }
         def main(): Light := next(Light#Red)",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => assert_eq!(variant_idx, 1), // Yellow
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

// ── match in expression context
// ───────────────────────────────────────────────

/// A match expression can be used inside a block.
#[test]
fn match_in_block() {
    let val = run_mir(
        "type Dir = North | South
         def main(): Int := {
             let x: Dir = Dir#South;
             let n: Int = match x {
                 Dir#North => 0,
                 Dir#South => 1,
             };
             n
         }",
    );
    assert_eq!(val, MirValue::Int(1));
}

/// A match expression can be the condition's result used in arithmetic.
#[test]
fn match_result_used_in_arithmetic() {
    let val = run_mir(
        "type Coin = Heads | Tails
         def main(): Int := {
             let side: Coin = Coin#Heads;
             match side {
                 Coin#Heads => 1,
                 Coin#Tails => 0,
             } + 10
         }",
    );
    assert_eq!(val, MirValue::Int(11));
}

// ── cross-module enum match
// ───────────────────────────────────────────────────

/// A match on a cross-module enum using local constructors works.
#[test]
fn match_cross_module_enum() {
    let val = run_mir(
        "module colors;
         type Light = Red | Yellow | Green

         module app;
         def main(): Int :=
             match colors::Light#Yellow {
                 Light#Red => 0,
                 Light#Yellow => 1,
                 Light#Green => 2,
             }",
    );
    assert_eq!(val, MirValue::Int(1));
}

// ── two-variant enum
// ──────────────────────────────────────────────────────────

/// A boolean-like two-variant enum works with match.
#[test]
fn match_two_variant_enum() {
    let val = run_mir(
        "type Bit = Zero | One
         def flip(b: Bit): Bit :=
             match b { Bit#Zero => Bit#One, Bit#One => Bit#Zero }
         def main(): Bit := flip(Bit#Zero)",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => assert_eq!(variant_idx, 1), // One
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

// ── single-variant enum
// ───────────────────────────────────────────────────────

/// A single-variant enum can be matched.
#[test]
fn match_single_variant_enum() {
    let val = run_mir(
        "type Unit2 = Only
         def main(): Int :=
             match Unit2#Only { Unit2#Only => 42 }",
    );
    assert_eq!(val, MirValue::Int(42));
}

// ── match in recursive function
// ───────────────────────────────────────────────

/// A recursive function using match works correctly.
#[test]
fn match_in_recursive_function() {
    // count down from n using an enum for the direction
    let val = run_mir(
        "type Dir = Up | Down
         def adjust(n: Int, d: Dir): Int :=
             match d {
                 Dir#Up => n + 1,
                 Dir#Down => n - 1,
             }
         def main(): Int := adjust(adjust(10, Dir#Up), Dir#Down)",
    );
    assert_eq!(val, MirValue::Int(10));
}

// ── error cases
// ───────────────────────────────────────────────────────────────

/// A match missing a variant (no wildcard) is a type error.
#[test]
fn match_non_exhaustive_is_error() {
    typecheck_fails(
        "type Light = Red | Yellow | Green
         def main(): Int :=
             match Light#Red {
                 Light#Red => 0,
                 Light#Yellow => 1,
             }",
    );
}

/// matching on Int with a wildcard is valid.
#[test]
fn match_on_int_with_wildcard_is_ok() {
    typecheck(
        "def main(): Int :=
             match 42 {
                 _ => 0,
             }",
    );
}

/// A match on a Unit type (no valid patterns) is still a type error.
#[test]
fn match_on_function_return_with_no_wildcard_is_error() {
    typecheck_fails(
        "def main(): Int :=
             match 42 {
                 1 => 1,
             }",
    );
}

/// A duplicate pattern in the same match is a type error.
#[test]
fn match_duplicate_pattern_is_error() {
    typecheck_fails(
        "type Light = Red | Yellow | Green
         def main(): Int :=
             match Light#Red {
                 Light#Red => 0,
                 Light#Red => 1,
                 Light#Yellow => 2,
                 Light#Green => 3,
             }",
    );
}

/// An arm after a wildcard is unreachable — type error.
#[test]
fn match_arm_after_wildcard_is_error() {
    typecheck_fails(
        "type Light = Red | Yellow | Green
         def main(): Int :=
             match Light#Red {
                 _ => 0,
                 Light#Red => 1,
             }",
    );
}

/// Matching with a pattern from a different enum is a type error.
#[test]
fn match_wrong_enum_type_is_error() {
    typecheck_fails(
        "type Light = Red | Yellow | Green
         type Color = Red | Blue
         def main(): Int :=
             match Light#Red {
                 Color#Red => 0,
                 Color#Blue => 1,
             }",
    );
}

/// A pattern with an unknown enum type is a qualify error.
#[test]
fn match_unknown_enum_type_in_pattern_is_error() {
    qualify_fails(
        "def main(): Int :=
             match 0 { Bogus#Foo => 1, _ => 0 }",
    );
}

/// A pattern with an unknown variant on a known enum is a qualify error.
#[test]
fn match_unknown_variant_in_pattern_is_error() {
    qualify_fails(
        "type Light = Red | Yellow | Green
         def main(): Int :=
             match Light#Red { Light#Purple => 0, _ => 1 }",
    );
}

/// A bare `#tag` pattern with an unknown tag on the scrutinee type is an error.
#[test]
fn match_unknown_bare_tag_pattern_is_error() {
    typecheck_fails(
        "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
             if a > b then #gt else if a < b then #lt else #eq
         def main(): Int :=
             match cmp(1, 2) {
                 #gt => 1,
                 #lt => -1,
                 #bogus => 0,
             }",
    );
}

/// Arm bodies must all have the same type.
#[test]
fn match_arm_type_mismatch_is_error() {
    typecheck_fails(
        "type Bit = Zero | One
         def main(): Int :=
             match Bit#Zero {
                 Bit#Zero => 0,
                 Bit#One => true,
             }",
    );
}

// ── ad-hoc single-tag match
// ───────────────────────────────────────────────────

/// A single-tag ad-hoc type matched with the wildcard works.
#[test]
fn match_single_adhoc_tag_wildcard() {
    let val = run_mir(
        "def get_unit(): #unit := #unit
         def main(): Int := match get_unit() { _ => 7 }",
    );
    assert_eq!(val, MirValue::Int(7));
}

/// A single-tag ad-hoc type matched exhaustively with the bare-tag pattern.
#[test]
fn match_single_adhoc_tag_exhaustive() {
    let val = run_mir(
        "def get_unit(): #unit := #unit
         def main(): Int := match get_unit() { #unit => 99 }",
    );
    assert_eq!(val, MirValue::Int(99));
}

// ── destructuring patterns ───────────────────────────────────────────────────
//
// covers the destructuring-patterns feature: binding a payload-carrying
// variant's payload (`Circle(r)`), tuple destructuring (`(a, b)`), nested
// destructuring (`Wrap((x, y))`), wildcard/binding sub-patterns, and the new
// error cases (duplicate bindings, arity mismatches, refutable nested
// patterns, undestructured payloads). see `DESTRUCTURING_PATTERNS.todo.md`.

/// destructuring a payload-carrying variant binds the payload to a variable
/// usable in the arm body.
#[test]
fn match_destructures_variant_payload_into_binding() {
    let val = run_mir(
        "type Shape = Circle(Int) | Point
         def main(): Int :=
             match Shape#Circle(5) {
                 Shape#Circle(r) => r,
                 Shape#Point => 0,
             }",
    );
    assert_eq!(val, MirValue::Int(5));
}

/// the bound payload variable can be used in arithmetic in the arm body.
#[test]
fn match_destructured_payload_used_in_arithmetic() {
    let val = run_mir(
        "type Shape = Circle(Int) | Point
         def main(): Int :=
             match Shape#Circle(5) {
                 Shape#Circle(r) => r * 2,
                 Shape#Point => 0,
             }",
    );
    assert_eq!(val, MirValue::Int(10));
}

/// a wildcard sub-pattern discards the payload while still requiring the
/// tag to match — equivalent to "match this variant, ignore its payload".
#[test]
fn match_variant_payload_wildcard() {
    let val = run_mir(
        "type Shape = Circle(Int) | Point
         def main(): Int :=
             match Shape#Circle(5) {
                 Shape#Circle(_) => 1,
                 Shape#Point => 2,
             }",
    );
    assert_eq!(val, MirValue::Int(1));
}

/// a top-level tuple pattern destructures a tuple scrutinee, binding each
/// element.
#[test]
fn match_destructures_tuple_into_bindings() {
    let val = run_mir(
        "def main(): Int :=
             match (3, 4) {
                 (a, b) => a + b,
             }",
    );
    assert_eq!(val, MirValue::Int(7));
}

/// tuple patterns may mix bindings and wildcards.
#[test]
fn match_tuple_pattern_with_wildcard_element() {
    let val = run_mir(
        "def main(): Int :=
             match (3, 4) {
                 (a, _) => a,
             }",
    );
    assert_eq!(val, MirValue::Int(3));
}

/// nested destructuring: a variant payload that is itself a tuple can be
/// destructured in one pattern, e.g. `Wrap((x, y))`.
#[test]
fn match_nested_destructure_variant_payload_tuple() {
    let val = run_mir(
        "type Pair = Wrap((Int, Int))
         def main(): Int :=
             match Pair#Wrap((3, 4)) {
                 Pair#Wrap((x, y)) => x + y,
             }",
    );
    assert_eq!(val, MirValue::Int(7));
}

/// nested destructuring also works with mixed bindings/wildcards at
/// different levels.
#[test]
fn match_nested_destructure_partial_bindings() {
    let val = run_mir(
        "type Pair = Wrap((Int, Int))
         def main(): Int :=
             match Pair#Wrap((3, 4)) {
                 Pair#Wrap((x, _)) => x,
             }",
    );
    assert_eq!(val, MirValue::Int(3));
}

/// a plain binding pattern at the top level of a match against a tuple
/// scrutinee binds the whole scrutinee (irrefutable, single arm suffices).
#[test]
fn match_tuple_scrutinee_binding_pattern() {
    let val = run_mir(
        "def main(): Int :=
             match (3, 4) {
                 p => 9,
             }",
    );
    assert_eq!(val, MirValue::Int(9));
}

/// destructuring picks the right arm out of multiple variants and binds
/// correctly in each.
#[test]
fn match_destructure_dispatches_and_binds_correct_arm() {
    let val = run_mir(
        "type Shape = Circle(Int) | Square(Int) | Point
         def main(): Int :=
             match Shape#Square(7) {
                 Shape#Circle(r) => r,
                 Shape#Square(s) => s * s,
                 Shape#Point => 0,
             }",
    );
    assert_eq!(val, MirValue::Int(49));
}

// ── destructuring error cases
// ──────────────────────────────────────────────

/// duplicate bindings in the same pattern (`(x, x)`) are a uniquify error
/// (decision D2 — mirrors Rust's E0416).
#[test]
fn match_duplicate_binding_in_pattern_is_error() {
    qualify_fails(
        "def main(): Int :=
             match (3, 4) {
                 (x, x) => x,
             }",
    );
}

/// a tuple pattern with the wrong arity is a type error.
#[test]
fn match_tuple_pattern_arity_mismatch_is_error() {
    typecheck_fails(
        "def main(): Int :=
             match (3, 4) {
                 (a, b, c) => a,
             }",
    );
}

/// a pattern that doesn't destructure a payload-carrying variant's payload
/// is a type error — the payload must be bound or wildcarded.
#[test]
fn match_undestructured_payload_is_error() {
    typecheck_fails(
        "type Shape = Circle(Int) | Point
         def main(): Int :=
             match Shape#Circle(5) {
                 Shape#Circle => 1,
                 Shape#Point => 2,
             }",
    );
}

/// a pattern that destructures a nullary variant (no declared payload) is a
/// type error.
#[test]
fn match_destructure_nullary_variant_is_error() {
    typecheck_fails(
        "type Shape = Circle(Int) | Point
         def main(): Int :=
             match Shape#Point(5) {
                 Shape#Circle(r) => r,
                 Shape#Point(x) => x,
             }",
    );
}

/// When nested variant arms together cover ALL inner variants the outer variant
/// is now considered fully covered — the match is exhaustive without a
/// wildcard. (Nested exhaustiveness, todo 7.)
#[test]
fn match_nested_variant_all_inner_covered_is_exhaustive() {
    let result = run_hir(
        "type Inner = A | B
         type Outer = Wrap(Inner)
         def main(): Int :=
             match Outer#Wrap(Inner#A) {
                 Outer#Wrap(Inner#A) => 1,
                 Outer#Wrap(Inner#B) => 2,
             }",
    );
    assert_eq!(result, Expression::Int(1));
}

/// When only a SUBSET of inner variants are covered the outer variant is still
/// not fully covered → NonExhaustiveMatch error.
#[test]
fn match_nested_variant_partial_inner_coverage_is_error() {
    typecheck_fails(
        "type Inner = A | B | C
         type Outer = Wrap(Inner)
         def main(): Int :=
             match Outer#Wrap(Inner#A) {
                 Outer#Wrap(Inner#A) => 1,
                 Outer#Wrap(Inner#B) => 2,
             }",
    );
}

// ─── Nested exhaustiveness (todo 7) ─────────────────────────────────────────

/// The false-positive bug: a tuple-wrapped refutable sub-pattern (e.g. the
/// `List#Empty` inside `Cons((x, List#Empty))`) must make the outer Cons arm
/// count as refutable → NonExhaustiveMatch.
#[test]
fn match_tuple_wrapped_refutable_sub_pattern_is_not_exhaustive() {
    typecheck_fails(
        "type List = Empty | Cons((Int, List))
         def f(l: List): Int :=
             match l {
                 List#Cons((x, List#Empty)) => x,
                 List#Empty => 0,
             }
         def main(): Int := 0",
    );
}

/// Three inner variants: all three arms needed.  Missing one → error.
#[test]
fn match_nested_three_inner_variants_partial_is_error() {
    typecheck_fails(
        "type Shape = Circle | Square | Triangle
         type Wrapper = W(Shape)
         def f(w: Wrapper): Int :=
             match w {
                 Wrapper#W(Shape#Circle) => 1,
                 Wrapper#W(Shape#Square) => 2,
             }
         def main(): Int := 0",
    );
}

/// Three inner variants: all three arms present → exhaustive.
#[test]
fn match_nested_three_inner_variants_all_covered_is_exhaustive() {
    let result = run_hir(
        "type Shape = Circle | Square | Triangle
         type Wrapper = W(Shape)
         def f(w: Wrapper): Int :=
             match w {
                 Wrapper#W(Shape#Circle)   => 1,
                 Wrapper#W(Shape#Square)   => 2,
                 Wrapper#W(Shape#Triangle) => 3,
             }
         def main(): Int := f(Wrapper#W(Shape#Triangle))",
    );
    assert_eq!(result, Expression::Int(3));
}

/// Duplicate inner variant under the same outer variant is an error.
#[test]
fn match_nested_duplicate_inner_variant_is_error() {
    typecheck_fails(
        "type Inner = X | Y
         type Outer = A(Inner) | B
         def f(o: Outer): Int :=
             match o {
                 Outer#A(Inner#X) => 1,
                 Outer#A(Inner#X) => 2,
                 Outer#B => 3,
             }
         def main(): Int := 0",
    );
}

/// Mixing irrefutable wildcard and nested variant arms: the wildcard arm
/// already covers the outer variant, so the nested arm before it is valid too.
#[test]
fn match_nested_variant_then_wildcard_typechecks() {
    let result = run_hir(
        "type Inner = X | Y
         type Outer = A(Inner) | B
         def f(o: Outer): Int :=
             match o {
                 Outer#A(Inner#X) => 1,
                 Outer#A(_)       => 2,
                 Outer#B          => 3,
             }
         def main(): Int := f(Outer#A(Inner#Y))",
    );
    assert_eq!(result, Expression::Int(2));
}

// ─── Nested enum-variant patterns in payload position ─────────────────

/// basic nested variant pattern with a wildcard catch-all type-checks.
#[test]
fn match_nested_variant_with_catchall_typechecks() {
    run_hir(
        "type Inner = A | B(Int)
         type Outer = Wrap(Inner)
         def main(): Int :=
             match Outer#Wrap(Inner#B(42)) {
                 Outer#Wrap(Inner#A)    => 0,
                 Outer#Wrap(Inner#B(n)) => n,
                 _                     => -1,
             }",
    );
}

/// the arm with the matching inner pattern is actually selected.
#[test]
fn match_nested_variant_selects_correct_arm() {
    let result = run_hir(
        "type Color = Red | Green | Blue
         type Box = Colored(Color)
         def main(): Int :=
             match Box#Colored(Color#Green) {
                 Box#Colored(Color#Red)   => 1,
                 Box#Colored(Color#Green) => 2,
                 Box#Colored(Color#Blue)  => 3,
                 _                        => 0,
             }",
    );
    assert_eq!(result, lang::ir_types::typed_hir::Expression::Int(2));
}

/// nested pattern extracts a binding from the inner variant's payload.
#[test]
fn match_nested_variant_binds_inner_payload() {
    let result = run_hir(
        "type Inner = A | B(Int)
         type Outer = Wrap(Inner)
         def main(): Int :=
             match Outer#Wrap(Inner#B(99)) {
                 Outer#Wrap(Inner#A)    => 0,
                 Outer#Wrap(Inner#B(n)) => n,
                 _                     => -1,
             }",
    );
    assert_eq!(result, lang::ir_types::typed_hir::Expression::Int(99));
}

/// wildcard catch-all is reached when the inner pattern does not match.
#[test]
fn match_nested_variant_falls_through_to_catchall() {
    let result = run_hir(
        "type Inner = A | B(Int)
         type Outer = Wrap(Inner)
         def main(): Int :=
             match Outer#Wrap(Inner#A) {
                 Outer#Wrap(Inner#B(n)) => n,
                 _                     => -1,
             }",
    );
    assert_eq!(result, lang::ir_types::typed_hir::Expression::Int(-1));
}

/// bare tag sub-pattern resolves against the expected inner type.
#[test]
fn match_nested_tag_pattern_works() {
    let result = run_hir(
        "type Inner = A | B(Int)
         type Outer = Wrap(Inner)
         def main(): Int :=
             match Outer#Wrap(Inner#B(7)) {
                 Outer#Wrap(#A)    => 0,
                 Outer#Wrap(#B(n)) => n,
                 _                 => -1,
             }",
    );
    assert_eq!(result, lang::ir_types::typed_hir::Expression::Int(7));
}

/// a nested variant pattern whose enum doesn't match the payload type is an
/// error.
#[test]
fn match_nested_variant_wrong_type_is_error() {
    typecheck_fails(
        "type Inner = A | B(Int)
         type Other = X
         type Outer = Wrap(Inner)
         def main(): Int :=
             match Outer#Wrap(Inner#A) {
                 Outer#Wrap(Other#X) => 1,
                 _                   => 0,
             }",
    );
}

/// a tuple pattern matched against a non-tuple type is a type error.
#[test]
fn match_tuple_pattern_against_non_tuple_is_error() {
    typecheck_fails(
        "type Shape = Circle(Int) | Point
         def main(): Int :=
             match Shape#Circle(5) {
                 (a, b) => a,
             }",
    );
}

// ─── Int / Bool literal patterns ──────────────────────────────────────

/// match on Int with literal patterns — requires a wildcard/binding catch-all.
#[test]
fn match_int_literal_exhaustive_with_wildcard() {
    run_hir(
        "def main(): Int :=
             match 5 {
                 1 => 10,
                 2 => 20,
                 _ => 99,
             }",
    );
}

/// literal pattern that matches the scrutinee returns the correct arm.
#[test]
fn match_int_literal_correct_arm_selected() {
    let result = run_hir(
        "def main(): Int :=
             match 2 {
                 1 => 100,
                 2 => 200,
                 _ => 999,
             }",
    );
    assert_eq!(result, lang::ir_types::typed_hir::Expression::Int(200));
}

/// negative integer literal pattern works.
#[test]
fn match_int_literal_negative() {
    let result = run_hir(
        "def main(): Int :=
             match (-3) {
                 -3 => 1,
                 _  => 0,
             }",
    );
    assert_eq!(result, lang::ir_types::typed_hir::Expression::Int(1));
}

/// Int match without a wildcard/binding catch-all is a type error.
#[test]
fn match_int_literal_non_exhaustive_is_error() {
    typecheck_fails(
        "def main(): Int :=
             match 5 {
                 1 => 10,
                 2 => 20,
             }",
    );
}

/// duplicate Int literal pattern is a type error.
#[test]
fn match_int_duplicate_literal_is_error() {
    typecheck_fails(
        "def main(): Int :=
             match 5 {
                 1 => 10,
                 1 => 20,
                 _ => 0,
             }",
    );
}

/// Bool match covering both true and false is exhaustive.
#[test]
fn match_bool_exhaustive() {
    run_hir(
        "def main(): Int :=
             match true {
                 true  => 1,
                 false => 0,
             }",
    );
}

/// Bool match selects the correct arm.
#[test]
fn match_bool_correct_arm() {
    let result = run_hir(
        "def main(): Int :=
             match false {
                 true  => 1,
                 false => 0,
             }",
    );
    assert_eq!(result, lang::ir_types::typed_hir::Expression::Int(0));
}

/// Bool match missing one arm is non-exhaustive.
#[test]
fn match_bool_non_exhaustive_is_error() {
    typecheck_fails(
        "def main(): Int :=
             match true {
                 true => 1,
             }",
    );
}

/// Int literal pattern against a Bool scrutinee is a type error.
#[test]
fn match_int_pattern_on_bool_scrutinee_is_error() {
    typecheck_fails(
        "def main(): Int :=
             match true {
                 1 => 10,
                 _ => 0,
             }",
    );
}

/// Bool literal pattern against an Int scrutinee is a type error.
#[test]
fn match_bool_pattern_on_int_scrutinee_is_error() {
    typecheck_fails(
        "def main(): Int :=
             match 5 {
                 true => 1,
                 _    => 0,
             }",
    );
}
