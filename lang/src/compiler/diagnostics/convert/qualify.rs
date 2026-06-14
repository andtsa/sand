//! convert qualify errors to SandDiagnostics

use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::DiagnosticSeverity;
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
            let message = format!("function '{}' is already defined in this module", name);

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
                    severity: DiagnosticSeverity::Error,
                    message: message.clone(),
                    range: *first_instance,
                    file: Some(file),
                    related,
                    ..Default::default()
                },
            );

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *second_instance,
                    file: Some(file),
                    ..Default::default()
                },
            );
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

            diagnostics.add_one(
                file_1,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: message.clone(),
                    range: *first,
                    file: Some(file),
                    ..Default::default()
                },
            );

            diagnostics.add_one(
                file_2,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *second,
                    file: Some(file),
                    ..Default::default()
                },
            );
        }

        QualifyError::DuplicateModule(dm) => {
            let message = format!("module '{}' is already defined", dm.name);

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: crate::compiler::structure::Range::default(),
                    file: Some(file),
                    ..Default::default()
                },
            );
        }

        QualifyError::FunctionQualFailedFunctionNotFound {
            func,
            module,
            source_module,
            range,
        } => {
            let file = ctx.file_of_module(source_module.index);
            let message = format!(
                "function '{}' is not defined in module '{}'",
                func, module.name
            );

            let related = vec![SdRelatedInfo {
                file,
                range: *range,
                message: "offending function call is here".into(),
            }];

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *range,
                    file: Some(file),
                    related,
                    ..Default::default()
                },
            );
        }

        QualifyError::FunctionQualFailedModuleNotFound {
            func,
            module,
            source_module,
            range,
        } => {
            let file = ctx.file_of_module(source_module.index);
            let message = format!("module '{}' is not found for function '{}'", module, func);

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *range,
                    file: Some(file),
                    ..Default::default()
                },
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
            let message = format!("module '{}' was not found", module);

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *range,
                    file: Some(file),
                    ..Default::default()
                },
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
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!("unknown enum type '{name}' in constructor expression"),
                    range: *range,
                    file: Some(file),
                    ..Default::default()
                },
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
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!("unknown variant '{variant}' on enum type '{type_name}'"),
                    range: *range,
                    file: Some(file),
                    ..Default::default()
                },
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
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!("unknown enum type '{name}' used in match pattern"),
                    range: *range,
                    file: Some(file),
                    ..Default::default()
                },
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
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "explicit type arguments on '{func}' are not supported yet (only on `size_of`)"
                    ),
                    range: *range,
                    file: Some(file),
                    ..Default::default()
                },
            );
        }
    }
    diagnostics
}
