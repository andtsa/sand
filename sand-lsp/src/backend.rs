//! LSP backend document checking functionality.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;

use lang::castles::project::CheckResult;
use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::ir_types::typed_hir::TypedProgram;
use tokio::task::block_in_place;
use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::DiagnosticSeverity;
use tower_lsp::lsp_types::MessageType;
use tower_lsp::lsp_types::Url;

use crate::diagnostics::LspDiagnostics;
use crate::diagnostics::lsp_diagnostics_from_result;
use crate::lsp::Backend;

impl Backend {
    /// Find the slot tracking `uri` and, if its last *good* check succeeded,
    /// run `f` against that analysis (ctx + AST + project). Returns `None`
    /// when no slot tracks the uri or there is no good analysis yet.
    /// Centralises the "scan slots → require `last_good` success"
    /// boilerplate shared by the read-only language features (hover,
    /// goto-definition).
    pub(crate) async fn with_analysis<T>(
        &self,
        uri: &Url,
        f: impl FnOnce(&CompileCtx<'static>, &TypedProgram<'static>, &Project) -> T,
    ) -> Option<T> {
        let slots = self.slots.read().await;
        let slot = slots.iter().find(|s| s.project.is_tracked(uri).is_some())?;
        let CheckResult::Success { ctx, ast, .. } = slot.last_good.as_ref()? else {
            return None;
        };
        Some(f(ctx, ast, &slot.project))
    }
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

    /// Re-check every slot (used at initialization).
    pub async fn check_project(&self) {
        self.recheck(None).await;
    }

    /// Re-check only the slot that tracks `uri` (used on edit / open), leaving
    /// other compilation units untouched.
    pub async fn check_uri(&self, uri: &Url) {
        self.recheck(Some(uri)).await;
    }

    /// Re-check the targeted slot(s), then publish diagnostics for the whole
    /// workspace.
    ///
    /// The (blocking, potentially slow) compiler check runs under a **shared
    /// read lock**, not the exclusive write lock, so hover / goto / formatting
    /// (all readers) stay responsive while a check is in flight. Results are
    /// stored under a brief write lock afterward. Each check is wrapped in
    /// `catch_unwind`, so a compiler panic on in-progress code becomes an error
    /// diagnostic rather than a silently dead request handler.
    async fn recheck(&self, only: Option<&Url>) {
        if self.slots.read().await.is_empty() {
            self.uninit_err().await;
            return;
        }

        // Phase 1: run the checks under a shared read lock.
        let mut fresh: Vec<(usize, CheckResult)> = Vec::new();
        let mut panics: Vec<(usize, String)> = Vec::new();
        {
            let slots = self.slots.read().await;
            for (i, slot) in slots.iter().enumerate() {
                if let Some(uri) = only
                    && slot.project.is_tracked(uri).is_none()
                {
                    continue;
                }
                // IDE check: the pre-monomorphisation, source-faithful program
                // (generic functions keep their real signatures), with type +
                // ownership diagnostics.
                let outcome =
                    block_in_place(|| catch_unwind(AssertUnwindSafe(|| slot.project.check_ide())));
                match outcome {
                    Ok(result) => fresh.push((i, result)),
                    Err(payload) => panics.push((i, panic_message(&payload))),
                }
            }
        }

        // Phase 2: store fresh results under a brief write lock. A `Success`
        // becomes the slot's `last_good` (keeping features alive on later broken
        // edits); a `Failure` is recorded without discarding the last good
        // analysis. Replaced results drop here, freeing their arenas (no leak).
        if !fresh.is_empty() {
            let mut slots = self.slots.write().await;
            for (i, result) in fresh {
                if let Some(slot) = slots.get_mut(i) {
                    slot.record(result);
                }
            }
        }

        for (_, msg) in &panics {
            self.log(
                MessageType::ERROR,
                format!("compiler panicked during check: {msg}"),
            )
            .await;
        }

        // Phase 3: build and publish diagnostics from every slot's cached result,
        // plus any panic diagnostics, and clear stale URIs.
        let mut new_diags = LspDiagnostics::default();
        {
            let slots = self.slots.read().await;
            for slot in slots.iter() {
                // The current failure if any, else the last good success (errors
                // when broken; reuse-hints when clean).
                let Some(result) = slot.diagnostics_source() else {
                    continue;
                };
                let mut slot_diags = lsp_diagnostics_from_result(result, &slot.project);
                if let CheckResult::Success { ctx, ast, .. } = result {
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
            // A panicked check has no `CheckResult`; surface it on every file of
            // the affected slot so the failure is visible rather than silent.
            for (i, msg) in &panics {
                if let Some(slot) = slots.get(*i) {
                    for fr in slot.project.file_contents.keys() {
                        let uri = slot.project.uri_of_file(*fr);
                        new_diags
                            .map
                            .entry(uri)
                            .or_default()
                            .push(internal_error_diagnostic(msg));
                    }
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
                    vec![Diagnostic {
                        range: Default::default(),
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: e.to_string(),
                        ..Default::default()
                    }],
                    None,
                )
                .await;
        }
    }
}

/// Best-effort message from a `catch_unwind` payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// A whole-file diagnostic reporting that the compiler panicked while checking.
fn internal_error_diagnostic(msg: &str) -> Diagnostic {
    Diagnostic {
        range: Default::default(),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("sand".into()),
        message: format!("internal compiler error while checking this file: {msg}"),
        ..Default::default()
    }
}
