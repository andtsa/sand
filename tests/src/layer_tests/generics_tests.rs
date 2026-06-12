//! Step 1 — type parameter syntax and structural plumbing.
//!
//! These tests cover *declaring* generic functions and enums: the grammar
//! accepts type parameters, they are threaded through the IRs, and a `T` in a
//! signature/body resolves to an opaque `Ty::Param`. Generic *instantiation*
//! (calling a generic function with concrete types) is Step 2, so these tests
//! only define generic items — calls are not exercised here.

use lang::ir_types::typed_hir::Expression;

use crate::common::parse;
use crate::common::run_hir;
use crate::common::run_mir_as_expr;
use crate::common::typecheck;
use crate::common::typecheck_fails;

#[test]
fn monomorphisation_removes_all_type_params_and_specialises() {
    // after compilation (which runs mono), no function carries type parameters,
    // and `id` has been specialised once per instantiation (`id$Int`,
    // `id$Bool`), distinct from the original generic name.
    let (ctx, prog) = typecheck(
        "def id<T>(x: T): T := x \n \
         def main(): Int := { let a: Int = id(5); let b: Bool = id(true); a }",
    );
    assert!(
        prog.functions.values().all(|f| f.type_params.is_empty()),
        "a function still has type parameters after monomorphisation"
    );
    let names: Vec<String> = prog
        .functions
        .values()
        .map(|f| ctx.original_fun_name(f.name))
        .collect();
    assert!(
        names.iter().any(|n| n == "id$Int"),
        "missing id$Int: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "id$Bool"),
        "missing id$Bool: {names:?}"
    );
}

/// Run a generic program through both interpreters (which execute the
/// monomorphised program) and assert they agree, returning the result.
fn run_both(src: &str) -> Expression<'static> {
    let hir = run_hir(src);
    let mir = run_mir_as_expr(src);
    assert_eq!(hir, mir, "HIR and MIR disagree for:\n  {src}");
    hir
}

// ── parsing: type parameters are captured on the declaration
// ──────────────────

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

// ── type checking: a parameter is opaque but self-consistent
// ──────────────────

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

// ── type checking: distinct parameters are distinct opaque types
// ──────────────

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

// ── resolution: the declaration is what makes `T` a type
// ──────────────────────

#[test]
fn undeclared_type_param_is_unknown_type() {
    // without `<T>`, `T` is an unknown enum type, not a parameter.
    typecheck_fails("def f(x: T): T := x \n def main(): Int := 0");
}

#[test]
fn type_param_shadows_same_named_enum() {
    // inside `f`, `T` is the parameter, not the enum `T`; `x : T(param)` matches
    // the return `T(param)`, so this type-checks.
    typecheck("type T = A | B \n def f<T>(x: T): T := x \n def main(): Int := 0");
}

// ── generic function calls: instantiation by unifying arguments
// ───────────────

#[test]
fn call_generic_identity_with_int() {
    // `id(5)` instantiates `T = Int`, so the call's type is `Int`.
    typecheck("def id<T>(x: T): T := x \n def main(): Int := id(5)");
}

#[test]
fn call_generic_identity_with_bool() {
    typecheck("def id<T>(x: T): T := x \n def main(): Bool := id(true)");
}

#[test]
fn call_generic_result_type_must_match_context() {
    // `id(5) : Int` cannot be used where `Bool` is expected.
    typecheck_fails("def id<T>(x: T): T := x \n def main(): Bool := id(5)");
}

#[test]
fn call_generic_first_of_two() {
    typecheck("def first<A, B>(a: A, b: B): A := a \n def main(): Int := first(7, true)");
}

#[test]
fn call_generic_second_of_two() {
    typecheck("def snd<A, B>(a: A, b: B): B := b \n def main(): Bool := snd(7, true)");
}

#[test]
fn call_generic_shared_param_consistent_ok() {
    typecheck("def same<T>(a: T, b: T): T := a \n def main(): Int := same(1, 2)");
}

#[test]
fn call_generic_shared_param_conflict_fails() {
    // `T` is forced to `Int` then `Bool` — unsolvable.
    typecheck_fails("def same<T>(a: T, b: T): T := a \n def main(): Int := same(1, true)");
}

