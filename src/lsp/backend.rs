//! LSP backend document checking functionality.

use tower_lsp::lsp_types::MessageType;
use tower_lsp::lsp_types::Url;

use crate::castles::project::CheckResult;
use crate::lsp::Backend;
use crate::lsp::diagnostics::lsp_diagnostics_from_result;

impl Backend {
    pub async fn check_project(&self) {
        let project_guard = self.project.read().await;
        let Some(project) = project_guard.as_ref() else {
            self.log(
                MessageType::WARNING,
                "check_project called before project was initialized",
            )
            .await;
            return;
        };

        self.log(
            MessageType::LOG,
            format!("checking {} files", project.file_count()),
        )
        .await;

        let result = project.check();

        // Build the new diagnostics map from this result
        let mut new_diags = lsp_diagnostics_from_result(&result, project);

        if let CheckResult::Success { ctx, ast } = &result {
            let hints = self.annotate_reused_expressions(ctx, ast).await;
            for (uri, diags) in hints.map {
                new_diags.map.entry(uri).or_default().extend(diags);
            }
        }

        // Clear stale diagnostics: any URI that had diagnostics last time
        // but isn't in the new map needs an explicit empty publish
        let stale_uris: Vec<Url> = self.last_published_uris.read().await.clone();
        for uri in &stale_uris {
            if !new_diags.map.contains_key(uri) {
                self.client
                    .publish_diagnostics(uri.clone(), vec![], None)
                    .await;
            }
        }

        // Publish new diagnostics
        for (uri, diags) in &new_diags.map {
            self.client
                .publish_diagnostics(uri.clone(), diags.clone(), None)
                .await;
        }

        *self.last_result.write().await = Some(result);
        *self.last_published_uris.write().await = new_diags.map.keys().cloned().collect();
    }

    pub async fn update_file(&self, uri: Url, text: String) {
        let mut project_guard = self.project.write().await;
        let Some(project) = project_guard.as_mut() else {
            self.log(
                MessageType::WARNING,
                format!("update_file called before init: {uri}"),
            )
            .await;
            return;
        };
        if let Err(e) = project.insert_file(uri.clone(), text) {
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
