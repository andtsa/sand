//! TypedHIR type-checking tests for the Sand compiler
//!
//! Tests cover the type-checking phase, including:
//! - Literal type checking (int, bool, unit)
//! - Arithmetic and comparison operators
//! - Boolean operators
//! - Control flow (if/while)
//! - Functions and recursion
//! - Variable binding and assignment
//! - Error cases for type mismatches

// ── happy-path type checking ──────────────────────────────────────────

use lang::interpreter::mir::MirValue;
use lang::ir_types::typed_hir::Expression;

use crate::common::run_hir;
use crate::common::run_mir;
use crate::common::typecheck;
use crate::common::typecheck_fails;

#[test]
fn typecheck_int_literal() {
    typecheck("def main(): Int := 0");
}

#[test]
fn typecheck_bool_literal() {
    typecheck("def main(): Bool := true");
}

#[test]
fn typecheck_unit_literal() {
    typecheck("def main(): Unit := { }");
}

#[test]
fn typecheck_arithmetic() {
    typecheck("def main(): Int := 1 + 2 * 3 - 4 / 2");
}

#[test]
fn typecheck_comparison_returns_bool() {
    typecheck("def main(): Bool := 1 < 2");
}

#[test]
fn typecheck_equality_returns_bool() {
    typecheck("def main(): Bool := 1 == 1");
}

#[test]
fn typecheck_boolean_and() {
    typecheck("def main(): Bool := true & false");
}

#[test]
fn typecheck_boolean_or() {
    typecheck("def main(): Bool := true | false");
}

#[test]
fn typecheck_boolean_not() {
    typecheck("def main(): Bool := !false");
}

#[test]
fn typecheck_unary_negation() {
    typecheck("def main(): Int := -(3)");
}

#[test]
fn typecheck_if_branches_same_type() {
    typecheck("def main(): Int := if true then 1 else 2");
}

#[test]
fn typecheck_while_loop() {
    typecheck(
        "def main(): Unit := {
            let mut i: Int = 0;
            while i < 3 do {
                i = i + 1;
            };
        }",
    );
}

#[test]
fn typecheck_let_binding_and_return() {
    typecheck(
        "def main(): Int := {
            let x: Int = 10;
            x
        }",
    );
}

#[test]
fn typecheck_function_call_correct_arg_types() {
    typecheck(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(1, 2)",
    );
}

#[test]
fn typecheck_recursive_function() {
    typecheck(
        "def fact(n: Int): Int :=
            if n == 0 then 1 else n * fact(n - 1)
         def main(): Int := fact(5)",
    );
}

#[test]
fn typecheck_block_with_assignment() {
    typecheck(
        "def main(): Int := {
            let mut x: Int = 5;
            x = x + 1;
            x
        }",
    );
}

#[test]
fn typecheck_nested_blocks() {
    typecheck(
        "def main(): Int := {
            let a: Int = {
                let b: Int = 3;
                b * 2
            };
            a + 1
        }",
    );
}

#[test]
fn typecheck_intrinsic_println() {
    typecheck(
        "def main(): Unit := {
            println(42);
        }",
    );
}

#[test]
fn typecheck_power_operator() {
    typecheck("def main(): Int := 2 ^ 10");
}

// ── type-error cases ──────────────────────────────────────────────────

#[test]
fn typecheck_fails_wrong_return_type() {
    // Function declares Int return, body produces Bool.
    typecheck_fails("def main(): Int := true");
}

#[test]
fn typecheck_fails_wrong_return_type_bool_for_int() {
    typecheck_fails("def main(): Bool := 42");
}

#[test]
fn typecheck_fails_add_bool_and_int() {
    typecheck_fails("def main(): Int := true + 1");
}

#[test]
fn typecheck_fails_negate_int() {
    // `!` is boolean NOT; applying it to Int should fail.
    typecheck_fails("def main(): Bool := !1");
}

#[test]
fn typecheck_fails_arithmetic_negate_bool() {
    // Unary minus on Bool.
    typecheck_fails("def main(): Int := -(true)");
}

#[test]
fn typecheck_fails_if_condition_not_bool() {
    typecheck_fails("def main(): Int := if 1 then 2 else 3");
}

#[test]
fn typecheck_fails_if_branches_different_types() {
    typecheck_fails("def main(): Int := if true then 1 else false");
}

#[test]
fn typecheck_fails_while_condition_not_bool() {
    typecheck_fails(
        "def main(): Unit := {
            while 1 do { };
        }",
    );
}

#[test]
fn typecheck_fails_declare_wrong_type() {
    typecheck_fails(
        "def main(): Int := {
            let x: Bool = 42;
            0
        }",
    );
}

#[test]
fn typecheck_fails_assign_wrong_type() {
    typecheck_fails(
        "def main(): Int := {
            let x: Int = 0;
            x = true;
            x
        }",
    );
}

