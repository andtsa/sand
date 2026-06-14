//! the sand compiler
#![allow(clippy::result_large_err)]

use thiserror::Error;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::ir_types::hhir;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedProgram;

pub mod analysis;
pub mod castles;
pub mod compiler;
pub mod interpreter;
pub mod ir_types;
pub mod lang;
pub mod passes;
pub mod util;

pub use util::bugs::*;

#[derive(Debug, Error)]
#[error("compilation error: {kind}")]
pub struct SandLangError<'tcx> {
    pub context: SandLangErrorContext<'tcx>,
    pub kind: SandLangErrorSource<'tcx>,
}

#[derive(Debug, Default)]
pub struct SandLangErrorContext<'tcx> {
    pub module: Option<ModuleRef<'tcx>>,
    pub file: Option<FileRef>,
}

#[derive(Debug, Error)]
pub enum SandLangErrorSource<'tcx> {
    #[error("parse error: {0}")]
    AstParseError(#[from] passes::build_ast::AstError),

    // Note: no `#[from]` — QualifyError<'tcx> is non-'static so it cannot be
    // an error "source". Use the manual From impl below instead.
    #[error("qualify error: {0}")]
    QualifyError(passes::qualify::error::QualifyError<'tcx>),

    // Note: no `#[from]` — AstTypeError<'tcx> is non-'static so it cannot be
    // an error "source". Use the manual From impl below instead.
    #[error("type error: {0}")]
    TypeError(passes::type_ast::AstTypeError<'tcx>),

    #[error("ownership error: {0}")]
    OwnershipError(#[from] passes::ownership::errors::OwnershipError),
}

impl<'tcx> From<passes::type_ast::AstTypeError<'tcx>> for SandLangErrorSource<'tcx> {
    fn from(e: passes::type_ast::AstTypeError<'tcx>) -> Self {
        SandLangErrorSource::TypeError(e)
    }
}

impl<'tcx> From<passes::qualify::error::QualifyError<'tcx>> for SandLangErrorSource<'tcx> {
    fn from(e: passes::qualify::error::QualifyError<'tcx>) -> Self {
        SandLangErrorSource::QualifyError(e)
    }
}

const CORE_SRC: &str = include_str!("core.sand");

pub fn compile_hir<'proj>(
    code: Map<FileRef, &'_ str>,
    ctx: &mut CompileCtx<'proj>,
) -> Result<TypedProgram<'proj>, SandLangError<'proj>> {
    let span = tracing::warn_span!("compile_hir");
    let _enter = span.enter();

    let core_file = ctx.ensure_core_module();
    let core_modules = hhir::ProgramModule::parse_source_file(ctx, CORE_SRC, core_file)
        .map_err(|e| SandLangErrorContext::default().wrap_err(e))?;

    let user_modules = code
        .into_iter()
        .map(|(file, source)| {
            let err_ctx = SandLangErrorContext {
                module: None,
                file: Some(file),
            };
            hhir::ProgramModule::parse_source_file(ctx, source, file)
                .map_err(|e| err_ctx.wrap_err(e))
        })
        .collect::<Result<Vec<Vec<_>>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let modules: Vec<_> = core_modules.into_iter().chain(user_modules).collect();

    tracing::trace!(
        "{} modules: {:?}",
        modules.len(),
        modules
            .iter()
            .map(|m| ctx.module_info(&m.module_name))
            .collect::<Vec<_>>()
    );

    let program = qhir::Program::combine(ctx, modules).map_err(|e| {
        ctx.entrypoint = None;
        SandLangErrorContext::with_module(e.source_module().index).wrap_err(e)
    })?;

    let typed_program = typed_hir::TypedProgram::from_ast_program(ctx, program).map_err(|e| {
        ctx.entrypoint = None;
        SandLangErrorContext::with_module(e.module).wrap_err(e.error)
    })?;

    // Heap lowering (Memory Step C.5): rewrite every `deriving Heaped` enum into
    // a `Unique<Node>` handle over the core-lib allocator *before* ownership, so
    // drops are inserted uniformly on the resulting handles, and before mono, so
    // the injected `unique_*` calls and node types are instantiated normally.
    let typed_program = passes::heap_lower::lower(ctx, typed_program);

    let typed_program = passes::ownership::check(ctx, typed_program)
        .map_err(|e| SandLangErrorContext::with_module(e.module).wrap_err(e.error))?;

    // Monomorphisation erases all type parameters, so every later pass (MIR
    // lowering, codegen) only sees concrete types.
    let typed_program = passes::mono::monomorphise(ctx, &typed_program);

    Ok(typed_program)
}

impl<'tcx> SandLangErrorContext<'tcx> {
    pub fn with_module(module: ModuleRef<'tcx>) -> Self {
        Self {
            module: Some(module),
            file: None,
        }
    }

    pub fn wrap_err<E: Into<SandLangErrorSource<'tcx>>>(self, err: E) -> SandLangError<'tcx> {
        SandLangError {
            context: self,
            kind: err.into(),
        }
    }
}
