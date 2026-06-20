//! convert qualify errors to SandDiagnostics

use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::diagnostics::SdRelatedInfo;
use crate::compiler::diagnostics::convert::uniquify::uniquify_error_to_diagnostics;
use crate::compiler::structure::FileRef;
use crate::passes::qualify::error::QualifyError;

pub fn qualify_error_to_diagnostics<'tcx>(
    ctx: &CompileCtx<'tcx>,
    file: FileRef,
    err: &QualifyError<'tcx>,
) -> SandDiagnostics {
    let mut diagnostics = SandDiagnostics::default();
    match err {
        QualifyError::DuplicateFunction {
            name,
            module,
            first_instance,
            second_instance,
        } => {
            let file = ctx.file_of_module(module.index);
            let message = format!("function '{name}' is already defined in this module");

            let related = vec![
                SdRelatedInfo {
                    file,
                    range: *first_instance,
                    message: "first definition is here".into(),
                },
                SdRelatedInfo {
                    file,
                    range: *second_instance,
                    message: "second definition is here".into(),
                },
            ];

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    related,
                    ..SandDiagnostic::error(file, *first_instance, message.clone())
                },
            );

            diagnostics.add_one(file, SandDiagnostic::error(file, *second_instance, message));
        }

        QualifyError::DuplicateMain {
            first,
            second,
            first_module,
            second_module,
        } => {
            let file_1 = ctx.file_of_module(first_module.index);
            let file_2 = ctx.file_of_module(second_module.index);
            let message = "main function is already defined! you can only have one main function per project.".to_string();

            // Each diagnostic references *its own* module's file, matching the
            // key it is filed under (so cross-module duplicate `main`s point at
            // the right source on each side).
            diagnostics.add_one(
                file_1,
                SandDiagnostic::error(file_1, *first, message.clone()),
            );
            diagnostics.add_one(file_2, SandDiagnostic::error(file_2, *second, message));
        }

        QualifyError::DuplicateModule(dm) => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    crate::compiler::structure::Range::default(),
                    format!("module '{}' is already defined", dm.name),
                ),
            );
        }

        QualifyError::FunctionQualFailedFunctionNotFound {
            func,
            module,
            source_module,
            range,
        } => {
            let file = ctx.file_of_module(source_module.index);
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *range,
                    format!(
                        "function '{}' is not defined in module '{}'",
                        func, module.name
                    ),
                    SdRelatedInfo {
                        file,
                        range: *range,
                        message: "offending function call is here".into(),
                    },
                ),
            );
        }

        QualifyError::FunctionQualFailedModuleNotFound {
            func,
            module,
            source_module,
            range,
        } => {
            let file = ctx.file_of_module(source_module.index);
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!("module '{module}' is not found for function '{func}'"),
                ),
            );
        }

        QualifyError::UniquifyError { module, source } => {
            let file = ctx.file_of_module(module.index);
            return uniquify_error_to_diagnostics(ctx, file, source);
        }

        QualifyError::ModuleNotFound {
            module,
            source_module,
            range,
        } => {
            let file = ctx.file_of_module(source_module.index);
            diagnostics.add_one(
                file,
                SandDiagnostic::error(file, *range, format!("module '{module}' was not found")),
            );
        }

        QualifyError::UnknownConstructorType {
            name,
            range,
            source_module,
        } => {
            let file = ctx.file_of_module(source_module.index);
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!("unknown enum type '{name}' in constructor expression"),
                ),
            );
        }

        QualifyError::UnknownVariant {
            type_name,
            variant,
            range,
            source_module,
        } => {
            let file = ctx.file_of_module(source_module.index);
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!("unknown variant '{variant}' on enum type '{type_name}'"),
                ),
            );
        }
        QualifyError::UnknownPatternType {
            name,
            range,
            source_module,
        } => {
            let file = ctx.file_of_module(source_module.index);
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!("unknown enum type '{name}' used in match pattern"),
                ),
            );
        }
        QualifyError::TurbofishUnsupported {
            func,
            range,
            source_module,
        } => {
            let file = ctx.file_of_module(source_module.index);
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "explicit type arguments on '{func}' are not supported yet (only on `size_of`)"
                    ),
                ),
            );
        }
    }
    diagnostics
}
