//! an lsp implementation for our language

use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;

use lang::castles::project::CheckResult;
use lang::castles::project::Project;
use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::lsp_types::*;

pub struct ProjectSlot {
    pub project: Project,
    /// The freshest check outcome *only when it is a `Failure`* (else `None`).
    /// Drives diagnostics together with `last_good`.
    pub last_result: Option<CheckResult>,
    /// The freshest *successful* check. Feature queries (hover, goto,
    /// completion, …) read this, so they keep working off the last good
    /// analysis even while the current edit fails to check.
    pub last_good: Option<CheckResult>,
}

impl ProjectSlot {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            last_result: None,
            last_good: None,
        }
    }

    /// Fold a fresh check outcome into the slot. A `Success` becomes the new
    /// `last_good` and clears any stale `Failure`; a `Failure` is recorded in
    /// `last_result` but **leaves `last_good` intact** — that retained success
    /// is what keeps features alive on broken code.
    pub fn record(&mut self, result: CheckResult) {
        match result {
            r @ CheckResult::Success { .. } => {
                self.last_good = Some(r);
                self.last_result = None;
            }
            r @ CheckResult::Failure { .. } => {
                self.last_result = Some(r);
            }
        }
    }

    /// The result to derive diagnostics from: the current failure if any, else
    /// the last good success (which yields only hints / no errors).
    pub fn diagnostics_source(&self) -> Option<&CheckResult> {
        self.last_result.as_ref().or(self.last_good.as_ref())
    }
}

pub struct Backend {
    pub client: Client,
    pub root: RwLock<Option<PathBuf>>,
    pub slots: RwLock<Vec<ProjectSlot>>,
    pub last_published_uris: RwLock<Vec<Url>>,
    /// Monotonic edit counter per document, used to debounce `did_change`: an
    /// edit records its generation, and the deferred re-check only runs if no
    /// newer edit has bumped the generation in the meantime.
    pub edit_generations: RwLock<HashMap<Url, u64>>,
}

impl Backend {
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            root: RwLock::new(None),
            slots: RwLock::new(vec![]),
            last_published_uris: RwLock::new(vec![]),
            edit_generations: RwLock::new(HashMap::new()),
        }
    }

    /// Record a new edit to `uri` and return its generation.
    pub async fn bump_edit_generation(&self, uri: &Url) -> u64 {
        let mut gens = self.edit_generations.write().await;
        let g = gens.entry(uri.clone()).or_insert(0);
        *g += 1;
        *g
    }

    /// Whether `gen` is still the latest recorded edit generation for `uri`
    /// (i.e. no newer edit has arrived since).
    pub async fn is_latest_edit(&self, uri: &Url, generation: u64) -> bool {
        self.edit_generations.read().await.get(uri).copied() == Some(generation)
    }

    pub async fn log(&self, ty: MessageType, msg: impl Display) {
        eprintln!("{ty:?}: {msg}");
        self.client.log_message(ty, format!("{msg}\n")).await;
    }

    /// Begin a work-done progress for a long-running operation (e.g. the
    /// initial load). Returns the token, or `None` if the client declined
    /// to create it — in which case
    /// [`Self::progress_report`]/[`Self::progress_end`] are no-ops,
    /// so callers don't need to branch. Must be called only after the server is
    /// initialized (tower-lsp drops notifications / errors requests before
    /// then).
    pub async fn progress_begin(&self, title: &str) -> Option<ProgressToken> {
        let token = ProgressToken::String("sand-lsp/load".to_string());
        // Server-initiated progress requires the client to allocate the token; if
        // it refuses (or doesn't support work-done progress), skip silently.
        if self
            .client
            .send_request::<tower_lsp::lsp_types::request::WorkDoneProgressCreate>(
                WorkDoneProgressCreateParams {
                    token: token.clone(),
                },
            )
            .await
            .is_err()
        {
            return None;
        }
        self.send_progress(
            &Some(token.clone()),
            WorkDoneProgress::Begin(WorkDoneProgressBegin {
                title: title.to_string(),
                cancellable: Some(false),
                message: None,
                percentage: None,
            }),
        )
        .await;
        Some(token)
    }

    pub async fn progress_report(
        &self,
        token: &Option<ProgressToken>,
        message: impl Into<String>,
        percentage: Option<u32>,
    ) {
        self.send_progress(
            token,
            WorkDoneProgress::Report(WorkDoneProgressReport {
                cancellable: Some(false),
                message: Some(message.into()),
                percentage,
            }),
        )
        .await;
    }

    pub async fn progress_end(&self, token: Option<ProgressToken>, message: impl Into<String>) {
        self.send_progress(
            &token,
            WorkDoneProgress::End(WorkDoneProgressEnd {
                message: Some(message.into()),
            }),
        )
        .await;
    }

    async fn send_progress(&self, token: &Option<ProgressToken>, value: WorkDoneProgress) {
        let Some(token) = token else { return };
        self.client
            .send_notification::<tower_lsp::lsp_types::notification::Progress>(ProgressParams {
                token: token.clone(),
                value: ProgressParamsValue::WorkDone(value),
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use lang::castles::project::CheckResult;
    use lang::castles::project::Project;

    use super::ProjectSlot;

    fn check(src: &str) -> CheckResult {
        let mut p = Project::empty();
        p.create_virtual_file(src.to_string(), "m");
        p.check()
    }

    #[test]
    fn last_good_survives_a_broken_edit() {
        let mut slot = ProjectSlot::new(Project::empty());

        // A good check becomes `last_good`.
        let good = check("def main(): Int := 42");
        assert!(matches!(good, CheckResult::Success { .. }));
        slot.record(good);
        assert!(matches!(slot.last_good, Some(CheckResult::Success { .. })));
        assert!(slot.last_result.is_none());
        assert!(matches!(
            slot.diagnostics_source(),
            Some(CheckResult::Success { .. })
        ));

        // A failing edit records the failure but must NOT discard `last_good`.
        let bad = check("def main(): Int := true");
        assert!(
            matches!(bad, CheckResult::Failure { .. }),
            "expected failure"
        );
        slot.record(bad);
        assert!(
            matches!(slot.last_good, Some(CheckResult::Success { .. })),
            "last good analysis retained across a broken edit"
        );
        assert!(matches!(
            slot.last_result,
            Some(CheckResult::Failure { .. })
        ));
        // Diagnostics now come from the current failure.
        assert!(matches!(
            slot.diagnostics_source(),
            Some(CheckResult::Failure { .. })
        ));

        // Recovering clears the failure and refreshes `last_good`.
        slot.record(check("def main(): Int := 7"));
        assert!(matches!(slot.last_good, Some(CheckResult::Success { .. })));
        assert!(slot.last_result.is_none());
    }
}
