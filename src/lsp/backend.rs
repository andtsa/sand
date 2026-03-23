//! LSP backend document checking functionality.

use std::collections::BTreeMap;
use std::fmt::Display;

use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;
use tower_lsp::lsp_types::Url;

use crate::compile_hir;
use crate::compiler::context::CompileCtx;
use crate::compiler::context::ProjectCtx;
use crate::lsp::Backend;
use crate::lsp::LastCompilation;

impl<'lsp> Backend<'lsp> {
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            project_root: RwLock::new(None),
            file_contents: RwLock::new(BTreeMap::new()),
            last_compilation: RwLock::new(None),

            standalone_files: RwLock::new(BTreeMap::new()),

            context: RwLock::new(ProjectCtx::initial()),
        }
    }

    pub async fn log(&self, ty: MessageType, msg: impl Display) {
        eprintln!("{ty:?}:{msg}");
        self.client.log_message(ty, format!("{msg}\n")).await
    }

    pub async fn check_project<'run>(&'run self)
    where
        'lsp: 'run,
    {
        self.log(MessageType::LOG, "starting project check...")
            .await;
        let mut modules = BTreeMap::new();
        let project_files = self.file_contents.read().await;
        self.log(
            MessageType::LOG,
            format!(
                "found {} project files: {project_files:?}",
                project_files.len()
            ),
        )
        .await;
        for (m, s) in project_files.iter() {
            if let Some(fr) = self.context.read().await.files.get_by_left(m) {
                modules.insert(*fr, s.as_str());
            }
        }

        self.log(
            MessageType::LOG,
            format!("compiling {} modules", modules.len()),
        )
        .await;

        let mut ctx = CompileCtx::initial();
        let last_compilation = match compile_hir(modules, &mut ctx) {
            Ok(ast) => {
                self.log(
                    MessageType::LOG,
                    "compilation successful, analyzing expressions".to_string(),
                )
                .await;
                let diagnostics = self.annotate_reused_expressions(&ctx, &ast).await;
                LastCompilation::Success {
                    context: Box::new(ctx),
                    diagnostics,
                    ast,
                }
            }
            Err(err) => {
                self.log(
                    MessageType::WARNING,
                    "compilation failed, generating diagnostics".to_string(),
                )
                .await;
                LastCompilation::Failure {
                    diagnostics: self.sand_diagnostics(&ctx, err).await,
                }
            }
        };

        let diagnostic_count = last_compilation
            .diagnostics()
            .map
            .values()
            .map(|v| v.len())
            .sum::<usize>();
        self.log(
            MessageType::LOG,
            format!("publishing {} diagnostics", diagnostic_count),
        )
        .await;
        self.publish_diagnostics(last_compilation.diagnostics().clone())
            .await;
        self.last_compilation
            .write()
            .await
            .replace(last_compilation);
        self.log(MessageType::LOG, "project check complete".to_string())
            .await;
    }

    pub async fn check_file(&self, uri: Url) {
        self.log(
            MessageType::LOG,
            format!("starting standalone file check: {}", uri),
        )
        .await;

        // if let Some((text, ctx)) =
        // self.standalone_files.write().await.get_mut(&uri) {
        //     let file_ref = match ctx.default_file(uri.clone()) {
        //         Ok(file_ref) => {
        //             self.log(
        //                 MessageType::LOG,
        //                 format!("registered file with ref: {:?}", file_ref),
        //             )
        //             .await;
        //             file_ref
        //         }
        //         Err(err) => {
        //             self.log(
        //                 MessageType::ERROR,
        //                 format!("failed to register file: {}", err.message),
        //             )
        //             .await;
        //             return;
        //         }
        //     };
        //     let _module = Map::from([(file_ref, text.as_str())]);

        //     self.log(
        //         MessageType::LOG,
        //         "standalone file check incomplete (todo)".to_string(),
        //     )
        //     .await;
        // todo!()
        // let diagnostics = match compile_hir(module) {
        //     Ok((_, ctx)) => {
        //         todo!()
        //     }
        //     Err(err) => {
        //         self.sand_individual_diagnostics(err).await
        //     }
        // };

        // self
        //     .publish_diagnostics(diagnostics, )
        //     .await;
        // } else {
        //     self.log(
        //         MessageType::WARNING,
        //         format!("file not found in standalone files: {}", uri),
        //     )
        //     .await;
        // }
    }
}
