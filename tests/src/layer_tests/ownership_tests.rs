//! tests for the affine-type ownership checker

use crate::common::*;

const CONSUME_FN: &str = "
type Light = Red | Yellow | Green
def consume(l: Light): Unit := { }
";

fn with_consume(body: &str) -> String {
    format!("{CONSUME_FN}\ndef main(): Unit := {{\n{body}\n}}")
}

#[test]
fn use_after_move_via_call() {
    ownership_fails(&with_consume(
        "let light: Light = Light#Red;
         consume(light);
         consume(light);",
    ));
}

#[test]
fn use_after_move_via_binding() {
    ownership_fails(&with_consume(
        "let a: Light = Light#Red;
         let b: Light = a;
         consume(a);",
    ));
}

#[test]
fn use_after_call_then_bind() {
    ownership_fails(&with_consume(
        "let d: Light = Light#Yellow;
         consume(d);
         let e: Light = d;",
    ));
}

#[test]
fn rebind_after_move_is_ok() {
    ownership_ok(&with_consume(
        "let light: Light = Light#Red;
         consume(light);
         let light: Light = Light#Yellow;
         consume(light);",
    ));
}

#[test]
fn mutable_reassignment_restores_ownership() {
    ownership_ok(&with_consume(
        "let mut a: Light = Light#Red;
         let mut b: Light = Light#Yellow;
         b = a;
         a = Light#Green;
         consume(a);
         consume(b);",
    ));
}

#[test]
fn ownership_transfers_across_mutability() {
    ownership_ok(&with_consume(
        "let a: Light = Light#Red;
         let mut b: Light = a;
         let c: Light = b;
         consume(c);",
    ));
}

#[test]
fn int_is_copy() {
    ownership_ok(
        "def double(x: Int): Int := x + x
         def main(): Int := {
             let x: Int = 5;
             let a: Int = x;
             let b: Int = x;
             a + b
         }",
    );
}

#[test]
fn bool_is_copy() {
    ownership_ok(
        "def main(): Bool := {
             let flag: Bool = true;
             let _a: Bool = flag;
             flag
         }",
    );
}

#[test]
fn moved_in_both_branches_unavailable_after() {
    ownership_fails(&with_consume(
        "let x: Light = Light#Red;
         if true then { consume(x) } else { consume(x) };
         consume(x);",
    ));
}

#[test]
fn moved_in_one_branch_unavailable_after() {
    ownership_fails(&with_consume(
        "let x: Light = Light#Red;
         if true then { consume(x) } else { };
         consume(x);",
    ));
}

#[test]
fn not_moved_in_either_branch_available_after() {
    ownership_ok(&with_consume(
        "let x: Light = Light#Red;
         if true then { } else { };
         consume(x);",
    ));
}

#[test]
fn move_inside_loop_is_error() {
    ownership_fails(&with_consume(
        "let light: Light = Light#Red;
         while true do { consume(light); };",
    ));
}

#[test]
fn already_moved_before_loop_is_error_inside() {
    ownership_fails(&with_consume(
        "let light: Light = Light#Red;
         consume(light);
         while true do { consume(light); };",
    ));
}

#[test]
fn move_and_reinit_inside_loop_is_ok() {
    ownership_ok(
        "type Light = Red | Yellow | Green
         def consume(l: Light): Light := Light#Green
         def main(): Unit := {
             let mut light: Light = Light#Red;
             while true do {
                 light = consume(light);
             };
         }",
    );
}

#[test]
fn match_scrutinee_consumed() {
    ownership_fails(
        "type Light = Red | Yellow | Green
         def consume(l: Light): Unit := { }
         def main(): Unit := {
             let x: Light = Light#Red;
             let _n: Int = match x {
                 Light#Red => 0,
                 Light#Yellow => 1,
                 Light#Green => 2,
             };
             consume(x);
         }",
    );
}

#[test]
fn match_scrutinee_valid_use() {
    ownership_ok(
        "type Light = Red | Yellow | Green
         def main(): Int := {
             let x: Light = Light#Green;
             match x {
                 Light#Red => 0,
                 Light#Yellow => 1,
                 Light#Green => 2,
             }
         }",
    );
}

#[test]
fn move_inside_block_propagates_out() {
    ownership_fails(&with_consume(
        "let x: Light = Light#Red;
         { consume(x); };
         consume(x);",
    ));
}

#[test]
fn block_local_var_not_visible_outside() {
    ownership_ok(&with_consume(
        "let x: Light = Light#Red;
         { let y: Light = Light#Green; consume(y); };
         consume(x);",
    ));
}

#[test]
fn parameter_can_be_used_once() {
    ownership_ok(
        "type Light = Red | Yellow | Green
         def consume(l: Light): Unit := { }
         def relay(l: Light): Unit := consume(l)
         def main(): Unit := relay(Light#Red)",
    );
}

#[test]
fn parameter_used_twice_is_error() {
    ownership_fails(
        "type Light = Red | Yellow | Green
         def consume(l: Light): Unit := { }
         def double_consume(l: Light): Unit := {
             consume(l);
             consume(l);
         }
         def main(): Unit := double_consume(Light#Red)",
    );
}