#[test]
fn typecheck_fails_wrong_argument_type() {
    typecheck_fails(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(true, 2)",
    );
}

#[test]
fn typecheck_fails_wrong_argument_count_too_few() {
    typecheck_fails(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(1)",
    );
}

#[test]
fn typecheck_fails_wrong_argument_count_too_many() {
    typecheck_fails(
        "def add(a: Int, b: Int): Int := a + b
         def main(): Int := add(1, 2, 3)",
    );
}

#[test]
fn typecheck_fails_comparison_mixed_types() {
    // `<` only accepts Int operands.
    typecheck_fails("def main(): Bool := true < false");
}

// ── let-tuple binding ────────────────────────────────────────────────────

/// `let (a, b) = (expr, expr)` typechecks when the RHS is a tuple of the
/// matching arity.
#[test]
fn let_tuple_basic_typechecks() {
    typecheck(
        "def main(): Int := {
            let (a, b) = (1, 2);
            a + b
        }",
    );
}

/// The bound variables carry the correct types from the tuple elements.
#[test]
fn let_tuple_binds_correct_types() {
    let result = run_hir(
        "def main(): Int := {
            let (a, b) = (3, 7);
            a + b
        }",
    );
    assert_eq!(result, Expression::Int(10));
}

/// Let-tuple binding works with a function returning a tuple.
#[test]
fn let_tuple_from_function_return() {
    let result = run_mir(
        "def pair(): (Int, Int) := (10, 20)
         def main(): Int := {
             let (x, y) = pair();
             x + y
         }",
    );
    assert_eq!(result, MirValue::Int(30));
}

/// Let-tuple with a `mut` element allows reassignment of that element.
#[test]
fn let_tuple_mut_elem_can_be_reassigned() {
    let result = run_hir(
        "def main(): Int := {
            let (a, mut b) = (1, 2);
            b = 10;
            a + b
        }",
    );
    assert_eq!(result, Expression::Int(11));
}

/// A three-element let-tuple works.
#[test]
fn let_tuple_three_elements() {
    let result = run_hir(
        "def main(): Int := {
            let (a, b, c) = (1, 2, 3);
            a + b + c
        }",
    );
    assert_eq!(result, Expression::Int(6));
}

/// Let-tuple with a type annotation on the RHS typechecks.
#[test]
fn let_tuple_with_type_annotation() {
    typecheck(
        "def main(): Int := {
            let (a, b): (Int, Bool) = (42, true);
            a
        }",
    );
}

/// Let-tuple whose RHS is not a tuple is a type error.
#[test]
fn let_tuple_non_tuple_rhs_is_error() {
    typecheck_fails(
        "def main(): Int := {
            let (a, b) = 5;
            a
        }",
    );
}

/// Let-tuple arity mismatch (too few LHS elements) is a type error.
#[test]
fn let_tuple_arity_mismatch_too_few_is_error() {
    typecheck_fails(
        "def main(): Int := {
            let (a, b) = (1, 2, 3);
            a
        }",
    );
}

/// Let-tuple arity mismatch (too many LHS elements) is a type error.
#[test]
fn let_tuple_arity_mismatch_too_many_is_error() {
    typecheck_fails(
        "def main(): Int := {
            let (a, b, c) = (1, 2);
            a
        }",
    );
}

// ── let-pattern (constructor) binding ────────────────────────────────────────

/// Basic let-pattern binding typechecks when the else branch produces the
/// same variant as the pattern.
#[test]
fn let_pattern_basic_typechecks() {
    typecheck(
        "type Opt = None | Some(Int)
         def main(): Int := {
             let Opt#Some(x) = Opt#Some(42) else Opt#Some(0);
             x
         }",
    );
}

/// The extracted binding carries the correct type and value.
#[test]
fn let_pattern_extracts_value() {
    let result = run_hir(
        "type Opt = None | Some(Int)
         def main(): Int := {
             let Opt#Some(x) = Opt#Some(7) else Opt#Some(0);
             x
         }",
    );
    assert_eq!(result, Expression::Int(7));
}

/// The else fallback is used when the scrutinee does not match.
#[test]
fn let_pattern_fallback_on_mismatch() {
    let result = run_hir(
        "type Opt = None | Some(Int)
         def main(): Int := {
             let Opt#Some(x) = Opt#None else Opt#Some(-1);
             x
         }",
    );
    assert_eq!(result, Expression::Int(-1));
}

/// A wildcard sub-pattern (`_`) inside a tuple payload is accepted.
#[test]
fn let_pattern_wildcard_in_tuple_payload() {
    let result = run_hir(
        "type Pair = P((Int, Int))
         def main(): Int := {
             let Pair#P((a, _)) = Pair#P((10, 99)) else Pair#P((0, 0));
             a
         }",
    );
    assert_eq!(result, Expression::Int(10));
}