#[test]
fn call_generic_nested_instantiation() {
    // a generic function calling another, with the parameter flowing through.
    typecheck(
        "def id<T>(x: T): T := x \n \
         def twice<U>(y: U): U := id(id(y)) \n \
         def main(): Int := twice(3)",
    );
}

#[test]
fn call_generic_with_tuple_argument() {
    // `T` is solved by unifying `(T, Int)` against the argument `(Bool, Int)`.
    typecheck("def f<T>(p: (T, Int)): Int := 0 \n def main(): Int := f((true, 5))");
}

#[test]
fn call_generic_tuple_argument_inner_mismatch_fails() {
    // the concrete `Int` slot of `(T, Int)` does not match a `Bool`.
    typecheck_fails("def f<T>(p: (T, Int)): Int := 0 \n def main(): Int := f((true, false))");
}

// ── generic enum declarations
// ─────────────────────────────────────────────────

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

// ── generic enum uses: instantiation via annotations and constructors
// ─────────

const OPTION: &str = "type Option<T> = None | Some(T) \n";

#[test]
fn construct_generic_enum_payload_infers_args() {
    // `Option#Some(5)` infers `T = Int` from the payload.
    typecheck(&format!(
        "{OPTION} def main(): Int := {{ let x: Option<Int> = Option#Some(5); 0 }}"
    ));
}

#[test]
fn construct_generic_enum_nullary_with_annotation() {
    // `Option#None` is ambiguous alone, but the annotation solves `T = Int`.
    typecheck(&format!(
        "{OPTION} def main(): Int := {{ let x: Option<Int> = Option#None; 0 }}"
    ));
}

#[test]
fn construct_generic_enum_nullary_without_annotation_fails() {
    typecheck_fails(&format!(
        "{OPTION} def main(): Int := {{ let x = Option#None; 0 }}"
    ));
}

#[test]
fn construct_generic_enum_payload_against_wrong_instantiation_fails() {
    // annotation forces `T = Bool`, but the payload is an `Int`.
    typecheck_fails(&format!(
        "{OPTION} def main(): Int := {{ let x: Option<Bool> = Option#Some(5); 0 }}"
    ));
}

#[test]
fn generic_enum_instantiations_are_distinct_types() {
    // an `Option<Int>` value cannot initialise an `Option<Bool>` binding.
    typecheck_fails(&format!(
        "{OPTION} def main(): Int := {{ \
           let a: Option<Int> = Option#Some(5); \
           let b: Option<Bool> = a; \
           0 }}"
    ));
}

#[test]
fn function_returning_generic_enum_type_checks() {
    typecheck(&format!(
        "{OPTION} def wrap<T>(x: T): Option<T> := Option#Some(x) \n def main(): Int := 0"
    ));
}

#[test]
fn calling_function_returning_generic_enum_instantiates_result() {
    // `wrap(5)` substitutes `T = Int` into the return type, yielding
    // `Option<Int>`, which then initialises an `Option<Int>` binding.
    typecheck(&format!(
        "{OPTION} \
         def wrap<T>(x: T): Option<T> := Option#Some(x) \n \
         def main(): Int := {{ let o: Option<Int> = wrap(5); 0 }}"
    ));
}

#[test]
fn calling_function_returning_generic_enum_wrong_arg_fails() {
    // `wrap(true) : Option<Bool>` cannot initialise an `Option<Int>` binding.
    typecheck_fails(&format!(
        "{OPTION} \
         def wrap<T>(x: T): Option<T> := Option#Some(x) \n \
         def main(): Int := {{ let o: Option<Int> = wrap(true); 0 }}"
    ));
}

#[test]
fn type_arg_arity_mismatch_fails() {
    // `Option` takes one type argument, not two.
    typecheck_fails(&format!(
        "{OPTION} def f(x: Option<Int, Bool>): Int := 0 \n def main(): Int := 0"
    ));
}

#[test]
fn instantiating_non_generic_enum_fails() {
    typecheck_fails(
        "type Color = Red | Green \n def f(x: Color<Int>): Int := 0 \n def main(): Int := 0",
    );
}

