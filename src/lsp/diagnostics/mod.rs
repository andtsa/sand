//! generate diagnostics from top-level compiler error `SandError`

use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DiagnosticRelatedInformation;
use tower_lsp::lsp_types::DiagnosticSeverity;
use tower_lsp::lsp_types::Location;
use tower_lsp::lsp_types::Url;

use crate::castles::project::CheckResult;
use crate::castles::project::Project;
use crate::castles::project::init::SetupWarning;
use crate::compiler::diagnostics::DiagnosticSeverity as SandDiagnosticSeverity;
use crate::compiler::diagnostics::Diagnostics;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::lsp::util::lsp_range_from_pest;

pub type LspDiagnostics = Diagnostics<Url, Diagnostic>;

/// Convert a CheckResult into LSP diagnostics, using the project
/// to look up the correct source text for each file.
pub fn lsp_diagnostics_from_result(result: &CheckResult, project: &Project) -> LspDiagnostics {
    let (_ctx, sand_diags) = match result {
        CheckResult::Success { ctx, ast: _ } => {
            // todo: we could have some "success" diagnostics here, e.g. warnings about
            // unused items
            (ctx, SandDiagnostics::default())
        }
        CheckResult::Failure { ctx, error } => {
            let diags = SandDiagnostic::from_compiler_error(ctx, error);
            (ctx, diags)
        }
    };

    let mut out = Diagnostics::default();
    for (file_ref, file_diags) in sand_diags.map {
        let uri = project.uri_of_file(file_ref);
        // look up the correct source text for THIS file (fixes B9)
        let text = project.text_for_file(file_ref).unwrap_or("");
        let lsp_diags = file_diags
            .into_iter()
            .map(|d| sand_diagnostic_to_lsp(text, d, uri.clone()))
            .collect();
        out.add(uri, lsp_diags);
    }
    out
}

pub(super) fn sand_diagnostic_severity_to_lsp(
    severity: SandDiagnosticSeverity,
) -> DiagnosticSeverity {
    use SandDiagnosticSeverity::*;
    match severity {
        Error => DiagnosticSeverity::ERROR,
        Warning => DiagnosticSeverity::WARNING,
        Info => DiagnosticSeverity::INFORMATION,
        CompilerDebug => DiagnosticSeverity::HINT,
    }
}

pub(super) fn sand_diagnostic_to_lsp(text: &str, diag: SandDiagnostic, uri: Url) -> Diagnostic {
    let range = lsp_range_from_pest(text, diag.range);
    let related_information = if !diag.related.is_empty() {
        Some(
            diag.related
                .into_iter()
                .map(|related| DiagnosticRelatedInformation {
                    location: Location {
                        uri: uri.clone(),
                        range: lsp_range_from_pest(text, related.range),
                    },
                    message: related.message,
                })
                .collect(),
        )
    } else {
        None
    };

    Diagnostic {
        range,
        severity: Some(sand_diagnostic_severity_to_lsp(diag.severity)),
        source: Some("sand".into()),
        message: diag.message,
        related_information,
        ..Default::default()
    }
}

pub fn setup_warning_to_lsp(warning: &SetupWarning) -> Diagnostic {
    Diagnostic {
        message: warning.message.clone(),
        severity: Some(DiagnosticSeverity::WARNING),
        source: Some("sand".into()),
        related_information: Some(vec![DiagnosticRelatedInformation {
            location: Location {
                uri: warning.url.clone(),
                range: Default::default(),
            },
            message: format!("{:?}", warning.kind), /* todo: better formatting for warning
                                                     * kind */
        }]),
        ..Default::default()
    }
}
