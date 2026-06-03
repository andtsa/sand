use std::path::PathBuf;

use clap::Args;
use lang::castles::project::CheckResult;
use lang::castles::project::Project;
use lang::castles::project::init::FatalProjectCreationError;
use lang::compiler::diagnostics::SandDiagnostic;

#[derive(Debug, Args)]
pub struct FmtArgs {
    /// Input file to format
    #[arg(required = true)]
    input: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum FmtCliError {
    #[error("project initialization error: {0}")]
    ProjectInit(Box<FatalProjectCreationError>),
    #[error("compiler error")]
    CompilerError,
}

impl From<FatalProjectCreationError> for FmtCliError {
    fn from(value: FatalProjectCreationError) -> Self {
        FmtCliError::ProjectInit(Box::new(value))
    }
}

pub fn fmt(args: FmtArgs) -> Result<(), FmtCliError> {
    let project_result = Project::from_paths(&[args.input])?;
    let project = project_result.project;

    for w in project_result.warnings {
        eprintln!("{}", w.to_diagnostic().render(&project));
    }

    match project.check() {
        CheckResult::Success { ctx, ast } => {
            let formatted = ast.format(&ctx);
            print!("{}", formatted.values().next().unwrap_or(&String::new()));
            Ok(())
        }
        CheckResult::Failure { ctx, error } => {
            let diags = SandDiagnostic::from_compiler_error(&ctx, &error);
            for (_file_ref, file_diags) in diags.map {
                for diag in file_diags {
                    eprintln!("{}", diag.render(&project));
                }
            }
            Err(FmtCliError::CompilerError)
        }
    }
}
