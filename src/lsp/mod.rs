//! an lsp implementation for our language

use std::collections::BTreeMap;
use std::path::PathBuf;

use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::compiler::context::CompileCtx;
use crate::compiler::context::ProjectCtx;
use crate::ir_types::typed_hir::TypedProgram;
use crate::lsp::config::load_config;
use crate::lsp::diagnostics::Diagnostics;
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
    // project context (persists for the lifetime of the server)
    pub project_root: RwLock<Option<PathBuf>>,
    pub file_contents: RwLock<BTreeMap<Url, String>>,

    pub context: RwLock<ProjectCtx>,

    pub standalone_files: RwLock<BTreeMap<Url, (String, Option<LastCompilation<'lsp>>)>>,

    pub last_compilation: RwLock<Option<LastCompilation<'lsp>>>,
}

pub enum LastCompilation<'cx> {
    Success {
        context: Box<CompileCtx<'cx>>,
        diagnostics: Diagnostics,
        ast: TypedProgram,
    },
    Failure {
        diagnostics: Diagnostics,
    },
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend<'static> {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root_uri = params.root_uri.as_ref();

        if let Some(uri) = root_uri
            && let Ok(root_path) = uri.to_file_path()
        {
            self.log(
                MessageType::INFO,
                format!("initializing with root: {}", root_path.display()),
            )
            .await;

            let mut lock = self.project_root.write().await;
            *lock = Some(root_path.clone());
            // Try to load config first
            match load_config(&root_path).await {
                Ok(Some(config)) => {
                    self.log(
                        MessageType::INFO,
                        format!("loaded project config from {}", root_path.display()),
                    )
                    .await;
                    // Use configured files
                    if let Err(e) = self.apply_config(&config).await {
                        self.log(MessageType::WARNING, format!("error applying config: {e}"))
                            .await;
                    } else {
                        let file_count = self.file_contents.read().await.len();
                        self.log(
                            MessageType::INFO,
                            format!("registered {} project files", file_count),
                        )
                        .await;
                    }
                }
                Ok(None) => {
                    self.log(
                        MessageType::INFO,
                        "no sand.toml found, discovering files...",
                    )
                    .await;
                    // Fall back to recursive discovery
                    if let Ok(paths) = discover_files(&root_path).await
                        && let Ok(files) = read_discovered_files(paths).await
                    {
                        let file_count = files.len();
                        for (url, text) in files.into_iter() {
                            self.register_file(url, text).await;
                        }
                        self.log(
                            MessageType::INFO,
                            format!("discovered {} sand files", file_count),
                        )
                        .await;
                    } else {
                        self.log(MessageType::WARNING, "failed to discover files")
                            .await;
                    }
                }
                Err(e) => {
                    self.log(
                        MessageType::WARNING,
                        format!("error loading sand.toml: {e}"),
                    )
                    .await;
                }
            }
        } else {
            self.log(
                MessageType::WARNING,
                "no root URI provided for initialization",
            )
            .await;
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
        let root = self.project_root.read().await;
        let file_count = self.file_contents.read().await.len();
        let root_display = root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        self.log(
            MessageType::INFO,
            format!(
                "sand-lsp initialized at {} with {} tracked files",
                root_display, file_count
            ),
        )
        .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.log(MessageType::LOG, format!("opening file: {}", uri))
            .await;
        let text = params.text_document.text;
        self.handle_file(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.log(MessageType::LOG, format!("file changed: {}", uri))
            .await;
        let text = params.content_changes[0].text.clone();
        self.handle_file(uri, text).await;
    }
}

impl Backend<'_> {
    async fn handle_file(&self, uri: Url, text: String) {
        if self.file_contents.read().await.contains_key(&uri) {
            self.log(
                MessageType::LOG,
                "file is part of tracked project, re-checking project".to_string(),
            )
            .await;
            self.register_file(uri, text).await;
            self.check_project().await;
        } else {
            self.log(
                MessageType::LOG,
                "file is standalone, updating and re-checking".to_string(),
            )
            .await;
            if let Some(entry) = self.standalone_files.write().await.get_mut(&uri) {
                entry.0 = text;
            } else {
                self.standalone_files
                    .write()
                    .await
                    .insert(uri.clone(), (text, None));
            }
            self.check_file(uri).await;
        }
    }
}
