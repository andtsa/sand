//! LSP protocol implementation for sand-lsp

use tokio::task::spawn_blocking;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::castles::discovery::discover_files;
use crate::castles::project::Project;
use crate::castles::project::init::ProjectCreationResult;
use crate::castles::project::init::SetupWarning;
use crate::lsp::Backend;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let Some(uri) = params.root_uri.as_ref() else {
            return Ok(Self::capabilities());
        };
        let Ok(root_path) = uri.to_file_path() else {
            return Ok(Self::capabilities());
        };

        *self.root.write().await = Some(root_path.clone());

        let (project, warnings) = match Project::from_config(&root_path) {
            Ok(result) => (result.project, result.warnings),
            Err(e) => {
                self.log(MessageType::WARNING, format!("config error: {e}"))
                    .await;
                // fall back to discovery
                let paths = match spawn_blocking(|| discover_files(root_path))
                    .await
                    .map_err(anyhow::Error::from)   // JoinError -> anyhow
                    .and_then(|r| r.map_err(anyhow::Error::from))  // io::Error -> anyhow
                {
                    Ok(paths) => paths,
                    Err(e) => {
                        self.log(MessageType::ERROR, format!("discovery failed: {e}")).await;
                        vec![]
                    }
                };
                let result =
                    Project::from_paths(&paths).unwrap_or_else(|_| ProjectCreationResult {
                        project: Project::empty(),
                        warnings: vec![],
                    });
                (result.project, result.warnings)
            }
        };

        // surface setup warnings as LSP diagnostics on the config file
        for warning in &warnings {
            self.log(MessageType::WARNING, &warning.message).await;
        }
        // publish them against the sand.toml URI
        if let Some(cfg_path) = project.config_url() {
            self.client
                .publish_diagnostics(
                    cfg_path,
                    warnings.iter().map(SetupWarning::to_diagnostic).collect(),
                    None,
                )
                .await;
        }

        *self.project.write().await = Some(project);
        self.check_project().await;

        Ok(Self::capabilities())
    }

    async fn initialized(&self, _: InitializedParams) {
        let project_guard = self.project.read().await;
        let Some(project) = project_guard.as_ref() else {
            self.log(
                MessageType::ERROR,
                "initialized called before project was set up",
            )
            .await;
            return;
        };
        let root = self.root.read().await;
        let file_count = project.file_count();
        let root_display = root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown??>".to_string());
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
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.update_file(uri, text).await;
        self.check_project().await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // Use the last change — with full sync this is always the complete document
        if let Some(change) = params.content_changes.into_iter().last() {
            self.update_file(uri, change.text).await;
            self.check_project().await;
        }
    }
}

impl Backend {
    fn capabilities() -> InitializeResult {
        InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL, // fixes B2
                )),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
