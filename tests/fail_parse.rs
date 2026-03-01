use sand::ir_types::ast::Program;

#[test]
fn gibberish() {
    let program = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert!(Program::parse(program).is_err());
}
