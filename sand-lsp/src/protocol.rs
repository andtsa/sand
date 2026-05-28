//! LSP protocol implementation for sand-lsp

use lang::castles::discovery::discover_files;
use lang::castles::project::Project;
use lang::castles::project::init::ProjectCreationResult;
use tokio::task::spawn_blocking;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::diagnostics::setup_warning_to_lsp;
use crate::hover;
use crate::lsp::Backend;
use tracing::debug;
use tracing::info;
use tracing::warn;
use tracing::error;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        debug!("initialising sand-lsp");
        let Some(uri) = params.root_uri.as_ref() else {
            debug!("no root uri provided for initialisation");
            return Ok(Self::capabilities());
        };
        let Ok(root_path) = uri.to_file_path() else {
            debug!("invalid root uri during initialisation: {uri}");
            return Ok(Self::capabilities());
        };

        *self.root.write().await = Some(root_path.clone());
        info!("initialised sand-lsp with root: {}", root_path.display());

        let (project, warnings) = match Project::from_rootdir(&root_path) {
            Ok(result) => (result.project, result.warnings),
            Err(e) => {
                error!("config error: {e}");
                // fall back to discovery
                let paths = match spawn_blocking(|| discover_files(root_path))
                    .await
                    .map_err(anyhow::Error::from)   // JoinError -> anyhow
                    .and_then(|r| r.map_err(anyhow::Error::from))  // io::Error -> anyhow
                {
                    Ok(paths) => paths,
                    Err(e) => {
                        error!("discovery failed: {e}");
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
            warn!("{}", warning.message);
            self.log(MessageType::WARNING, &warning.message).await;
        }
        // publish them against the sand.toml URI
        if let Some(cfg_path) = project.config_url() {
            self.client
                .publish_diagnostics(
                    cfg_path,
                    warnings.iter().map(setup_warning_to_lsp).collect(),
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

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let result_guard = self.last_result.read().await;
        let Some(lang::castles::project::CheckResult::Success { ctx, ast }) = result_guard.as_ref()
        else {
            return Ok(None);
        };
        let project_guard = self.project.read().await;
        let Some(project) = project_guard.as_ref() else {
            return Ok(None);
        };
        let uri = &params.text_document_position_params.text_document.uri;
        let lsp_pos = params.text_document_position_params.position;
        Ok(hover::hover_at_position(lsp_pos, uri, ctx, ast, project))
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
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
