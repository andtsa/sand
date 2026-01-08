//! tests for examples in examples/

mod setup;
use untitled::lang::Expression;

#[test]
fn fact() -> anyhow::Result<()> {
    // run the code, examples must always work
    let out = setup::interpret_example("fact")?;

    assert_eq!(out, Expression::Int(362880));

    Ok(())
}
