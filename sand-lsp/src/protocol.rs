//! LSP protocol implementation for sand-lsp

use lang::castles::discovery::discover_configs;
use lang::castles::discovery::discover_files;
use lang::castles::project::Project;
use lang::castles::project::init::ProjectCreationResult;
use tokio::task::spawn_blocking;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::diagnostics::setup_warning_to_lsp;
use crate::goto_definition;
use crate::hover;
use crate::lsp::Backend;
use crate::lsp::ProjectSlot;

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

        let mut slots: Vec<ProjectSlot> = vec![];

        // Phase 1: find all sand.toml files recursively and register each as a project.
        let config_paths = match spawn_blocking({
            let root_path = root_path.clone();
            move || discover_configs(root_path)
        })
        .await
        .map_err(anyhow::Error::from)
        .and_then(|r| r.map_err(anyhow::Error::from))
        {
            Ok(paths) => paths,
            Err(e) => {
                error!("config discovery failed: {e}");
                vec![]
            }
        };

        for config_path in config_paths {
            match Project::from_config(&config_path) {
                Ok(result) => {
                    for warning in &result.warnings {
                        warn!("{}", warning.message);
                        self.log(MessageType::WARNING, &warning.message).await;
                    }
                    if let Some(cfg_uri) = result.project.config_url() {
                        self.client
                            .publish_diagnostics(
                                cfg_uri,
                                result.warnings.iter().map(setup_warning_to_lsp).collect(),
                                None,
                            )
                            .await;
                    }
                    slots.push(ProjectSlot {
                        project: result.project,
                        last_result: None,
                    });
                }
                Err(e) => {
                    error!("failed to load config {config_path:?}: {e}");
                }
            }
        }

        // Phase 2: discover all .sand files; create a standalone slot for each
        // file not already tracked by a config-based project.
        let all_sand_files = match spawn_blocking({
            let root_path = root_path.clone();
            move || discover_files(root_path)
        })
        .await
        .map_err(anyhow::Error::from)
        .and_then(|r| r.map_err(anyhow::Error::from))
        {
            Ok(paths) => paths,
            Err(e) => {
                error!("file discovery failed: {e}");
                vec![]
            }
        };

        for path in all_sand_files {
            let Ok(file_uri) = Url::from_file_path(&path) else {
                continue;
            };
            if slots
                .iter()
                .any(|s| s.project.is_tracked(&file_uri).is_some())
            {
                continue;
            }
            let result = match Project::from_paths(&[path]) {
                Ok(r) => r,
                Err(e) => {
                    error!("failed to create slot: {e}");
                    ProjectCreationResult {
                        project: Project::empty(),
                        warnings: vec![],
                    }
                }
            };
            for warning in &result.warnings {
                warn!("{}", warning.message);
                self.log(MessageType::WARNING, &warning.message).await;
                self.client
                    .publish_diagnostics(
                        warning.url.clone(),
                        vec![setup_warning_to_lsp(warning)],
                        None,
                    )
                    .await;
            }
            slots.push(ProjectSlot {
                project: result.project,
                last_result: None,
            });
        }

        *self.slots.write().await = slots;
        self.check_project().await;

        Ok(Self::capabilities())
    }

    async fn initialized(&self, _: InitializedParams) {
        let slots_guard = self.slots.read().await;
        if slots_guard.is_empty() {
            self.log(
                MessageType::ERROR,
                "initialized called before project was set up",
            )
            .await;
            return;
        }
        let slot_count = slots_guard.len();
        let total_files: usize = slots_guard.iter().map(|s| s.project.file_count()).sum();
        let root = self.root.read().await;
        let root_display = root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown??>".to_string());
        self.log(
            MessageType::INFO,
            format!(
                "sand-lsp initialized at {} with {} compilation units, {} total tracked files",
                root_display, slot_count, total_files
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
        let uri = &params.text_document_position_params.text_document.uri;
        let lsp_pos = params.text_document_position_params.position;
        let slots_guard = self.slots.read().await;
        for slot in slots_guard.iter() {
            if slot.project.is_tracked(uri).is_some() {
                let Some(lang::castles::project::CheckResult::Success { ctx, ast }) =
                    slot.last_result.as_ref()
                else {
                    return Ok(None);
                };
                return Ok(hover::hover_at_position(
                    lsp_pos,
                    uri,
                    ctx,
                    ast,
                    &slot.project,
                ));
            }
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let lsp_pos = params.text_document_position_params.position;
        let slots_guard = self.slots.read().await;
        for slot in slots_guard.iter() {
            if slot.project.is_tracked(uri).is_some() {
                let Some(lang::castles::project::CheckResult::Success { ctx, ast }) =
                    slot.last_result.as_ref()
                else {
                    return Ok(None);
                };
                let loc = goto_definition::definition_at_position(
                    lsp_pos,
                    uri,
                    ctx,
                    ast,
                    &slot.project,
                );
                return Ok(loc.map(GotoDefinitionResponse::Scalar));
            }
        }
        Ok(None)
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
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
