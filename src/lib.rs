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
pub mod compiler;
pub mod interpreter;
pub mod ir_types;
pub mod lang;
pub mod lsp;
pub mod passes;
pub mod util;

pub use util::bugs::*;

#[derive(Debug, Error)]
#[error("compilation error: {source}")]
pub struct SandError {
    pub context: SandErrorContext,
    pub source: SandErrorSource,
}

#[derive(Debug, Default)]
pub struct SandErrorContext {
    pub module: Option<ModuleRef>,
    pub file: Option<FileRef>,
}

#[derive(Debug, Error)]
pub enum SandErrorSource {
    #[error("parse error: {0}")]
    AstParseError(#[from] passes::build_ast::AstError),

    #[error("qualify error: {0}")]
    QualifyError(#[from] passes::qualify::error::QualifyError),

    #[error("type error: {0}")]
    TypeError(#[from] passes::type_ast::AstTypeError),
}

pub fn compile_hir<'run, 'proj>(
    code: Map<FileRef, &'_ str>,
    ctx: &'run mut CompileCtx<'proj>,
) -> Result<TypedProgram, SandError> {
    let mut modules = Vec::new();
    for (file, source) in code {
        let err_ctx = SandErrorContext {
            module: None,
            file: Some(file),
        };
        modules.append(
            &mut hhir::ProgramModule::parse_source_file(ctx, source, file)
                .map_err(|e| err_ctx.wrap_err(e))?,
        );
    }

    let program = qhir::Program::combine(ctx, modules)
        .map_err(|e| SandErrorContext::with_module(e.source_module().index).wrap_err(e))?;

    let typed_program = typed_hir::TypedProgram::from_ast_program(ctx, program)
        .map_err(|e| SandErrorContext::with_module(e.module).wrap_err(e.error))?;

    Ok(typed_program)
}

impl SandErrorContext {
    pub fn with_module(module: ModuleRef) -> Self {
        Self {
            module: Some(module),
            file: None,
        }
    }

    pub fn wrap_err<E: Into<SandErrorSource>>(self, err: E) -> SandError {
        SandError {
            context: self,
            source: err.into(),
        }
    }
}
