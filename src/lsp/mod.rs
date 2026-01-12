//! an lsp implementation for our language

use std::collections::BTreeMap;

use pest::Parser;
use pest::error::LineColLocation;
use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::parse::LangParser;
use crate::parse::Rule;

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub documents: RwLock<BTreeMap<Url, String>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "kap-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        self.documents
            .write()
            .await
            .insert(uri.clone(), text.clone());
        self.check_document(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.content_changes[0].text.clone();

        self.documents
            .write()
            .await
            .insert(uri.clone(), text.clone());
        self.check_document(uri, text).await;
    }
}

impl Backend {
    async fn check_document(&self, uri: Url, text: String) {
        let diagnostics = match LangParser::parse(Rule::program, &text) {
            Ok(_) => Vec::new(),
            Err(err) => vec![parse_error_to_diagnostic(&text, err)],
        };

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

fn position_from_line_col(text: &str, line: usize, col: usize) -> Position {
    // pest reports 1-based line/col; convert to 0-based
    let line_idx = line.saturating_sub(1);
    let col_idx = col.saturating_sub(1);

    // get the text of the line (lines() drops the newline)
    let line_str = text.lines().nth(line_idx).unwrap_or("");

    // take `col_idx` rust chars, then count UTF-16 code units (LSP uses UTF-16)
    let prefix: String = line_str.chars().take(col_idx).collect();
    let utf16_col = prefix.encode_utf16().count();

    Position::new(line_idx as u32, utf16_col as u32)
}

fn parse_error_to_diagnostic(text: &str, err: pest::error::Error<Rule>) -> Diagnostic {
    let (start, end) = match err.line_col {
        LineColLocation::Pos((l, c)) => {
            let p = position_from_line_col(text, l, c);
            (p, p)
        }
        LineColLocation::Span((sl, sc), (el, ec)) => {
            let start = position_from_line_col(text, sl, sc);
            let end = position_from_line_col(text, el, ec);
            (start, end)
        }
    };

    Diagnostic {
        range: Range::new(start, end),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("kap".into()),
        message: err.variant.message().into(),
        ..Default::default()
    }
}
