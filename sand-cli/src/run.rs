//! run the input files with the interpreter
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use clap::clap_derive::ValueEnum;
use lang::interpreter::mir_exit_code;
use lang::interpreter::thir_exit_code;
use lang::ir_types::mir::MirProgram;

use crate::compile::load_and_check;
use crate::error::CliError;

#[derive(Default, ValueEnum, Debug, Clone, PartialEq, Eq)]
enum InterpMode {
    Hir,
    #[default]
    Mir,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Input file(s) to run
    #[arg(required = true, conflicts_with = "config")]
    input: Vec<PathBuf>,
    /// Run a project from a config file
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Print the AST instead of running
    #[arg(short, long)]
    print_ast: bool,
    /// Whether to use the HIR or MIR interpreter for running
    #[arg(short, long, value_enum, default_value = "mir")]
    mode: InterpMode,
}

pub fn run(args: RunArgs, dry_run: bool) -> Result<ExitCode, CliError> {
    let span = tracing::info_span!("run subcommand");
    let _g = span.enter();

    let (ctx, ast) = load_and_check(args.config.as_ref(), &args.input)?;

    if args.print_ast {
        println!("{}", ast.dump(&ctx));
        return Ok(ExitCode::SUCCESS);
    }
    if dry_run {
        return Ok(ExitCode::SUCCESS);
    }

    // run code
    if args.mode == InterpMode::Hir {
        let expr = ast.interpret(&ctx)?;
        return Ok(thir_exit_code(&expr));
    }

    let mir = MirProgram::from_typed_program(&ast, &ctx);

    let val = mir.interpret(&ctx)?;

    Ok(mir_exit_code(&val))
}
