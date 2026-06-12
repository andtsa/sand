//! run examples through the whole compiler

use crate::common::run_compiled;

#[cfg(test)]
fn assert_return(input: &[&str], expected: i32) -> anyhow::Result<()> {
    let (result, _) = run_compiled(input)?;
    assert_eq!(result, expected);
    Ok(())
}

#[cfg(test)]
fn assert_output(input: &[&str], expected: &[&str]) -> anyhow::Result<()> {
    let (_, output_lines) = run_compiled(input)?;
    assert_eq!(output_lines, expected);
    Ok(())
}

#[test]
fn compiled_factorial() {
    assert_output(&["fact"], &["362880"]).unwrap();
}

#[test]
fn compiled_prime() {
    assert_return(&["prime"], 97).unwrap();
}

#[test]
fn compiled_gcd() {
    assert_return(&["gcd"], 1).unwrap();
}

include!(concat!(env!("OUT_DIR"), "/compiled_examples_tests.rs"));
