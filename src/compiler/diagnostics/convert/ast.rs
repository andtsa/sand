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
    err: AstError,
) -> SandDiagnostics {
    let mut diagnostics = SandDiagnostics::default();
    match err {
        AstError::Pest(parse_err) => {
            diagnostics.add_one(file, parse_error_to_diagnostic(file, *parse_err))
        }

        AstError::UnexpectedRule {
            expected,
            got,
            range,
        } => {
            let message = format!("unexpected rule: expected {:?}, got {:?}", expected, got);

            let related = SdRelatedInfo {
                file,
                range,
                message: format!("expected: {:?}, got: {:?}", expected, got),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message,
                    range,
                    file,
                    related: vec![related],
                    module: None,
                },
            );
        }

        AstError::Missing { expected, range } => {
            let message = format!("missing {}", expected);

            let related = SdRelatedInfo {
                file,
                range,
                message: "syntax may be incomplete here".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file,
                    range,
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
                range,
                message: format!(
                    "integer literal must fit in i64 and contain only digits. parsing raised error: {source}"
                ),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file,
                    range,
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
                range,
                message: "name is reserved or otherwise invalid".into(),
            };

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file,
                    range,
                    severity: DiagnosticSeverity::Error,
                    message,
                    related: vec![related],
                    module: None,
                },
            );
        }

        AstError::ContextError(ce) => {
            let range = crate::compiler::structure::Range::default();
            let message = format!("internal compiler error: {:?}", ce);

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file,
                    range,
                    severity: DiagnosticSeverity::Error,
                    message,
                    related: vec![],
                    module: None,
                },
            );
        }

        AstError::UriError(err) => {
            let range = crate::compiler::structure::Range::default();
            let message = format!("uri error: {}", err.message);

            diagnostics.add_one(
                file,
                SandDiagnostic {
                    file,
                    range,
                    severity: DiagnosticSeverity::Error,
                    message,
                    related: vec![],
                    module: None,
                },
            );
        }
    }
    diagnostics
}

fn parse_error_to_diagnostic(file: FileRef, err: pest::error::Error<Rule>) -> SandDiagnostic {
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
        file,
        related: vec![],
        module: None,
    }
}
