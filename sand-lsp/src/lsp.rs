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
    pub last_result: Option<CheckResult>,
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
}
