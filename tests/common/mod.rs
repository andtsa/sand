//! helper methods for integration tests
#![allow(dead_code)]

use untitled::lang::Expression;

pub fn open_example_from_file(name: &str) -> String {
    let path = format!("examples/{}.u", name);
    std::fs::read_to_string(path).expect("failed to read example file")
}

pub fn interpret_example(name: &str) -> anyhow::Result<Expression> {
    let program = untitled::lang::Program::parse(&open_example_from_file(name))?;
    let result = program.interpret()?;
    Ok(result)
}
