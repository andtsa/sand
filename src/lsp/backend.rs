//! LSP backend document checking functionality.

use tower_lsp::lsp_types::*;

use crate::ir_types::hhir::Program;
use crate::ir_types::typed_hir::TypedProgram;
use crate::lsp::Backend;
use crate::lsp::annotate::annotate_reused_expressions;
use crate::lsp::ast::ast_error_to_diagnostics;
use crate::lsp::typecheck::type_error_to_diagnostic;
use crate::lsp::uniquify::uniquify_error_to_diagnostic;

impl Backend {
    pub async fn check_document(&self, uri: Url, text: String) {
        let diagnostics = match Program::parse(&text) {
            Ok(program) => {
                // parsed & AST built successfully
                match program.uniquify() {
                    Ok(ast) => {
                        let reuse_annotations = annotate_reused_expressions(&text, &ast);
                        match TypedProgram::from_ast_program(&ast) {
                            Ok(_p) => reuse_annotations,
                            Err(type_error) => type_error_to_diagnostic(&uri, &text, type_error)
                                .into_iter()
                                .chain(reuse_annotations)
                                .collect(),
                        }
                    }
                    Err(uniquify_error) => {
                        uniquify_error_to_diagnostic(&uri, &text, uniquify_error)
                    }
                }
            }
            Err(ast_error) => ast_error_to_diagnostics(&uri, &text, ast_error),
        };

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}
