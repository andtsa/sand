//! an lsp implementation for our language

use std::collections::BTreeMap;

use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub mod annotate;
pub mod ast;
pub mod backend;
pub mod uniquify;
pub mod util;

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub documents: RwLock<BTreeMap<Url, String>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "kap-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        self.documents
            .write()
            .await
            .insert(uri.clone(), text.clone());
        self.check_document(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.content_changes[0].text.clone();

        self.documents
            .write()
            .await
            .insert(uri.clone(), text.clone());
        self.check_document(uri, text).await;
    }
}
