//! generate diagnostics from top-level compiler error `SandError`

pub mod ast;
pub mod qualify;
pub mod typecheck;
pub mod uniquify;

use bimap::BiBTreeMap;
use tower_lsp::lsp_types::*;

use crate::SandError;
use crate::SandErrorContext;
use crate::SandErrorSource;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::lsp::Backend;
use crate::lsp::diagnostics::ast::ast_error_to_diagnostics;
use crate::lsp::diagnostics::qualify::qualify_error_to_diagnostics;
use crate::lsp::diagnostics::typecheck::type_error_to_diagnostic;
use crate::lsp::util::url_of_module_unchecked;

// todo: unimplement clone
#[derive(Debug, Default, Clone)]
pub struct Diagnostics {
    pub map: Map<Url, Vec<Diagnostic>>,
}

impl Diagnostics {
    pub fn add(&mut self, uri: Url, mut diagnostics: Vec<Diagnostic>) {
        self.map
            .entry(uri.clone())
            .and_modify(|e| e.append(&mut diagnostics))
            .or_insert(diagnostics);
    }
    pub fn add_one(&mut self, uri: Url, diagnostic: Diagnostic) {
        self.map
            .entry(uri)
            .and_modify(|e| e.push(diagnostic.clone()))
            .or_insert(vec![diagnostic]);
    }
    pub fn single(uri: Url, diagnostic: Diagnostic) -> Self {
        Self {
            map: Map::from([(uri, vec![diagnostic])]),
        }
    }
}

pub fn sand_source_diagnostics(
    ctx: &CompileCtx,
    file_map: &BiBTreeMap<Url, FileRef>,
    uri: Url,
    text: &str,
    sand_err: SandErrorSource,
) -> Diagnostics {
    match sand_err {
        SandErrorSource::AstParseError(err) => ast_error_to_diagnostics(ctx, uri, text, err),
        SandErrorSource::QualifyError(err) => {
            qualify_error_to_diagnostics(ctx, file_map, uri, text, err)
        }
        SandErrorSource::TypeError(err) => type_error_to_diagnostic(ctx, uri, text, err),
    }
}

impl<'lsp> Backend<'lsp> {
    async fn uri_of_context(
        &self,
        ctx: &CompileCtx<'lsp>,
        context: &SandErrorContext,
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
        sand_err: SandError,
    ) -> Diagnostics {
        self.log(
            MessageType::LOG,
            "processing project compilation error".to_string(),
        )
        .await;

        let SandError { source, context } = sand_err;
        let Some(uri) = self.uri_of_context(ctx, &context).await else {
            self.log(
                MessageType::ERROR,
                format!("cannot resolve error context for error: {:?}", context),
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
            sand_source_diagnostics(ctx, &self.context.read().await.files, uri, text, source)
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
        sand_err: SandError,
    ) -> Diagnostics {
        self.log(
            MessageType::LOG,
            "processing standalone file error".to_string(),
        )
        .await;

        let SandError { source, context } = sand_err;
        let Some(uri) = self.uri_of_context(ctx, &context).await else {
            self.log(
                MessageType::ERROR,
                format!(
                    "cannot resolve error context for standalone file error: {:?}",
                    context
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
                uri,
                text.as_str(),
                source,
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
