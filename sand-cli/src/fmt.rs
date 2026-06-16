use std::path::PathBuf;

use clap::Args;
use lang::Stage;
use lang::castles::project::Project;
use lang::castles::project::init::FatalProjectCreationError;

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

    // Formatting only needs the type-checked program *before* heap-lowering /
    // monomorphisation, so the output stays source-faithful (heaped types aren't
    // rewritten to `Unique<…>`) and we skip ownership + mono entirely.
    let c = project.check_to(Stage::Typed);
    // Format only a *complete*, well-typed program. With function-granular
    // recovery, `typed` is `Some` even on type errors (carrying just the
    // functions that checked), formatting that would silently drop the broken
    // ones, so gate on the absence of any fatal error and render every
    // accumulated diagnostic otherwise.
    match c.typed {
        Some(ast) if c.first_error.is_none() => {
            let formatted = ast.format(&c.ctx);
            print!("{}", formatted.values().next().unwrap_or(&String::new()));
            Ok(())
        }
        _ => {
            for file_diags in c.diagnostics.map.values() {
                for diag in file_diags {
                    eprintln!("{}", diag.render(&project));
                }
            }
            Err(FmtCliError::CompilerError)
        }
    }
}
