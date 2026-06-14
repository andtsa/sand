//! an lsp implementation for our language

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
}

impl Backend {
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            root: RwLock::new(None),
            slots: RwLock::new(vec![]),
            last_published_uris: RwLock::new(vec![]),
        }
    }

    pub async fn log(&self, ty: MessageType, msg: impl Display) {
        eprintln!("{ty:?}: {msg}");
        self.client.log_message(ty, format!("{msg}\n")).await;
    }
}
