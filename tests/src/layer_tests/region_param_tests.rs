//! Step 8a — region (lifetime) parameter plumbing (Calculus §8.4, §8.10).
//!
//! Declarations may carry region parameters (`def f<'r>(...)`, `type Ref<'r,
//! a>`) mixed with type parameters, and functions may carry `where 'r >= 's`
//! outlives constraints. This step is *structural*: regions resolve against the
//! declaring item's scope and are stored on the IR, but nothing is enforced yet
//! — the escape check and the outlives solver arrive in Step 8b. Regions are
//! still erased by monomorphisation, so region-parametric code compiles and
//! runs unchanged.

use lang::lang::types::Region;
use lang::lang::types::TyKind;

use crate::common::parse;
use crate::common::parse_fails;
use crate::common::typecheck;

// ── region parameters are declared and resolve in scope
// ───────────────────────

#[test]
fn declared_region_parameter_resolves_on_a_reference() {
    let pm = parse("def f<'r>(x: &'r Int): Int := 0");
    let func = &pm.functions[0];
    assert_eq!(func.region_params.len(), 1);
    assert_eq!(func.region_params[0].name, "r");
    // the parameter type is `&'r Int`, carrying the declared region.
    let pty = func.parameters[0].ty;
    let TyKind::Ref(Region::Var(rv), _) = pty.kind() else {
        panic!("expected a reference type, got {:?}", pty.kind());
    };
    assert_eq!(*rv, func.region_params[0].region);
}

#[test]
fn region_and_type_parameters_mix_in_one_list() {
    let pm = parse("def f<'r, T>(x: &'r T): Int := 0");
    let func = &pm.functions[0];
    assert_eq!(func.region_params.len(), 1);
    assert_eq!(func.type_params.len(), 1);
    assert_eq!(func.region_params[0].name, "r");
    assert_eq!(func.type_params[0].name, "T");
}

#[test]
fn region_ascription_uses_a_declared_region() {
    typecheck("def f<'r>(x: Int @ 'r): Int @ 'r := x \n def main(): Int := 0");
}

#[test]
fn region_parametric_function_definition_type_checks() {
    // a region-parametric helper type-checks; regions are erased by
    // monomorphisation. (Calling it with an *elided* borrow needs region
    // inference at the call site, which arrives in Step 8b — so `main` does not
    // call it here.)
    typecheck("def id<'r>(x: &'r Int): Int := 0 \n def main(): Int := 0");
}

// ── undeclared lifetimes are rejected
// ─────────────────────────────────────────

#[test]
fn undeclared_lifetime_on_reference_is_rejected() {
    parse_fails("def f(x: &'r Int): Int := 0");
}

#[test]
fn undeclared_lifetime_on_ascription_is_rejected() {
    parse_fails("def f(x: Int @ 'r): Int := 0");
}

#[test]
fn undeclared_lifetime_in_where_clause_is_rejected() {
    parse_fails("def f<'r>(x: &'r Int): Int where 'r >= 's := 0");
}

// ── `where 'r >= 's` outlives constraints parse and are stored
// ────────────────

#[test]
fn where_clause_parses_and_is_stored() {
    let pm = parse(
        "def longest<'r, 's>(x: &'r Int, y: &'s Int): &'r Int where 'r >= 's := x \n \
         def main(): Int := 0",
    );
    let func = pm
        .functions
        .iter()
        .find(|f| f.region_params.len() == 2)
        .expect("longest");
    assert_eq!(func.where_constraints.len(), 1);
    let c = func.where_constraints[0];
    // `'r >= 's`: 'r (longer) outlives 's (shorter).
    assert_eq!(c.longer, Region::Var(func.region_params[0].region));
    assert_eq!(c.shorter, Region::Var(func.region_params[1].region));
}

#[test]
fn multiple_where_constraints_parse() {
    let pm = parse(
        "def f<'r, 's, 't>(x: &'r Int): Int where 'r >= 's, 's >= 't := 0 \n \
         def main(): Int := 0",
    );
    let func = pm
        .functions
        .iter()
        .find(|f| f.region_params.len() == 3)
        .expect("f");
    assert_eq!(func.where_constraints.len(), 2);
}

#[test]
fn static_outlives_a_declared_region_in_where_clause() {
    typecheck("def f<'r>(x: &'r Int): Int where 'static >= 'r := 0 \n def main(): Int := 0");
}

// ── enums may carry region parameters
// ─────────────────────────────────────────

#[test]
fn enum_region_parameter_resolves_in_payloads() {
    let pm = parse("type Ref<'r, a> = Mk(&'r a) \n def main(): Int := 0");
    // the declaration parses and the `&'r a` payload resolves against the
    // enum's own region scope.
    assert!(!pm.functions.is_empty());
}
