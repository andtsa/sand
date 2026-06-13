//! convert AstErrors into SandDiagnostics

use pest::error::LineColLocation;

use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::DiagnosticSeverity;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::diagnostics::SdRelatedInfo;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Pos;
use crate::compiler::structure::Range;
use crate::passes::build_ast::AstError;
use crate::passes::parse::Rule;

/// convert an AstError into one or more sand diagnostics
pub fn ast_error_to_diagnostics(
    _ctx: &CompileCtx,
    file: FileRef,
    err: &AstError,
) -> SandDiagnostics {
    let mut diagnostics = SandDiagnostics::default();
    match err {
        AstError::Pest(parse_err) => {
            diagnostics.add_one(file, parse_error_to_diagnostic(file, parse_err))
        }

        AstError::UnexpectedRule {
            expected,
            got,
            range,
        } => {
            let message = format!("unexpected rule: expected {:?}, got {:?}", expected, got);

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: format!("expected: {:?}, got: {:?}", expected, got),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range: *range,
                    file: Some(file),
                    url: None,
                    related: vec![related],
                    module: None,
                },
            );
        }

        AstError::Missing { expected, range } => {
            let message = format!("missing {}", expected);

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: "syntax may be incomplete here".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    url: None,
                    range: *range,
                    severity: DiagnosticSeverity::Error,
                    message,
                    related: vec![related],
                    module: None,
                },
            );
        }

        AstError::InvalidInteger { got, range, source } => {
            let message = format!("invalid integer literal: {}", got);

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: format!(
                    "integer literal must fit in i64 and contain only digits. parsing raised error: {source}"
                ),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    url: None,
                    range: *range,
                    severity: DiagnosticSeverity::Error,
                    message,
                    related: vec![related],
                    module: None,
                },
            );
        }

        AstError::InvalidName { got, range } => {
            let message = format!("invalid name: {}", got);

            let related = SdRelatedInfo {
                file,
                range: *range,
                message: "name is reserved or otherwise invalid".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    range: *range,
                    severity: DiagnosticSeverity::Error,
                    message,
                    related: vec![related],
                    ..Default::default()
                },
            );
        }

        AstError::ContextError(ce) => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: ce.to_string(),
                    ..Default::default()
                },
            );
        }

        AstError::UriError(err) => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: err.to_string(),
                    ..Default::default()
                },
            );
        }

        AstError::UnknownType { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: format!("unknown type '{name}'"),
                    range: *range,
                    ..Default::default()
                },
            );
        }
        AstError::UnknownModule { module, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: format!("unknown module '{module}'"),
                    range: *range,
                    ..Default::default()
                },
            );
        }
        AstError::UnknownRegion { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "unknown lifetime '{name}': declare it as a region parameter, e.g. `<'{name}>`"
                    ),
                    range: *range,
                    ..Default::default()
                },
            );
        }
        AstError::TypeArgArityMismatch {
            name,
            expected,
            found,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "generic type '{name}' expects {expected} type argument(s) but {found} were given"
                    ),
                    range: *range,
                    ..Default::default()
                },
            );
        }
        AstError::RegionArgArityMismatch {
            name,
            expected,
            found,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "type '{name}' expects {expected} lifetime argument(s) but {found} were given"
                    ),
                    range: *range,
                    ..Default::default()
                },
            );
        }
        AstError::RegionArgsNotFirst { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "lifetime arguments must come before type arguments (write `{name}<'a, T>`)"
                    ),
                    range: *range,
                    ..Default::default()
                },
            );
        }
        AstError::PayloadBorrowNeedsLifetime { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "a reference in a payload of '{name}' must use a declared lifetime parameter (e.g. `type {name}<'a> = …(&'a T)`) or `'static`"
                    ),
                    range: *range,
                    ..Default::default()
                },
            );
        }
        AstError::KindArgMismatch {
            type_name,
            param,
            expected,
            found,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "type argument for parameter '{param}' of '{type_name}' has kind {found:?}, but kind {expected:?} is required"
                    ),
                    range: *range,
                    ..Default::default()
                },
            );
        }
        AstError::UnsoundVariance {
            type_name,
            param,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file: Some(file),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "parameter '{param}' of '{type_name}' is declared contravariant but appears in a covariant (producer) position"
                    ),
                    range: *range,
                    ..Default::default()
                },
            );
        }
    }
    diagnostics
}

fn parse_error_to_diagnostic(file: FileRef, err: &pest::error::Error<Rule>) -> SandDiagnostic {
    let (start, end) = match err.line_col {
        LineColLocation::Pos((l, c)) => {
            let p = Pos::new(l, c);
            (p, p)
        }
        LineColLocation::Span((sl, sc), (el, ec)) => {
            let start = Pos::new(sl, sc);
            let end = Pos::new(el, ec);
            (start, end)
        }
    };

    SandDiagnostic {
        range: Range::new_from_pos(start, end),
        severity: DiagnosticSeverity::Error,
        message: err.variant.message().into(),
        file: Some(file),
        ..Default::default()
    }
}
