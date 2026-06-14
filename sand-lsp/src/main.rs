//! implement (basic) language server

pub mod annotate;
pub mod backend;
pub mod diagnostics;
pub mod goto_definition;
pub mod hover;
pub mod lsp;
pub mod protocol;
pub mod util;
use tower_lsp::LspService;
use tower_lsp::Server;
use tracing::debug;
use tracing::info;

use crate::lsp::Backend;

#[tokio::main]
async fn main() {
    // set up tracing to log to stderr
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    info!("starting sand lsp");
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    debug!("creating lsp service");
    let (service, socket) = LspService::new(Backend::with_client);

    debug!("serving lsp service");
    Server::new(stdin, stdout, socket).serve(service).await;
}
