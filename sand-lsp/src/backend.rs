//! LSP backend document checking functionality.

use lang::castles::project::CheckResult;
use tokio::task::block_in_place;
use tower_lsp::lsp_types::MessageType;
use tower_lsp::lsp_types::Url;

use crate::diagnostics::lsp_diagnostics_from_result;
use crate::lsp::Backend;

impl Backend {
    pub async fn uninit_err(&self) {
        self.log(
            MessageType::ERROR,
            format!(
                "operation at {:?} called before project was initialized",
                std::panic::Location::caller()
            ),
        )
        .await;
    }

    pub async fn check_project(&self) {
        // Run checks for all slots under a write lock so results are stored atomically.
        {
            let mut slots = self.slots.write().await;
            if slots.is_empty() {
                self.uninit_err().await;
                return;
            }
            for slot in slots.iter_mut() {
                self.log(
                    MessageType::LOG,
                    format!("checking {} files", slot.project.file_count()),
                )
                .await;
                slot.last_result = Some(block_in_place(|| slot.project.check()));
            }
        }

        // Build combined diagnostics from all slots.
        let mut new_diags = crate::diagnostics::LspDiagnostics::default();
        {
            let slots = self.slots.read().await;
            for slot in slots.iter() {
                let Some(result) = slot.last_result.as_ref() else {
                    continue;
                };
                let mut slot_diags = lsp_diagnostics_from_result(result, &slot.project);
                if let CheckResult::Success { ctx, ast } = result {
                    let hints = self
                        .annotate_reused_expressions(ctx, ast, &slot.project)
                        .await;
                    for (uri, diags) in hints.map {
                        slot_diags.map.entry(uri).or_default().extend(diags);
                    }
                }
                for (uri, diags) in slot_diags.map {
                    new_diags.map.entry(uri).or_default().extend(diags);
                }
            }
        }

        // Clear stale diagnostics.
        let stale_uris: Vec<Url> = self.last_published_uris.read().await.clone();
        for uri in &stale_uris {
            if !new_diags.map.contains_key(uri) {
                self.client
                    .publish_diagnostics(uri.clone(), vec![], None)
                    .await;
            }
        }

        // Publish new diagnostics.
        for (uri, diags) in &new_diags.map {
            self.client
                .publish_diagnostics(uri.clone(), diags.clone(), None)
                .await;
        }

        *self.last_published_uris.write().await = new_diags.map.keys().cloned().collect();
    }

    pub async fn update_file(&self, uri: Url, text: String) {
        let mut slots = self.slots.write().await;
        let slot = slots
            .iter_mut()
            .find(|s| s.project.is_tracked(&uri).is_some());
        let Some(slot) = slot else {
            self.log(
                MessageType::WARNING,
                format!("update_file: URI not tracked in any slot: {uri}"),
            )
            .await;
            return;
        };
        if let Err(e) = slot.project.insert_file(uri.clone(), text) {
            self.log(MessageType::ERROR, format!("failed to register {uri}: {e}"))
                .await;
            self.client
                .publish_diagnostics(
                    uri,
                    vec![tower_lsp::lsp_types::Diagnostic {
                        range: Default::default(),
                        severity: Some(tower_lsp::lsp_types::DiagnosticSeverity::ERROR),
                        message: e.to_string(),
                        ..Default::default()
                    }],
                    None,
                )
                .await;
        }
    }
}
