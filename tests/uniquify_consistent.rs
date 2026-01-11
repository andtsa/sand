use untitled::lang::Program;

mod common;

#[test]
fn uniquify_consistent() -> anyhow::Result<()> {
    let examples = vec!["fact", "fib", "test"];

    for example in examples {
        let code = common::open_example_from_file(example);

        let program = Program::parse(&code)?;

        let result_a = program.interpret()?;

        let result_b = program.uniquify()?.interpret()?;

        assert_eq!(result_a, result_b);
    }

    Ok(())
}
