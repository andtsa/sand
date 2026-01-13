//! implement (basic) language server

use std::collections::BTreeMap;

use tokio::sync::RwLock;
use tower_lsp::LspService;
use tower_lsp::Server;
use untitled::lsp::Backend;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: RwLock::new(BTreeMap::new()),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
