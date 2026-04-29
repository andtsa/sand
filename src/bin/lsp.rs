//! implement (basic) language server

use sand::lsp::Backend;
use tower_lsp::LspService;
use tower_lsp::Server;

#[tokio::main]
async fn main() {
    eprintln!("starting sand lsp");
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    eprintln!("creating lsp service");
    let (service, socket) = LspService::new(Backend::with_client);

    eprintln!("serving lsp service");
    Server::new(stdin, stdout, socket).serve(service).await;
}
