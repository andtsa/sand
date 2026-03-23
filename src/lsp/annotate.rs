//! generate diagnostics for expression annotation

use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DiagnosticSeverity;
use tower_lsp::lsp_types::MessageType;

use crate::analysis::ProgramAnnotations;
use crate::analysis::analyse;
use crate::analysis::interactions::has_other_side_effects;
use crate::compiler::context::CompileCtx;
use crate::ir_types::typed_hir::TypedProgram;
use crate::lsp::Backend;
use crate::lsp::diagnostics::Diagnostics;
use crate::lsp::util::lsp_range_from_pest;

impl Backend<'_> {
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
        ast: &TypedProgram,
    ) -> Diagnostics {
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

        // produce diagnostics for keys with more than one occurrence
        let mut diagnostics: Diagnostics = Diagnostics::default();
        for (e, occs) in annotations.expr_occurrences.into_iter() {
            // // NOTE: whether we include this check or not
            // // depends on how the annotations are made
            // if occs.len() <= 1 {
            //     continue;
            // }
            if has_other_side_effects(&e) {
                continue;
            }

            for (module, range) in occs {
                let uri = self
                    .context
                    .read()
                    .await
                    .url_of_file(ctx.file_of_module(module))
                    .clone();
                if let Some(text) = self.file_contents.read().await.get(&uri) {
                    let range = lsp_range_from_pest(text, range);

                    let message = format!("reused expression: {:?}", e.expr);

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
