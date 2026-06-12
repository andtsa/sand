//! convert internal errors into [`crate::compiler::diagnostics::Diagnostic`]

pub mod ast;
pub mod ownership;
pub mod qualify;
pub mod reused_expr;
pub mod setup;
pub mod typecheck;
pub mod uniquify;

use crate::SandLangError;
use crate::SandLangErrorSource;
use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::diagnostics::convert::ast::ast_error_to_diagnostics;
use crate::compiler::diagnostics::convert::ownership::ownership_error_to_diagnostic;
use crate::compiler::diagnostics::convert::qualify::qualify_error_to_diagnostics;
use crate::compiler::diagnostics::convert::typecheck::type_error_to_diagnostic;
use crate::internal_bug;

impl SandDiagnostic {
    pub fn from_compiler_error<'tcx>(
        ctx: &CompileCtx<'tcx>,
        error: &SandLangError<'tcx>,
    ) -> SandDiagnostics {
        let source_file = error
            .context
            .file
            .or_else(|| error.context.module.map(|mr| ctx.file_of_module(mr)))
            .unwrap_or_else(|| internal_bug!("error with no source context"));
        match &error.kind {
            SandLangErrorSource::AstParseError(err) => {
                ast_error_to_diagnostics(ctx, source_file, err)
            }
            SandLangErrorSource::QualifyError(err) => {
                qualify_error_to_diagnostics(ctx, source_file, err)
            }
            SandLangErrorSource::TypeError(err) => type_error_to_diagnostic(ctx, source_file, err),
            SandLangErrorSource::OwnershipError(err) => {
                ownership_error_to_diagnostic(ctx, source_file, err)
            }
        }
    }
}
