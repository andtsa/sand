//! This module contains the implementation of the compilation process.
//! It does not contain any compiler logic, it just strings together the
//! appropriate compiler passes.
//!
//! Other subcommands that implement smaller parts of the compilation process
//! will use this module for functionality (interacting with the backend) and
//! will _only_ implement the frontend / printing.
use std::path::PathBuf;

use clap::Args;
use sand::SandLangError;
use sand::compile_hir;
use sand::compiler::context::CompileCtx;
use sand::compiler::context::ProjectCtx;
use sand::compiler::structure::Map;
use sand::compiler::structure::UriError;
use sand::ir_types::mir::MirProgram;
use sand::passes::llvm_codegen::CodegenError;
use sand::passes::llvm_codegen::LlvmCodegen;
use sand::util::fs::FileOperations;
use sand::util::fs::error::FsError;
use tower_lsp::lsp_types::Url;

use crate::CliCtx;

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
    Fs(#[from] Box<FsError>),
    #[error("url error: {0}")]
    Url(String),
    #[error(transparent)]
    Uri(#[from] UriError),
    #[error(transparent)]
    Lang(#[from] Box<SandLangError>),
    #[error("llvm error: {0}")]
    Llvm(#[from] CodegenError),
}

pub fn compile<FS: FileOperations>(
    ctx: &mut CliCtx<FS>,
    args: CompileArgs,
) -> Result<(), CompileCliError> {
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

    let mut project_ctx = ProjectCtx::initial();
    let mut compile_ctx = CompileCtx::initial();
    let mut modules = Map::new();

    let span = tracing::debug_span!("collecting modules from files");
    let _g1 = span.enter();

    args.input
        .iter()
        .map(|file| {
            let pb = ctx.fs.canonicalize(file)?;
            tracing::trace!("canonicalized path: {}", pb.display());
            let uri = Url::from_file_path(&pb).map_err(|_| {
                CompileCliError::Url(format!("could not convert {} to a valid URL", pb.display()))
            })?;
            let fr = project_ctx.register_file(uri)?;
            tracing::trace!("file registered as {fr:?}");
            let (name, content) = ctx.fs.read_file(file)?;
            ctx.opened_files.insert(fr, content);
            tracing::debug!("loaded file {name} from {} as {fr:?}", pb.display());
            Ok((fr, name))
        })
        .collect::<Result<Vec<_>, CompileCliError>>()?
        .into_iter()
        .for_each(|(fr, name)| {
            modules.insert(fr, ctx.opened_files[&fr].as_str());
            let _ = compile_ctx.create_default_module(fr, &name);
        });
    drop(_g1);

    let span = tracing::debug_span!("compiling modules");
    let _g2 = span.enter();

    let ast = compile_hir(modules, &mut compile_ctx)?;
    tracing::debug!("compiled hir with {} functions", ast.functions.len());
    ast.functions
        .values()
        .for_each(|f| tracing::trace!(name = compile_ctx.original_fun_name(f.name)));

    let mir = MirProgram::from_typed_program(&ast);
    let llvm_ctx = inkwell::context::Context::create();
    let codegen = LlvmCodegen::new(&llvm_ctx, "sand_module");
    codegen.emit_program(&mir, &compile_ctx)?;
    drop(_g2);

    if args.emit_llvm {
        tracing::debug!("emmiting llvm ir directly");
        codegen.write_ir(output_file, ctx.dry)?;
        return Ok(());
    }

    let object_file = output_file.with_extension("o");
    tracing::debug!("writing object file {}", object_file.display());
    codegen.write_object(&object_file, ctx.dry)?;
    if !ctx.dry {
        LlvmCodegen::link(
            &object_file.display().to_string(),
            &output_file.display().to_string(),
        )?;

        ctx.fs.delete_file(&object_file)?;
    }

    Ok(())
}

impl From<FsError> for CompileCliError {
    fn from(value: FsError) -> Self {
        CompileCliError::Fs(Box::new(value))
    }
}

impl From<SandLangError> for CompileCliError {
    fn from(value: SandLangError) -> Self {
        CompileCliError::Lang(Box::new(value))
    }
}
