//! tests for examples in examples/

mod setup;
use untitled::lang::Expression;

#[test]
fn fib() -> anyhow::Result<()> {
    // run the code, examples must always work
    let out = setup::interpret_example("fib")?;

    assert_eq!(out, Expression::Int(55));

    Ok(())
}
