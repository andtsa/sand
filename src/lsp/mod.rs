//! an lsp implementation for our language

use std::collections::BTreeMap;
use std::path::PathBuf;

use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::ModuleRef;
use crate::lsp::config::load_config;
use crate::lsp::files::discover_files;
use crate::lsp::files::read_discovered_files;

pub mod annotate;
pub mod backend;
pub mod config;
pub mod diagnostics;
pub mod files;
pub mod util;

pub struct Backend<'lsp> {
    pub client: Client,
    // todo: identify project root and which files we should be tracking
    pub project_root: RwLock<Option<PathBuf>>,
    pub project_files: RwLock<BTreeMap<Url, String>>,
    pub modules: RwLock<BTreeMap<ModuleRef, Url>>,

    pub files: RwLock<BTreeMap<Url, FileRef>>,

    pub standalone_files: RwLock<BTreeMap<Url, (String, CompileCtx<'lsp>)>>,

    // todo: incremental compilation
    pub context: RwLock<CompileCtx<'lsp>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend<'static> {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root_uri = params.root_uri.as_ref();

        if let Some(uri) = root_uri
            && let Ok(root_path) = uri.to_file_path()
        {
            let mut lock = self.project_root.write().await;
            *lock = Some(root_path.clone());
            // Try to load config first
            match load_config(&root_path).await {
                Ok(Some(config)) => {
                    // Use configured files
                    if let Err(e) = self.apply_config(&config).await {
                        self.log(MessageType::WARNING, format!("error loading config: {e}"))
                            .await;
                    }
                }
                _ => {
                    // Fall back to recursive discovery
                    if let Ok(paths) = discover_files(&root_path).await
                        && let Ok(files) = read_discovered_files(paths).await
                    {
                        let mut lock = self.project_files.write().await;
                        *lock = files;
                    }
                }
            }
        }
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
            .log_message(MessageType::INFO, "sand-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;

        if self.project_files.read().await.contains_key(&uri) {
            self.check_project().await;
        } else {
            let text = params.text_document.text;
            self.standalone_files
                .write()
                .await
                .entry(uri.clone())
                .and_modify(|(t, _)| {
                    *t = text.clone();
                })
                .or_insert({
                    let mut ctx = CompileCtx::initial();
                    if let Err(e) = ctx.default_file(uri.clone()) {
                        self.log(MessageType::ERROR, e).await;
                    };
                    (text, ctx)
                });
            self.check_file(uri).await;
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        if self.project_files.read().await.contains_key(&uri) {
            self.check_project().await;
        } else {
            let text = params.content_changes[0].text.clone();
            self.standalone_files
                .write()
                .await
                .entry(uri.clone())
                .and_modify(|(t, _)| {
                    *t = text.clone();
                })
                .or_insert({
                    let mut ctx = CompileCtx::initial();
                    if let Err(e) = ctx.default_file(uri.clone()) {
                        self.log(MessageType::ERROR, e).await;
                    };
                    (text, ctx)
                });
            self.check_file(uri).await;
        }
    }
}
