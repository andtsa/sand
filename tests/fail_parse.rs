use sand::ir_types::hhir::ProgramModule;

#[test]
fn gibberish() {
    let program = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert!(ProgramModule::parse(program).is_err());
}