/// Works via MIR/compiled path too.
#[test]
fn let_pattern_compiled() {
    let result = run_mir(
        "type List = Empty | Cons((Int, List))
         def head_or(list: List, default: Int): Int := {
             let List#Cons((x, _)) = list else List#Cons((default, List#Empty));
             x
         }
         def main(): Int := {
             head_or(List#Cons((5, List#Empty)), 0)
         }",
    );
    assert_eq!(result, MirValue::Int(5));
}

/// A missing `else` branch is a parse/type error.
#[test]
fn let_pattern_missing_else_is_error() {
    typecheck_fails(
        "type Opt = None | Some(Int)
         def main(): Int := {
             let Opt#Some(x) = Opt#Some(42);
             x
         }",
    );
}

/// The else expression must have the same variant as the pattern; a different
/// variant (even from the same enum) is rejected.
#[test]
fn let_pattern_else_wrong_variant_is_error() {
    typecheck_fails(
        "type AB = A(Int) | B(Int)
         def main(): Int := {
             let AB#A(x) = AB#A(1) else AB#B(0);
             x
         }",
    );
}

/// A nested refutable sub-pattern (variant inside variant) is rejected;
/// users should use `match` for that.
#[test]
fn let_pattern_nested_variant_sub_pattern_is_error() {
    typecheck_fails(
        "type Inner = X(Int)
         type Outer = Wrap(Inner)
         def main(): Int := {
             let Outer#Wrap(Inner#X(n)) = Outer#Wrap(Inner#X(1)) else Outer#Wrap(Inner#X(0));
             n
         }",
    );
}

// ── bare-tag constructor inference (todo 6)
// ───────────────────────────────────

/// A bare #Tag with payload typechecks when the expected type is known.
#[test]
fn tag_inference_with_payload_in_let_annotation() {
    typecheck(
        "type Opt = None | Some(Int)
         def main(): Int := {
             let x: Opt = #Some(42);
             0
         }",
    );
}

/// Bare #Tag (nullary) in a `let` with annotation typechecks.
#[test]
fn tag_inference_nullary_in_let_annotation() {
    typecheck(
        "type Opt = None | Some(Int)
         def main(): Int := {
             let x: Opt = #None;
             0
         }",
    );
}

/// Bare #Tag with payload in a function return position is resolved.
#[test]
fn tag_inference_in_return_position() {
    let result = run_hir(
        "type Opt = None | Some(Int)
         def wrap(x: Int): Opt := #Some(x)
         def main(): Int := {
             match wrap(7) {
                 Opt#Some(v) => v,
                 Opt#None => 0,
             }
         }",
    );
    assert_eq!(result, Expression::Int(7));
}

/// Bare #Tag in a function argument position is resolved against the parameter
/// type.
#[test]
fn tag_inference_in_call_argument() {
    let result = run_hir(
        "type Opt = None | Some(Int)
         def unwrap_or(o: Opt, default: Int): Int :=
             match o {
                 Opt#Some(v) => v,
                 Opt#None => default,
             }
         def main(): Int :=
             unwrap_or(#Some(5), 0)",
    );
    assert_eq!(result, Expression::Int(5));
}

/// Bare #Tag in both branches of an if-else is resolved.
#[test]
fn tag_inference_in_if_else_branches() {
    let result = run_hir(
        "type Opt = None | Some(Int)
         def main(): Int := {
             let x: Opt = if true then #Some(3) else #None;
             match x {
                 Opt#Some(v) => v,
                 Opt#None => -1,
             }
         }",
    );
    assert_eq!(result, Expression::Int(3));
}

/// Bare #Tag works in a match arm body.
#[test]
fn tag_inference_in_match_arm_body() {
    let result = run_mir(
        "type List = Empty | Cons((Int, List))
         def sum(l: List): Int :=
             match l {
                 #Cons((x, rest)) => x + sum(rest),
                 #Empty           => 0,
             }
         def main(): Int := sum(List#Cons((1, List#Cons((2, List#Empty)))))",
    );
    assert_eq!(result, MirValue::Int(3));
}

/// Providing a payload to a nullary variant via bare tag is a type error.
#[test]
fn tag_inference_payload_on_nullary_is_error() {
    typecheck_fails(
        "type Opt = None | Some(Int)
         def main(): Int := {
             let x: Opt = #None(42);
             0
         }",
    );
}

/// Omitting the payload for a non-nullary variant via bare tag is a type error.
#[test]
fn tag_inference_missing_payload_is_error() {
    typecheck_fails(
        "type Opt = None | Some(Int)
         def main(): Int := {
             let x: Opt = #Some;
             0
         }",
    );
}
