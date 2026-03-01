//! helper methods for integration tests
#![allow(dead_code)]

use sand::ir_types::ast::Expression;
use sand::ir_types::ast::Program;

pub fn open_example_from_file(name: &str) -> String {
    let path = format!("examples/{}.kap", name);
    std::fs::read_to_string(path).expect("failed to read example file")
}

pub fn interpret_example(name: &str) -> anyhow::Result<Expression> {
    let program = Program::parse(&open_example_from_file(name))?;
    let result = program.interpret()?;
    Ok(result)
}
