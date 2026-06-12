//! generate diagnostics for expression annotation

use lang::analysis::ProgramAnnotations;
use lang::analysis::analyse;
use lang::analysis::interactions::has_other_side_effects;
use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::ir_types::typed_hir::TypedProgram;
use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DiagnosticSeverity;
use tower_lsp::lsp_types::MessageType;

use crate::diagnostics::LspDiagnostics;
use crate::lsp::Backend;
use crate::util::lsp_range_from_pest;

impl Backend {
    /// if an expression without side effects appears multiple times in the
    /// code, we can compute its value just once,
    /// and reuse everywhere else.
    ///
    /// we want to show this to the user by highlighting the repeated
    /// expressions:
    ///
    /// ```ignore
    /// def main(): Int := {
    ///     let x: Int = 5;
    ///     let y: Int = x ^ x;
    ///     let i: Int = 4;
    ///     while i ≥ 0 do {
    ///         y = y + (x ^ x);
    ///         i = i - 1;
    ///     }
    ///     y
    /// }
    /// ```
    /// in this example, both instances of `x ^ x` should be highlighted,
    /// indicating a reused value
    pub async fn annotate_reused_expressions<'run, 'lsp>(
        &'run self,
        ctx: &'run CompileCtx<'lsp>,
        ast: &TypedProgram<'lsp>,
        project: &'run Project,
    ) -> LspDiagnostics {
        self.log(
            MessageType::LOG,
            "analyzing expressions for reuse patterns".to_string(),
        )
        .await;

        let annotations: ProgramAnnotations = analyse(ctx, ast);
        let expr_count = annotations.expr_occurrences.len();
        self.log(
            MessageType::LOG,
            format!("found {} expressions to analyze", expr_count),
        )
        .await;

        let mut diagnostics: LspDiagnostics = LspDiagnostics::default();

        for (e, occs) in annotations.expr_occurrences.into_iter() {
            // // NOTE: whether we include this check or not
            // // depends on how the annotations are made
            if occs.len() <= 1 {
                continue;
            }
            if has_other_side_effects(&e) {
                continue;
            }

            for (module, range) in occs {
                let fr = ctx.file_of_module(module);
                let uri = project.uri_of_file(fr).clone();
                if let Some(text) = project.text_for_file(fr) {
                    let range = lsp_range_from_pest(text, range);

                    let message = format!("reused expression: {}", e.fmt_inline(ctx));

                    diagnostics.add_one(
                        uri.clone(),
                        Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::HINT),
                            source: Some("sand".into()),
                            message,
                            ..Default::default()
                        },
                    );
                };
            }
        }

        diagnostics
    }
}
