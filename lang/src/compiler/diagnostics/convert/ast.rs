//! convert AstErrors into SandDiagnostics

use pest::error::LineColLocation;

use crate::compiler::context::CompileCtx;
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
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *range,
                    format!("unexpected rule: expected {expected:?}, got {got:?}"),
                    SdRelatedInfo {
                        file,
                        range: *range,
                        message: format!("expected: {expected:?}, got: {got:?}"),
                    },
                ),
            );
        }

        AstError::Missing { expected, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *range,
                    format!("missing {expected}"),
                    SdRelatedInfo {
                        file,
                        range: *range,
                        message: "syntax may be incomplete here".into(),
                    },
                ),
            );
        }

        AstError::InvalidInteger { got, range, source } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *range,
                    format!("invalid integer literal: {got}"),
                    SdRelatedInfo {
                        file,
                        range: *range,
                        message: format!(
                            "integer literal must fit in i64 and contain only digits. parsing raised error: {source}"
                        ),
                    },
                ),
            );
        }

        AstError::InvalidName { got, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error_with(
                    file,
                    *range,
                    format!("invalid name: {got}"),
                    SdRelatedInfo {
                        file,
                        range: *range,
                        message: "name is reserved or otherwise invalid".into(),
                    },
                ),
            );
        }

        AstError::ContextError(ce) => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(file, Range::default(), ce.to_string()),
            );
        }

        AstError::UriError(err) => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(file, Range::default(), err.to_string()),
            );
        }

        AstError::UnknownType { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(file, *range, format!("unknown type '{name}'")),
            );
        }
        AstError::UnknownModule { module, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(file, *range, format!("unknown module '{module}'")),
            );
        }
        AstError::UnknownRegion { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "unknown lifetime '{name}': declare it as a region parameter, e.g. `<'{name}>`"
                    ),
                ),
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
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "generic type '{name}' expects {expected} type argument(s) but {found} were given"
                    ),
                ),
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
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "type '{name}' expects {expected} lifetime argument(s) but {found} were given"
                    ),
                ),
            );
        }
        AstError::MalformedUse { range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    "malformed `use`: expected `use module::name;` or `use module::*;`".to_string(),
                ),
            );
        }
        AstError::RegionArgsNotFirst { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "lifetime arguments must come before type arguments (write `{name}<'a, T>`)"
                    ),
                ),
            );
        }
        AstError::PayloadBorrowNeedsLifetime { name, range } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "a reference in a payload of '{name}' must use a declared lifetime parameter (e.g. `type {name}<'a> = …(&'a T)`) or `'static`"
                    ),
                ),
            );
        }
        // Typeclass declaration errors: the `#[error]` Display already
        // carries a full message; surface it with the variant's range.
        err @ (AstError::UnknownTypeclass { range, .. }
        | AstError::TypeclassParamArity { range, .. }
        | AstError::UnknownSuperclass { range, .. }
        | AstError::DuplicateMethodName { range, .. }
        | AstError::NonInstanceableType { range }
        | AstError::UnknownMethod { range, .. }
        | AstError::MissingMethod { range, .. }
        | AstError::DuplicateInstance { range, .. }
        | AstError::OrphanInstance { range, .. }
        | AstError::MissingSuperclass { range, .. }
        | AstError::CopyPayloadNotCopy { range }
        | AstError::CopyOnGenericType { range }
        | AstError::NonFfiSafeType { range, .. }
        | AstError::NotDerivable { range, .. }
        | AstError::DuplicateDerive { range, .. }
        | AstError::RecursiveTypeNeedsHeaped { range, .. }
        | AstError::NotATypeConstructor { range, .. }
        | AstError::TypeConstructorNotApplied { range, .. }) => {
            diagnostics.add_one(file, SandDiagnostic::error(file, *range, err.to_string()));
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
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "type argument for parameter '{param}' of '{type_name}' has kind {found:?}, but kind {expected:?} is required"
                    ),
                ),
            );
        }
        AstError::UnsoundVariance {
            type_name,
            param,
            range,
        } => {
            diagnostics.add_one(
                file,
                SandDiagnostic::error(
                    file,
                    *range,
                    format!(
                        "parameter '{param}' of '{type_name}' is declared contravariant but appears in a covariant (producer) position"
                    ),
                ),
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

    SandDiagnostic::error(
        file,
        Range::new_from_pos(start, end),
        err.variant.message().into(),
    )
}
