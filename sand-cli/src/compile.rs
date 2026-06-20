//! This module contains the implementation of the compilation process.
//! It does not contain any compiler logic, it just strings together the
//! appropriate compiler passes.
//!
//! Other subcommands that implement smaller parts of the compilation process
//! will use this module for functionality (interacting with the backend) and
//! will _only_ implement the frontend / printing.
use std::path::PathBuf;

use clap::Args;
use lang::castles::project::CheckResult;
use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::ir_types::mir::MirProgram;
use lang::ir_types::typed_hir::TypedProgram;
use lang::passes::llvm_codegen::LlvmCodegen;
use lang::util::fs::FileOperations;
use lang::util::fs::real_fs::FileSystem;

use crate::error::CliError;

/// Load a project (from a config file, else from input paths), surface its load
/// warnings plus every check diagnostic (skipping synthetic core files), and
/// return the checked program. Shared by the `compile` and `run` subcommands.
///
/// The returned tuple is `(ctx, ast)`, `ctx` owns the arena `ast` borrows, so
/// this order matches [`CheckResult::Success`]'s drop-order invariant.
pub fn load_and_check(
    config: Option<&PathBuf>,
    input: &[PathBuf],
) -> Result<(CompileCtx<'static>, TypedProgram<'static>), CliError> {
    let project_result = if let Some(config) = config {
        // Load project from config file
        let span = tracing::debug_span!("loading project from config");
        let _g1 = span.enter();
        Project::from_config(config)
    } else {
        // Load input files using [`Project::from_paths`]
        let span = tracing::debug_span!("loading project from paths");
        let _g1 = span.enter();
        Project::from_paths(input)
    }?;
    let project = project_result.project;

    for warning in project_result.warnings {
        eprintln!("{}", warning.to_diagnostic().render(&project));
    }

    tracing::debug!("loaded {} files", project.file_count());

    let span = tracing::debug_span!("compiling modules");
    let _g2 = span.enter();

    let result = project.check();
    // Render every accumulated diagnostic (errors *and* warnings) from the sink,
    // skipping synthetic (core library) files.
    for (fr, file_diags) in &result.diagnostics().map {
        if project.is_synthetic_file(*fr) {
            continue;
        }
        for diag in file_diags {
            eprintln!("{}", diag.render(&project));
        }
    }
    match result {
        CheckResult::Success { ctx, ast, .. } => {
            tracing::debug!(
                "compilation successful with {} functions",
                ast.functions.len()
            );
            ast.functions
                .values()
                .for_each(|f| tracing::trace!(name = ctx.original_fun_name(f.name)));
            Ok((ctx, ast))
        }
        CheckResult::Failure { error, .. } => Err(CliError::CompilerError {
            diagnostic: error.to_string(),
        }),
    }
}

#[derive(Debug, Args)]
pub struct CompileArgs {
    /// Input file(s) to compile
    #[arg(required = true, conflicts_with = "config")]
    input: Vec<PathBuf>,
    /// Compile a project from a config file
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Output file to write
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Emit LLVM IR instead of machine code
    #[arg(short, long)]
    emit_llvm: bool,
    /// Print the AST instead of compiling
    #[arg(short, long)]
    print_ast: bool,
}

pub fn compile(args: CompileArgs, dry_run: bool) -> Result<(), CliError> {
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

    if !args.emit_llvm && output_file.ends_with(".ll") {
        eprintln!("If you want to emit LLVM IR, use the --emit-llvm flag");
    }

    tracing::debug!(
        "outfile_name: {outfile_name}, output_file: {}",
        output_file.display()
    );

    let (ctx, ast) = load_and_check(args.config.as_ref(), &args.input)?;

    if args.print_ast {
        println!("{}", ast.dump(&ctx));
        return Ok(());
    }

    // Emit code
    let mir = MirProgram::from_typed_program(&ast, &ctx);
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
        if let Err(e) = fs.delete_file(&object_file) {
            eprintln!("failed to delete object file: {}", e);
        }
    }

    Ok(())
}
