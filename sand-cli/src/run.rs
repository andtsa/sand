//! run the input files with the interpreter
use std::path::PathBuf;

use clap::Args;
use clap::clap_derive::ValueEnum;
use lang::castles::project::CheckResult;
use lang::castles::project::Project;
use lang::compiler::diagnostics::SandDiagnostic;
use lang::ir_types::mir::MirProgram;

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

pub fn run(args: RunArgs, dry_run: bool) -> Result<(), CliError> {
    let span = tracing::info_span!("run subcommand");
    let _g = span.enter();

    let project_result = if let Some(config) = &args.config {
        // Load project from config file
        let span = tracing::debug_span!("loading project from config");
        let _g1 = span.enter();
        Project::from_config(config)
    } else {
        // Load input files using [`Project::from_paths`]
        let span = tracing::debug_span!("loading project from paths");
        let _g1 = span.enter();
        Project::from_paths(&args.input)
    }?;
    let project = project_result.project;

    for warning in project_result.warnings {
        eprintln!("{}", warning.to_diagnostic().render(&project));
    }

    tracing::debug!("loaded {} files", project.file_count());

    let span = tracing::debug_span!("compiling modules");
    let _g2 = span.enter();

    let result = project.check();
    let (ctx, ast) = match result {
        CheckResult::Success { ctx, ast } => {
            tracing::debug!(
                "compilation successful with {} functions",
                ast.functions.len()
            );
            ast.functions
                .values()
                .for_each(|f| tracing::trace!(name = ctx.original_fun_name(f.name)));
            for diag in &ctx.diagnostics {
                // Skip diagnostics from synthetic files (e.g. the core library).
                if diag.file.is_some_and(|fr| project.is_synthetic_file(fr)) {
                    continue;
                }
                eprintln!("{}", diag.render(&project));
            }
            (ctx, ast)
        }
        CheckResult::Failure { ctx, error } => {
            let diags = SandDiagnostic::from_compiler_error(&ctx, &error);
            for (_file_ref, file_diags) in diags.map {
                for diag in file_diags {
                    eprintln!("{}", diag.render(&project));
                }
            }
            return Err(CliError::CompilerError {
                diagnostic: error.to_string(),
            });
        }
    };
    drop(_g2);

    if args.print_ast {
        println!("{}", ast.dump(&ctx));
        return Ok(());
    }
    if dry_run {
        return Ok(());
    }

    // run code
    if args.mode == InterpMode::Hir {
        ast.interpret(&ctx)?;
        return Ok(());
    }

    let mir = MirProgram::from_typed_program(&ast, &ctx);

    mir.interpret(&ctx)?;
    
    Ok(())
}
