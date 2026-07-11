//! Step 1 of the partial-application feature (Calculus §4.5): `TyKind::Hole`,
//! under-saturated `Ty::App`, and `constructor_kind`. Pure kinding — no
//! surface syntax is wired up yet.
use crate::compiler::context::CompileCtx;
use crate::ir_types::hhir::ProgramModule;
use crate::ir_types::qhir;
use crate::lang::types::AdtRef;
use crate::lang::types::Kind;

/// Register a binary enum `Duo<A, B>` and hand back `(ctx, er)`.
fn ctx_with_duo() -> (CompileCtx<'static>, AdtRef<'static>) {
    let mut ctx = CompileCtx::initial();
    let pm =
        ProgramModule::parse_stub(&mut ctx, "type Duo<A, B> = Two(A, B)\ndef main(): Int := 0")
            .expect("parse stub");
    let _ = qhir::Program::combine(&mut ctx, vec![pm]);
    let er = ctx
        .all_enums()
        .find(|&e| ctx.get_enum(e).name == "Duo")
        .expect("Duo registered");
    (ctx, er)
}

#[test]
fn saturated_app_is_owned() {
    let (mut ctx, er) = ctx_with_duo();
    let int = ctx.types.int;
    let sat = ctx.intern_app(er, vec![int, int], vec![]);
    assert_eq!(ctx.constructor_kind(sat), Kind::Owned);
    assert!(!sat.has_hole());
}

#[test]
fn trailing_undersaturation_currys() {
    // `Duo<Int>` ≡ `Duo<Int, _>` : Owned -> Owned.
    let (mut ctx, er) = ctx_with_duo();
    let int = ctx.types.int;
    let partial = ctx.intern_app(er, vec![int], vec![]);
    let oo = ctx.intern_kind(Kind::Owned, Kind::Owned);
    assert_eq!(ctx.constructor_kind(partial), oo);
    assert!(!partial.has_hole()); // implicit trailing hole, no `Hole` node
}

#[test]
fn interior_hole_has_same_kind_as_trailing() {
    // `Duo<_, Int>` : Owned -> Owned — one hole, like `Duo<Int>`.
    let (mut ctx, er) = ctx_with_duo();
    let int = ctx.types.int;
    let h0 = ctx.hole_ty(0);
    let interior = ctx.intern_app(er, vec![h0, int], vec![]);
    let oo = ctx.intern_kind(Kind::Owned, Kind::Owned);
    assert_eq!(ctx.constructor_kind(interior), oo);
    assert!(interior.has_hole());
}

#[test]
fn two_holes_give_a_binary_arrow() {
    // `Duo<_, _>` and the fully under-saturated `Duo` are both
    // Owned -> Owned -> Owned.
    let (mut ctx, er) = ctx_with_duo();
    let h0 = ctx.hole_ty(0);
    let h1 = ctx.hole_ty(1);
    let both = ctx.intern_app(er, vec![h0, h1], vec![]);
    let oo = ctx.intern_kind(Kind::Owned, Kind::Owned);
    let ooo = ctx.intern_kind(Kind::Owned, oo);
    assert_eq!(ctx.constructor_kind(both), ooo);
    let empty = ctx.intern_app(er, vec![], vec![]);
    assert_eq!(ctx.constructor_kind(empty), ooo);
}

#[test]
fn a_hole_contributes_no_params_or_regions() {
    let (mut ctx, _er) = ctx_with_duo();
    let h = ctx.hole_ty(0);
    assert!(!h.has_param());
    let mut ps = Vec::new();
    h.collect_params(&mut ps);
    assert!(ps.is_empty());
    let mut rs = Vec::new();
    h.free_regions(&mut rs);
    assert!(rs.is_empty());
}

// ---- Step 2: β-reduction of a partial application in `subst` (Calculus §4.5).

use crate::lang::types::TypeParamId;
use crate::passes::type_ast::generics::Subst;
use crate::passes::type_ast::generics::subst;

#[test]
fn subst_beta_reduces_interior_hole() {
    // `F := Duo<_, E>`, then `F<A>` β-reduces to `Duo<A, E>`, with the fixed
    // instance param `E` resolved through the same mapping.
    let (mut ctx, er) = ctx_with_duo();
    let int = ctx.types.int;
    let boolean = ctx.types.bool;
    let (f, e, a) = (TypeParamId(9000), TypeParamId(9001), TypeParamId(9002));
    let param_e = ctx.param_ty(e);
    let param_a = ctx.param_ty(a);
    let h0 = ctx.hole_ty(0);
    let abstraction = ctx.intern_app(er, vec![h0, param_e], vec![]); // Duo<_, E>
    let fa = ctx.param_app_ty(f, vec![param_a]); // F<A>

    let mut m = Subst::new();
    m.insert(f, abstraction);
    m.insert(a, int);
    m.insert(e, boolean);

    let got = subst(&mut ctx, fa, &m);
    let want = ctx.intern_app(er, vec![int, boolean], vec![]); // Duo<Int, Bool>
    assert_eq!(got, want);
    assert!(!got.has_hole());
}

#[test]
fn subst_beta_fills_two_holes_positionally() {
    // `F := Duo<_, _>`, then `F<A, B>` β-reduces to `Duo<A, B>` (hole `i` <- arg
    // `i`).
    let (mut ctx, er) = ctx_with_duo();
    let int = ctx.types.int;
    let boolean = ctx.types.bool;
    let (f, a, b) = (TypeParamId(9100), TypeParamId(9101), TypeParamId(9102));
    let param_a = ctx.param_ty(a);
    let param_b = ctx.param_ty(b);
    let (h0, h1) = (ctx.hole_ty(0), ctx.hole_ty(1));
    let abstraction = ctx.intern_app(er, vec![h0, h1], vec![]); // Duo<_, _>
    let fab = ctx.param_app_ty(f, vec![param_a, param_b]); // F<A, B>

    let mut m = Subst::new();
    m.insert(f, abstraction);
    m.insert(a, int);
    m.insert(b, boolean);

    let got = subst(&mut ctx, fab, &m);
    let want = ctx.intern_app(er, vec![int, boolean], vec![]); // Duo<Int, Bool>
    assert_eq!(got, want);
}

#[test]
fn subst_bare_constructor_shorthand_still_reduces() {
    // Regression: `F := Duo` (bare `Enum`, the all-holes shorthand) still turns
    // `F<A, B>` into `Duo<A, B>` — the pre-existing unary path is unchanged.
    let (mut ctx, er) = ctx_with_duo();
    let int = ctx.types.int;
    let boolean = ctx.types.bool;
    let (f, a, b) = (TypeParamId(9200), TypeParamId(9201), TypeParamId(9202));
    let param_a = ctx.param_ty(a);
    let param_b = ctx.param_ty(b);
    let bare = ctx.enum_ty(er); // `Enum(er)`
    let fab = ctx.param_app_ty(f, vec![param_a, param_b]);

    let mut m = Subst::new();
    m.insert(f, bare);
    m.insert(a, int);
    m.insert(b, boolean);

    let got = subst(&mut ctx, fab, &m);
    let want = ctx.intern_app(er, vec![int, boolean], vec![]);
    assert_eq!(got, want);
}
