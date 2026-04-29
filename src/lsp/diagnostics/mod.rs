//! generate diagnostics from top-level compiler error `SandError`

use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::Url;

use crate::castles::project::CheckResult;
use crate::castles::project::Project;
use crate::compiler::diagnostics::Diagnostics;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::lsp::util::sand_diagnostic_to_lsp;

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
