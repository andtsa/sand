//! parse to typed ast and print the formatted code

use std::path::PathBuf;

use lang::castles::project::CheckResult;
use lang::castles::project::Project;
use lang::castles::project::init::ProjectCreationResult;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input-file>", args[0]);
        std::process::exit(1);
    }

    // let src = std::fs::read_to_string(&args[1])
    //     .map_err(|e| anyhow::anyhow!("failed to read {}: {}", args[1], e))?;

    let ProjectCreationResult { project, warnings } =
        Project::from_paths(&[PathBuf::from(&args[1])])?;

    for w in warnings {
        eprintln!("{}", w.to_diagnostic().render(&project));
    }

    let cr = project.check();
    match cr {
        CheckResult::Success { ctx, ast } => {
            let formatted = ast.format(&ctx);
            print!("{}", formatted.values().next().unwrap_or(&String::new()));
        }
        CheckResult::Failure { ctx: _, error } => {
            eprintln!("{error}");
        }
    }
    //

    Ok(())
}
