//! # Sand language CLI
//!
//! This module is the main entrypoint for the CLI.
//!
//! ## Usage
//! better documentation is a todo,
//! for now see example commands:
//!
//! ### Compile individual files
//! ```ignore
//! sand compile path/to/file.sand --output path/to/output.o
//! ```

// implementation of the compilation process.
// this module does not contain any compiler logic,
// it just strings together the appropriate compiler passes.
pub mod compile;
pub mod error;
pub mod fmt;

use clap::ArgAction;
use clap::Parser;
use clap::Subcommand;
use tracing::debug;
use tracing::trace;

use crate::compile::CompileArgs;
use crate::compile::compile;
use crate::fmt::FmtArgs;
use crate::fmt::fmt;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct SandCLI {
    /// Sand subcommand to execute
    #[command(subcommand)]
    pub command: SandCommand,

    /// Verbose mode, prints debug info. For trace more try: -vv.
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,

    /// Dry run: run but don't actually affect anything.
    #[arg(short, long, global = true)]
    pub dry: bool,
}

#[derive(Subcommand, Debug)]
pub enum SandCommand {
    #[command()]
    Compile(CompileArgs),
    #[command()]
    Fmt(FmtArgs),
}

fn main() -> Result<(), anyhow::Error> {
    let args = SandCLI::parse();

    let log_level = match args.verbose {
        1 => tracing::Level::DEBUG,
        x if x > 1 => tracing::Level::TRACE,
        _ => {
            if cfg!(debug_assertions) {
                tracing::Level::INFO
            } else {
                tracing::Level::WARN
            }
        }
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_ansi(true)
        .with_line_number(cfg!(debug_assertions))
        .compact()
        .pretty()
        .init();

    debug!(log_level = ?log_level);
    trace!("args: {args:?}");

    match args.command {
        SandCommand::Compile(compile_args) => compile(compile_args, args.dry)?,
        SandCommand::Fmt(fmt_args) => fmt(fmt_args)?,
    }

    Ok(())
}
