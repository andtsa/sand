use sand::compiler::context::CompileCtx;
use sand::ir_types::hhir::ProgramModule;

#[test]
fn gibberish() {
    let mut ctx = CompileCtx::initial();
    let program = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert!(ProgramModule::parse_stub(&mut ctx, program).is_err());
}