// ── matching on generic enums substitutes the binding types
// ───────────────────

#[test]
fn match_generic_enum_binds_concrete_payload() {
    // `Option#Some(x)` against an `Option<Int>` scrutinee binds `x : Int`, so
    // returning `x` where `Int` is expected type-checks.
    typecheck(&format!(
        "{OPTION} \
         def unwrap(o: Option<Int>): Int := match o {{ Option#Some(x) => x, Option#None => 0 }} \n \
         def main(): Int := unwrap(Option#Some(5))"
    ));
}

#[test]
fn match_generic_enum_uses_the_right_argument() {
    // the same enum at `Bool` binds `x : Bool`.
    typecheck(&format!(
        "{OPTION} \
         def f(o: Option<Bool>): Bool := match o {{ Option#Some(x) => x, Option#None => false }} \n \
         def main(): Int := 0"
    ));
}

#[test]
fn match_generic_enum_binding_wrong_type_fails() {
    // `x : Int` cannot be returned where `Bool` is expected.
    typecheck_fails(&format!(
        "{OPTION} \
         def f(o: Option<Int>): Bool := match o {{ Option#Some(x) => x, Option#None => false }} \n \
         def main(): Int := 0"
    ));
}

#[test]
fn let_pattern_on_generic_enum_binds_concrete_payload() {
    // the `else` fallback is a value of the same instantiation; `x : Int`.
    typecheck(&format!(
        "{OPTION} \
         def f(o: Option<Int>): Int := {{ let Option#Some(x) = o else Option#Some(0); x }} \n \
         def main(): Int := 0"
    ));
}

// ── end-to-end execution: generics monomorphise, lower to MIR, and run
// ────────

#[test]
fn run_generic_identity() {
    assert_eq!(
        run_both("def id<T>(x: T): T := x \n def main(): Int := id(5)"),
        Expression::Int(5)
    );
}

#[test]
fn run_generic_identity_bool() {
    assert_eq!(
        run_both("def id<T>(x: T): T := x \n def main(): Bool := id(true)"),
        Expression::Bool(true)
    );
}

#[test]
fn run_generic_first_of_two() {
    assert_eq!(
        run_both("def first<A, B>(a: A, b: B): A := a \n def main(): Int := first(7, true)"),
        Expression::Int(7)
    );
}

#[test]
fn run_generic_used_at_two_types() {
    // `id` is instantiated at both `Int` and `Bool`; two specialisations.
    assert_eq!(
        run_both(
            "def id<T>(x: T): T := x \n \
             def main(): Int := { let a: Int = id(5); let b: Bool = id(true); a }"
        ),
        Expression::Int(5)
    );
}

#[test]
fn run_generic_nested_calls() {
    assert_eq!(
        run_both(
            "def id<T>(x: T): T := x \n \
             def twice<U>(y: U): U := id(id(y)) \n \
             def main(): Int := twice(42)"
        ),
        Expression::Int(42)
    );
}

#[test]
fn run_generic_enum_construct_and_match() {
    // build an `Option<Int>` and consume it via `match` — exercises specialised
    // enum construction and pattern matching through MIR.
    let src = "type Option<T> = None | Some(T) \n \
        def unwrap(o: Option<Int>): Int := match o { Option#Some(x) => x, Option#None => 0 } \n \
        def main(): Int := unwrap(Option#Some(7))";
    assert_eq!(run_both(src), Expression::Int(7));
}

#[test]
fn run_generic_enum_none_branch() {
    let src = "type Option<T> = None | Some(T) \n \
        def unwrap(o: Option<Int>): Int := match o { Option#Some(x) => x, Option#None => 99 } \n \
        def main(): Int := unwrap(Option#None)";
    assert_eq!(run_both(src), Expression::Int(99));
}

#[test]
fn run_generic_function_returning_generic_enum() {
    let src = "type Option<T> = None | Some(T) \n \
        def wrap<T>(x: T): Option<T> := Option#Some(x) \n \
        def unwrap(o: Option<Int>): Int := match o { Option#Some(x) => x, Option#None => 0 } \n \
        def main(): Int := unwrap(wrap(13))";
    assert_eq!(run_both(src), Expression::Int(13));
}
