//! tests for examples in examples/

mod common;
use sand::ir_types::hhir::Expression;

#[test]
fn fact() -> anyhow::Result<()> {
    // run the code, examples must always work
    let out = common::interpret_example("fact")?;

    assert_eq!(out, Expression::Int(362880));

    Ok(())
}
