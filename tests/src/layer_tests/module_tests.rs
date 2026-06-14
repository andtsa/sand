//! Module system (Step M): lexical resolution + `use` imports.
//!
//! Unqualified names resolve in the caller's own module → explicit `use` → glob
//! `use` → prelude (`core`). There is no global fallback: a name in another
//! *user* module must be qualified (`mod::name`) or `use`d.

use lang::interpreter::mir::MirValue;

use crate::common::run_mir;
use crate::common::typecheck_fails;

// ── no global fallback
// ────────────────────────────────────────────────────────

#[test]
fn unqualified_cross_module_function_is_rejected() {
    typecheck_fails(
        "module lib;
         def helper(): Int := 7
         module app;
         def main(): Int := helper()",
    );
}

#[test]
fn qualified_cross_module_function_works() {
    assert_eq!(
        run_mir(
            "module lib;
             def helper(): Int := 7
             module app;
             def main(): Int := lib::helper()"
        ),
        MirValue::Int(7)
    );
}

#[test]
fn unqualified_cross_module_type_is_rejected() {
    typecheck_fails(
        "module colors;
         type Light = Red | Yellow | Green
         module app;
         def main(): Int := match Light#Red { _ => 0 }",
    );
}

// ── prelude (core) is auto-imported
// ───────────────────────────────────────────

#[test]
fn core_functions_are_callable_unqualified() {
    // `abs` lives in the `core` module; the prelude makes it reachable with no
    // `use` and no qualifier.
    assert_eq!(run_mir("def main(): Int := abs(0 - 5)"), MirValue::Int(5));
}

// ── `use module::name` (explicit)
// ─────────────────────────────────────────────

#[test]
fn use_imports_a_function() {
    assert_eq!(
        run_mir(
            "module lib;
             def helper(): Int := 7
             module app;
             use lib::helper;
             def main(): Int := helper()"
        ),
        MirValue::Int(7)
    );
}

#[test]
fn use_imports_a_type() {
    let val = run_mir(
        "module colors;
         type Light = Red | Yellow | Green
         module app;
         use colors::Light;
         def main(): Light := Light#Yellow",
    );
    match val {
        MirValue::EnumVariant { variant_idx, .. } => assert_eq!(variant_idx, 1),
        other => panic!("expected EnumVariant, got {:?}", other),
    }
}

#[test]
fn use_of_one_name_does_not_import_others() {
    // importing `helper` does not bring in `other` from the same module.
    typecheck_fails(
        "module lib;
         def helper(): Int := 7
         def other(): Int := 9
         module app;
         use lib::helper;
         def main(): Int := other()",
    );
}

// ── `use module::*` (glob)
// ────────────────────────────────────────────────────

#[test]
fn glob_use_imports_all_functions() {
    assert_eq!(
        run_mir(
            "module lib;
             def a(): Int := 3
             def b(): Int := 4
             module app;
             use lib::*;
             def main(): Int := a() + b()"
        ),
        MirValue::Int(7)
    );
}

#[test]
fn own_module_shadows_an_import() {
    // a local `helper` wins over an imported one (own items have priority).
    assert_eq!(
        run_mir(
            "module lib;
             def helper(): Int := 7
             module app;
             use lib::helper;
             def helper(): Int := 1
             def main(): Int := helper()"
        ),
        MirValue::Int(1)
    );
}

// ── malformed `use`
// ───────────────────────────────────────────────────────────

#[test]
fn use_of_unknown_module_is_rejected_on_reference() {
    // an unresolvable `use` errors when the imported name is referenced
    // (validate-on-use; an *unused* bad import is currently lenient).
    typecheck_fails(
        "module app;
         use nonexistent::foo;
         def main(): Int := foo()",
    );
}

#[test]
fn a_plain_use_with_no_item_parses_but_resolves_nothing() {
    // `use lib;` (single segment, no item or glob) is malformed.
    typecheck_fails(
        "module lib;
         def helper(): Int := 7
         module app;
         use lib;
         def main(): Int := 0",
    );
}
