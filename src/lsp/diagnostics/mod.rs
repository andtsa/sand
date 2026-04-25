//! generate diagnostics from top-level compiler error `SandError`

use bimap::BiBTreeMap;
use tower_lsp::lsp_types::*;

use crate::SandLangError;
use crate::SandLangErrorContext;
use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::structure::FileRef;
use crate::lsp::Backend;
use crate::lsp::util::sand_diagnostic_to_lsp;
use crate::lsp::util::url_of_module_unchecked;

pub(super) type Diagnostics = crate::compiler::diagnostics::Diagnostics<Url, Diagnostic>;

pub fn sand_source_diagnostics(
    ctx: &CompileCtx,
    file_map: &BiBTreeMap<Url, FileRef>,
    text: &str,
    sand_err: SandLangError,
) -> Diagnostics {
    let sand_diagnostics = SandDiagnostic::from_compiler_error(ctx, sand_err);
    let mut diagnostics = Diagnostics::default();
    for (file, diag) in sand_diagnostics.map {
        let url = file_map.get_by_right(&file).cloned();
        if let Some(u) = url {
            diagnostics.add(
                u.clone(),
                diag.into_iter()
                    .map(|d| sand_diagnostic_to_lsp(text, d, u.clone()))
                    .collect(),
            );
        }
    }
    diagnostics
}

impl<'lsp> Backend<'lsp> {
    async fn uri_of_context(
        &self,
        ctx: &CompileCtx<'lsp>,
        context: &SandLangErrorContext,
    ) -> Option<Url> {
        match (context.module, context.file) {
            (Some(mr), _) => {
                let url = Some(url_of_module_unchecked(
                    mr,
                    ctx,
                    &self.context.read().await.files,
                ));
                self.log(
                    MessageType::LOG,
                    format!("resolved module context: {:?}", mr),
                )
                .await;
                url
            }
            (None, Some(fr)) => {
                let url = Some(self.context.read().await.url_of_file(fr));
                self.log(MessageType::LOG, format!("resolved file context: {:?}", fr))
                    .await;
                url
            }
            (None, None) => {
                self.log(
                    MessageType::WARNING,
                    "no module or file context available".to_string(),
                )
                .await;
                None
            }
        }
    }

    pub async fn publish_diagnostics(&self, diagnostics: Diagnostics) {
        let total_diagnostics = diagnostics.map.values().map(|v| v.len()).sum::<usize>();
        self.log(
            MessageType::LOG,
            format!("publishing diagnostics to {} files", diagnostics.map.len()),
        )
        .await;

        for (uri, diags) in diagnostics.map {
            let count = diags.len();
            self.log(
                MessageType::LOG,
                format!("publishing {} diagnostics to {}", count, uri),
            )
            .await;
            self.client
                .publish_diagnostics(uri.clone(), diags, None)
                .await;
        }

        self.log(
            MessageType::LOG,
            format!("published {} total diagnostics", total_diagnostics),
        )
        .await;
    }

    pub async fn sand_diagnostics(
        &self,
        ctx: &CompileCtx<'lsp>,
        sand_err: SandLangError,
    ) -> Diagnostics {
        self.log(
            MessageType::LOG,
            "processing project compilation error".to_string(),
        )
        .await;

        let Some(uri) = self.uri_of_context(ctx, &sand_err.context).await else {
            self.log(
                MessageType::ERROR,
                format!(
                    "cannot resolve error context for error: {:?}",
                    sand_err.context
                ),
            )
            .await;
            return Diagnostics::default();
        };

        if let Some(text) = self
            .file_contents
            .read()
            .await
            .get(&uri)
            .map(|s| s.as_str())
        {
            self.log(
                MessageType::LOG,
                format!("converting source diagnostics for {}", uri),
            )
            .await;
            sand_source_diagnostics(ctx, &self.context.read().await.files, text, sand_err)
        } else {
            self.log(
                MessageType::WARNING,
                format!("file text not found for error context: {}", uri),
            )
            .await;
            Diagnostics::default()
        }
    }

    pub async fn sand_individual_diagnostics(
        &self,
        ctx: &CompileCtx<'lsp>,
        sand_err: SandLangError,
    ) -> Diagnostics {
        self.log(
            MessageType::LOG,
            "processing standalone file error".to_string(),
        )
        .await;

        let Some(uri) = self.uri_of_context(ctx, &sand_err.context).await else {
            self.log(
                MessageType::ERROR,
                format!(
                    "cannot resolve error context for standalone file error: {:?}",
                    sand_err.context
                ),
            )
            .await;
            return Diagnostics::default();
        };

        if let Some((text, _)) = self.standalone_files.read().await.get(&uri) {
            self.log(
                MessageType::LOG,
                format!("converting standalone file diagnostics for {}", uri),
            )
            .await;
            sand_source_diagnostics(
                ctx,
                &self.context.read().await.files,
                text.as_str(),
                sand_err,
            )
        } else {
            self.log(
                MessageType::WARNING,
                format!("standalone file not found for error context: {}", uri),
            )
            .await;
            Diagnostics::default()
        }
    }
}
