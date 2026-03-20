//! generate diagnostics from top-level compiler error `SandError`

pub mod ast;
pub mod qualify;
pub mod typecheck;
pub mod uniquify;

use tower_lsp::lsp_types::*;

use crate::SandError;
use crate::SandErrorContext;
use crate::SandErrorSource;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Map;
use crate::lsp::Backend;
use crate::lsp::diagnostics::ast::ast_error_to_diagnostics;
use crate::lsp::diagnostics::qualify::qualify_error_to_diagnostics;
use crate::lsp::diagnostics::typecheck::type_error_to_diagnostic;

#[derive(Debug, Default)]
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
    uri: Url,
    text: &str,
    sand_err: SandErrorSource,
) -> Diagnostics {
    match sand_err {
        SandErrorSource::AstParseError(err) => ast_error_to_diagnostics(ctx, uri, text, err),
        SandErrorSource::QualifyError(err) => qualify_error_to_diagnostics(ctx, uri, text, err),
        SandErrorSource::TypeError(err) => type_error_to_diagnostic(ctx, uri, text, err),
    }
}

impl Backend<'_> {
    async fn uri_of_context(&self, context: &SandErrorContext) -> Option<Url> {
        match (context.module, context.file) {
            (Some(mr), _) => Some(self.context.read().await.url_of_module(mr)),
            (None, Some(fr)) => Some(self.context.read().await.url_of_file(fr)),
            (None, None) => None,
        }
    }

    pub async fn publish_diagnostics(&self, diagnostics: Diagnostics) {
        for (uri, diagnostics) in diagnostics.map {
            self.client
                .publish_diagnostics(uri.clone(), diagnostics, None)
                .await;
        }
    }

    pub async fn sand_diagnostics<'lsp>(
        &self,
        ctx: &CompileCtx<'lsp>,
        sand_err: SandError,
    ) -> Diagnostics {
        let SandError { source, context } = sand_err;
        let Some(uri) = self.uri_of_context(&context).await else {
            self.log(
                MessageType::ERROR,
                "no module or file context for error {sand_err}",
            )
            .await;
            return Diagnostics::default();
        };
        if let Some(text) = self
            .project_files
            .read()
            .await
            .get(&uri)
            .map(|s| s.as_str())
        {
            sand_source_diagnostics(ctx, uri, text, source)
        } else {
            Diagnostics::default()
        }
    }

    pub async fn sand_individual_diagnostics(&self, sand_err: SandError) -> Diagnostics {
        let SandError { source, context } = sand_err;
        let Some(uri) = self.uri_of_context(&context).await else {
            self.log(
                MessageType::ERROR,
                "no module or file context for error {sand_err}",
            )
            .await;
            return Diagnostics::default();
        };
        if let Some((text, ctx)) = self.standalone_files.read().await.get(&uri) {
            sand_source_diagnostics(ctx, uri, text.as_str(), source)
        } else {
            Diagnostics::default()
        }
    }
}
