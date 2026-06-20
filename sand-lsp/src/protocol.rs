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
        // Keep `initialize` fast: just record the root and return capabilities.
        // The heavy discovery + initial type-check is deferred to `initialized`,
        // because tower-lsp suppresses progress notifications until the server is
        // initialized — so progress can only be reported from `initialized` on.
        debug!("initialising sand-lsp");
        match params.root_uri.as_ref().and_then(|u| u.to_file_path().ok()) {
            Some(root_path) => {
                info!("initialised sand-lsp with root: {}", root_path.display());
                *self.root.write().await = Some(root_path);
            }
            None => debug!("no usable root uri provided for initialisation"),
        }
        Ok(Self::capabilities())
    }

    async fn initialized(&self, _: InitializedParams) {
        // Now that the handshake is complete, load the workspace with progress.
        self.load_workspace().await;

        let slots_guard = self.slots.read().await;
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
        self.update_file(uri.clone(), text).await;
        self.check_uri(&uri).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let lsp_pos = params.text_document_position_params.position;
        // `with_analysis` uses the last *good* analysis, so hover keeps working
        // while the current edit doesn't check.
        Ok(self
            .with_analysis(uri, |ctx, ast, project| {
                hover::hover_at_position(lsp_pos, uri, ctx, ast, project)
            })
            .await
            .flatten())
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let lsp_pos = params.text_document_position_params.position;
        Ok(self
            .with_analysis(uri, |ctx, ast, project| {
                goto_definition::definition_at_position(lsp_pos, uri, ctx, ast, project)
                    .map(GotoDefinitionResponse::Scalar)
            })
            .await
            .flatten())
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let slots_guard = self.slots.read().await;
        for slot in slots_guard.iter() {
            let Some(file_ref) = slot.project.is_tracked(uri) else {
                continue;
            };
            // Formatting *rewrites* the document, so it must reflect the current
            // text — never a stale AST. Only format when the current check
            // succeeded (no pending failure); `last_good` then matches the file.
            if slot.last_result.is_some() {
                return Ok(None);
            }
            let Some(lang::castles::project::CheckResult::Success { ctx, ast, .. }) =
                slot.last_good.as_ref()
            else {
                return Ok(None);
            };
            let formatted = ast.format(ctx);
            let Some(new_text) = formatted.get(&file_ref) else {
                return Ok(None);
            };
            let current_text = slot.project.text_for_file(file_ref).unwrap_or("");
            let end = doc_end(current_text);
            return Ok(Some(vec![TextEdit {
                range: Range::new(Position::new(0, 0), end),
                new_text: new_text.clone(),
            }]));
        }
        Ok(None)
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // Use the last change. with full sync this is always the complete document
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        // Apply the edit immediately so the document state is always current,
        // then debounce the (expensive) re-check: wait ~300ms and only run if no
        // newer edit superseded this one. Rapid typing coalesces into one check.
        self.update_file(uri.clone(), change.text).await;
        let generation = self.bump_edit_generation(&uri).await;
        tokio::time::sleep(std::time::Duration::from_millis(DEBOUNCE_MS)).await;
        if self.is_latest_edit(&uri, generation).await {
            self.check_uri(&uri).await;
        }
    }
}

/// Debounce window for re-checking after an edit.
const DEBOUNCE_MS: u64 = 300;

/// Compute the LSP `Position` of the very end of `text` (UTF-16 columns).
fn doc_end(text: &str) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for ch in text.chars() {
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    Position::new(line, col)
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
                document_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Discover and load every project / loose file under the workspace root,
    /// then run the initial check — reporting `$/progress` for each phase so
    /// the editor shows a loading indicator. Runs from `initialized`
    /// (progress notifications are dropped before the server is
    /// initialized).
    async fn load_workspace(&self) {
        let Some(root_path) = self.root.read().await.clone() else {
            return; // no root (e.g. single-file client with no workspace)
        };

        let token = self.progress_begin("sand: loading").await;

        // Phase 1: find all sand.toml files recursively and register each project.
        self.progress_report(&token, "discovering projects", Some(10))
            .await;
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

        let mut slots: Vec<ProjectSlot> = vec![];
        let config_total = config_paths.len().max(1);
        for (i, config_path) in config_paths.into_iter().enumerate() {
            self.progress_report(
                &token,
                format!("loading project {}/{config_total}", i + 1),
                Some(10 + (30 * i as u32 / config_total as u32)),
            )
            .await;
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
                    slots.push(ProjectSlot::new(result.project));
                }
                Err(e) => {
                    error!("failed to load config {config_path:?}: {e}");
                }
            }
        }

        // Phase 2: discover all .sand files; create a standalone slot for each
        // file not already tracked by a config-based project.
        self.progress_report(&token, "discovering files", Some(45))
            .await;
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
            slots.push(ProjectSlot::new(result.project));
        }

        *self.slots.write().await = slots;

        // Phase 3: the initial type-check of everything.
        self.progress_report(&token, "type-checking", Some(70))
            .await;
        self.check_project().await;

        self.progress_end(token, "ready").await;
    }
}
