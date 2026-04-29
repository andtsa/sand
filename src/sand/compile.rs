//! This module contains the implementation of the compilation process.
//! It does not contain any compiler logic, it just strings together the
//! appropriate compiler passes.
//!
//! Other subcommands that implement smaller parts of the compilation process
//! will use this module for functionality (interacting with the backend) and
//! will _only_ implement the frontend / printing.
use std::path::PathBuf;

use clap::Args;
use sand::castles::project::CheckResult;
use sand::castles::project::Project;
use sand::castles::project::init::FatalProjectCreationError;
use sand::compiler::diagnostics::SandDiagnostic;
use sand::ir_types::mir::MirProgram;
use sand::passes::llvm_codegen::CodegenError;
use sand::passes::llvm_codegen::LlvmCodegen;
use sand::util::fs::FileOperations;
use sand::util::fs::error::FsError;
use sand::util::fs::real_fs::FileSystem;

#[derive(Debug, Args)]
pub struct CompileArgs {
    /// Input file(s) to compile
    #[arg(required = true)]
    input: Vec<PathBuf>,
    /// Output file to write
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Emit LLVM IR instead of machine code
    #[arg(short, long)]
    emit_llvm: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CompileCliError {
    #[error("fs error: {0}")]
    Fs(Box<FsError>),
    #[error("project initialization error: {0}")]
    ProjectInit(Box<FatalProjectCreationError>),
    #[error("compiler error")]
    CompilerError { diagnostic: String },
    #[error("llvm error: {0}")]
    Llvm(#[from] CodegenError),
}

pub fn compile(args: CompileArgs, dry_run: bool) -> Result<(), CompileCliError> {
    let span = tracing::info_span!("compile subcommand");
    let _g = span.enter();

    let outfile_name = if args.input.len() == 1 {
        args.input[0]
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("a")
    } else {
        "a"
    };

    let output_file = args.output.unwrap_or_else(|| {
        if args.emit_llvm {
            PathBuf::from(format!("{}.ll", outfile_name))
        } else {
            PathBuf::from(format!("{}.out", outfile_name))
        }
    });

    tracing::debug!(
        "outfile_name: {outfile_name}, output_file: {}",
        output_file.display()
    );

    // Load input files using Project::from_paths
    let span = tracing::debug_span!("loading project from paths");
    let _g1 = span.enter();

    let project_result = Project::from_paths(&args.input)?;
    let project = project_result.project;

    for warning in project_result.warnings {
        tracing::warn!("project setup warning: {}", warning.message);
    }

    tracing::debug!("loaded {} files", project.file_count());
    drop(_g1);

    // Perform compilation check using the Project abstraction
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
            (ctx, ast)
        }
        CheckResult::Failure { ctx, error } => {
            // Convert compiler errors to diagnostics and print them
            let diags = SandDiagnostic::from_compiler_error(&ctx, &error);
            for (file_ref, file_diags) in diags.map {
                let uri = project.uri_of_file(file_ref);
                for diag in file_diags {
                    let severity_str = match diag.severity {
                        sand::compiler::diagnostics::DiagnosticSeverity::Error => "error",
                        sand::compiler::diagnostics::DiagnosticSeverity::Warning => "warning",
                        sand::compiler::diagnostics::DiagnosticSeverity::Info => "info",
                        sand::compiler::diagnostics::DiagnosticSeverity::CompilerDebug => "debug",
                    };
                    eprintln!("{}: {}: {}", uri, severity_str, diag.message);
                }
            }
            return Err(CompileCliError::CompilerError {
                diagnostic: error.to_string(),
            });
        }
    };
    drop(_g2);

    // Emit code
    let mir = MirProgram::from_typed_program(&ast);
    let llvm_ctx = inkwell::context::Context::create();
    let codegen = LlvmCodegen::new(&llvm_ctx, "sand_module");
    codegen.emit_program(&mir, &ctx)?;

    if args.emit_llvm {
        tracing::debug!("emitting llvm ir directly");
        codegen.write_ir(&output_file, dry_run)?;
        return Ok(());
    }

    let object_file = output_file.with_extension("o");
    tracing::debug!("writing object file {}", object_file.display());
    codegen.write_object(&object_file, dry_run)?;

    if !dry_run {
        LlvmCodegen::link(
            &object_file.display().to_string(),
            &output_file.display().to_string(),
        )?;

        let fs = FileSystem { dry_run };
        fs.delete_file(&object_file)?;
    }

    Ok(())
}
