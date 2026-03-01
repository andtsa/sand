use sand::ir_types::hhir::Program;

#[test]
fn gibberish() {
    let program = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert!(Program::parse(program).is_err());
}
