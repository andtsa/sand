//! Tests for nominal enum types
//!
//! Covers: declaration, qualified constructors, bare tags in check position,
//! equality comparisons, and error cases.

use lang::interpreter::mir::MirValue;

use crate::common::*;

// ── declaration and construction ─────────────────────────────────────────────

/// A simple enum declaration compiles without error.
#[test]
fn enum_declaration_compiles() {
    run_mir(
        "type Light = Red | Yellow | Green
         def main(): Light := Light#Red",
    );
}

/// A qualified constructor returns the correct variant, and two variants of
/// the same enum share one `EnumRef`.
///
/// `EnumRef` is an arena handle with pointer identity, so the two variants
/// must be produced by a **single** compilation for their handles to be
/// comparable — hence the tuple-returning program rather than two `run_mir`
/// calls.
#[test]
fn qualified_constructor_first_variant() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def main(): (Light, Light) := (Light#Red, Light#Yellow)",
    );
    let MirValue::Tuple(elems) = val else {
        panic!("expected Tuple, got {:?}", val);
    };
    let er = match &elems[0] {
        MirValue::EnumVariant {
            enum_ref,
            variant_idx,
            ..
        } => {
            assert_eq!(*variant_idx, 0, "Red should be variant index 0");
            *enum_ref
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    };
    match &elems[1] {
        MirValue::EnumVariant {
            enum_ref,
            variant_idx,
            ..
        } => {
            assert_eq!(*variant_idx, 1, "Yellow should be variant index 1");
            assert_eq!(*enum_ref, er, "Both come from the same enum type");
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// Third variant gets index 2.
#[test]
fn qualified_constructor_third_variant() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def main(): Light := Light#Green",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            assert_eq!(variant_idx, 2, "Green should be variant index 2");
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// Type is inferred from a qualified constructor (no annotation needed).
#[test]
fn type_inferred_from_qualified_constructor() {
    run_mir(
        "type Light = Red | Yellow | Green
         def main(): Light := {
             let x = Light#Red;
             x
         }",
    );
}

/// A bare tag is resolved when the variable has an enum type annotation.
#[test]
fn bare_tag_resolved_by_annotation() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def main(): Light := {
             let x: Light = #Green;
             x
         }",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            assert_eq!(variant_idx, 2);
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// A bare tag in return position is resolved by the declared return type.
#[test]
fn bare_tag_resolved_by_return_type() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def main(): Light := #Yellow",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            assert_eq!(variant_idx, 1);
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// A bare tag in an if-then-else return position is resolved by return type.
#[test]
fn bare_tag_resolved_in_if_else() {
    let val = run_mir(
        "type Light = Red | Yellow | Green
         def f(b: Bool): Light := if b then #Red else #Green
         def main(): Light := f(true)",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            assert_eq!(variant_idx, 0); // Red
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

// ── equality ─────────────────────────────────────────────────────────────────

/// Two equal enum constructors compare as equal.
#[test]
fn same_variant_is_equal() {
    assert_eq!(
        run_mir(
            "type Light = Red | Yellow | Green
             def main(): Bool := {
                 let x: Light = Light#Red;
                 x == Light#Red
             }"
        ),
        MirValue::Bool(true)
    );
}

/// Two different variants compare as not equal.
#[test]
fn different_variants_are_not_equal() {
    assert_eq!(
        run_mir(
            "type Light = Red | Yellow | Green
             def main(): Bool := {
                 let x: Light = Light#Red;
                 x == Light#Green
             }"
        ),
        MirValue::Bool(false)
    );
}

/// `!=` works for different variants.
#[test]
fn different_variants_ne() {
    assert_eq!(
        run_mir(
            "type Light = Red | Yellow | Green
             def main(): Bool := {
                 let x: Light = Light#Red;
                 x != Light#Green
             }"
        ),
        MirValue::Bool(true)
    );
}

/// Bare tag in equality context is resolved against the other side's type.
#[test]
fn bare_tag_equality_with_qualified_left() {
    assert_eq!(
        run_mir(
            "type Light = Red | Yellow | Green
             def main(): Bool := {
                 let x: Light = Light#Red;
                 x == #Red
             }"
        ),
        MirValue::Bool(true)
    );
}

/// Bare tag inequality with a qualified left side.
#[test]
fn bare_tag_inequality_with_qualified_left() {
    assert_eq!(
        run_mir(
            "type Light = Red | Yellow | Green
             def main(): Bool := {
                 let x: Light = Light#Red;
                 x != #Green
             }"
        ),
        MirValue::Bool(true)
    );
}

// ── nominal typing
// ────────────────────────────────────────────────────────────

/// Two enums with the same variant names are distinct types.
#[test]
fn two_enums_same_variants_are_distinct_types() {
    typecheck_fails(
        "type Light = Red | Green
         type Signal = Red | Green
         def f(x: Light): Signal := x",
    );
}

// ── error cases
// ───────────────────────────────────────────────────────────────

/// Using an unknown variant on a known enum is a compile error.
#[test]
fn unknown_variant_is_error() {
    qualify_fails(
        "type Light = Red | Yellow | Green
         def main(): Light := Light#Purple",
    );
}

/// Using an unknown type name in a constructor is a compile error.
#[test]
fn unknown_type_in_constructor_is_error() {
    qualify_fails(
        "type Color = Red | Blue
         def main(): Int := { let x = Bogus#Thing; 0 }",
    );
}

/// A bare tag without any type context is a type error.
#[test]
fn bare_tag_without_context_is_type_error() {
    typecheck_fails(
        "def main(): Int := {
             let x = #Red;
             0
         }",
    );
}

/// Referencing an undefined enum type in a return annotation is an error.
#[test]
fn undefined_enum_type_in_annotation_is_error() {
    typecheck_fails("def main(): Nonexistent := 0");
}

// ── ad-hoc tag union types ───────────────────────────────────────────────────

/// A function may declare an ad-hoc return type `#gt | #lt | #eq`.
#[test]
fn adhoc_tag_union_return_type_compiles() {
    run_mir(
        "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
             if a > b then #gt
             else if a < b then #lt
             else #eq
         def main(): #gt | #lt | #eq := cmp(2, 3)",
    );
}

/// Ad-hoc tags are interned: the same tag set used in two different functions
/// produces the same type, so values can flow between them.
#[test]
fn adhoc_tag_union_structural_equivalence() {
    let val = run_mir(
        "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
             if a > b then #gt
             else if a < b then #lt
             else #eq
         def main(): #gt | #lt | #eq := cmp(2, 2)",
    );
    // eq is alphabetically first, so variant_idx == 0
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            assert_eq!(variant_idx, 0, "#eq should be index 0 (alphabetical)");
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// Variant indices are assigned alphabetically, independent of write order.
#[test]
fn adhoc_tag_union_variant_indices_are_alphabetical() {
    // Write order: #gt | #lt | #eq.  Alphabetical: eq=0, gt=1, lt=2.
    let gt = run_mir(
        "def f(): #gt | #lt | #eq := #gt
         def main(): #gt | #lt | #eq := f()",
    );
    let lt = run_mir(
        "def f(): #gt | #lt | #eq := #lt
         def main(): #gt | #lt | #eq := f()",
    );
    let eq = run_mir(
        "def f(): #gt | #lt | #eq := #eq
         def main(): #gt | #lt | #eq := f()",
    );
    match gt {
        MirValue::EnumVariant { variant_idx, .. } => assert_eq!(variant_idx, 1),
        other => panic!("expected EnumVariant for #gt, got {:?}", other),
    }
    match lt {
        MirValue::EnumVariant { variant_idx, .. } => assert_eq!(variant_idx, 2),
        other => panic!("expected EnumVariant for #lt, got {:?}", other),
    }
    match eq {
        MirValue::EnumVariant { variant_idx, .. } => assert_eq!(variant_idx, 0),
        other => panic!("expected EnumVariant for #eq, got {:?}", other),
    }
}

/// Two ad-hoc tags that are equal compare as `true`.
#[test]
fn adhoc_tag_equality_true() {
    assert_eq!(
        run_mir(
            "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
                 if a > b then #gt else if a < b then #lt else #eq
             def main(): Bool := cmp(2, 2) == #eq"
        ),
        MirValue::Bool(true)
    );
}

/// Two ad-hoc tags that differ compare as `false`.
#[test]
fn adhoc_tag_equality_false() {
    assert_eq!(
        run_mir(
            "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
                 if a > b then #gt else if a < b then #lt else #eq
             def main(): Bool := cmp(1, 2) == #gt"
        ),
        MirValue::Bool(false)
    );
}

/// `!=` on ad-hoc tags.
#[test]
fn adhoc_tag_inequality() {
    assert_eq!(
        run_mir(
            "def cmp(a: Int, b: Int): #gt | #lt | #eq :=
                 if a > b then #gt else if a < b then #lt else #eq
             def main(): Bool := cmp(1, 2) != #gt"
        ),
        MirValue::Bool(true)
    );
}

/// A `let` binding annotated with an ad-hoc tag-union type.
#[test]
fn adhoc_tag_in_let_annotation() {
    let val = run_mir(
        "def main(): #ok | #err := {
             let r: #ok | #err = #ok;
             r
         }",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            // err=0, ok=1 (alphabetical)
            assert_eq!(variant_idx, 1, "#ok should be index 1");
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// A single-tag ad-hoc type (a singleton) is valid.
#[test]
fn adhoc_single_tag() {
    let val = run_mir("def main(): #unit_tag := #unit_tag");
    match val {
        MirValue::EnumVariant { variant_idx, .. } => assert_eq!(variant_idx, 0),
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// Write order does not matter — `#lt | #gt | #eq` and `#gt | #lt | #eq` are
/// the same structural type.
#[test]
fn adhoc_tag_write_order_irrelevant() {
    // First function uses one order; second uses another — values must flow.
    assert_eq!(
        run_mir(
            "def f(): #lt | #gt | #eq := #gt
             def main(): Bool := f() == #gt"
        ),
        MirValue::Bool(true)
    );
}

/// Using a tag not in the declared set is a type error.
#[test]
fn adhoc_tag_wrong_variant_is_error() {
    typecheck_fails("def main(): #ok | #err := #bogus");
}

// ── cross-module enum types
// ───────────────────────────────────────────────────

/// A type declared in one module section is accessible from another via
/// `mod::TypeName` in a return type annotation.
#[test]
fn cross_module_qualified_type_annotation() {
    let val = run_mir(
        "module colors;
         type Light = Red | Yellow | Green

         module app;
         def main(): colors::Light := colors::Light#Red",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            assert_eq!(variant_idx, 0, "Red should be index 0");
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// `mod::Type#Variant` constructor works and returns the correct index.
#[test]
fn cross_module_external_constructor() {
    let val = run_mir(
        "module colors;
         type Light = Red | Yellow | Green

         module app;
         def main(): colors::Light := colors::Light#Green",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            assert_eq!(variant_idx, 2, "Green should be index 2");
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// A `let` binding with a cross-module type annotation and a bare tag.
#[test]
fn cross_module_bare_tag_resolved_by_annotation() {
    let val = run_mir(
        "module colors;
         type Light = Red | Yellow | Green

         module app;
         def main(): colors::Light := {
             let x: colors::Light = #Yellow;
             x
         }",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            assert_eq!(variant_idx, 1, "Yellow should be index 1");
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// Equality of cross-module constructed values works.
#[test]
fn cross_module_enum_equality() {
    assert_eq!(
        run_mir(
            "module colors;
             type Light = Red | Yellow | Green

             module app;
             def main(): Bool := {
                 let x: colors::Light = colors::Light#Red;
                 x == colors::Light#Red
             }"
        ),
        MirValue::Bool(true)
    );
}

/// Inequality of two different cross-module variants works.
#[test]
fn cross_module_enum_inequality() {
    assert_eq!(
        run_mir(
            "module colors;
             type Light = Red | Yellow | Green

             module app;
             def main(): Bool := {
                 let x: colors::Light = colors::Light#Red;
                 x != colors::Light#Green
             }"
        ),
        MirValue::Bool(true)
    );
}

/// A function in the enum's own module can use the local type name,
/// while a caller in another module uses the qualified form.
#[test]
fn cross_module_function_returns_enum() {
    let val = run_mir(
        "module colors;
         type Light = Red | Yellow | Green
         def make_red(): Light := Light#Red

         module app;
         def main(): colors::Light := colors::make_red()",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => {
            assert_eq!(variant_idx, 0, "Red should be index 0");
        }
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

/// Referencing a non-existent module in a qualified type is an error.
#[test]
fn unknown_module_in_qualified_type_is_error() {
    typecheck_fails("def main(): no_such_module::Light := 0");
}

/// Referencing an unknown type in a known module is an error.
#[test]
fn unknown_type_in_known_module_is_error() {
    typecheck_fails(
        "module colors;
         type Light = Red | Yellow | Green

         module app;
         def main(): colors::Bogus := 0",
    );
}

/// An `external_constructor_expr` with a non-existent module is a qualify
/// error.
#[test]
fn unknown_module_in_external_constructor_is_error() {
    qualify_fails("def main(): Int := { let _x = no_such::Type#Var; 0 }");
}

/// An `external_constructor_expr` with an unknown type name is a qualify
/// error.
#[test]
fn unknown_type_in_external_constructor_is_error() {
    qualify_fails(
        "module colors;
         type Light = Red | Yellow | Green

         module app;
         def main(): Int := { let _x = colors::Bogus#Red; 0 }",
    );
}
