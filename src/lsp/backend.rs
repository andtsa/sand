//! LSP backend document checking functionality.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::ops::Deref;

use tokio::sync::RwLock;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;
use tower_lsp::lsp_types::Url;

use crate::compile_hir;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Map;
use crate::lsp::Backend;

impl<'lsp> Backend<'lsp> {
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            project_root: RwLock::new(None),
            project_files: RwLock::new(BTreeMap::new()),
            files: RwLock::new(BTreeMap::new()),
            modules: RwLock::new(BTreeMap::new()),

            standalone_files: RwLock::new(BTreeMap::new()),

            context: RwLock::new(CompileCtx::initial()),
        }
    }

    pub async fn log(&self, ty: MessageType, msg: impl Display) {
        self.client.log_message(ty, msg).await
    }

    pub async fn check_project<'run>(&'run self)
    where
        'lsp: 'run,
    {
        let mut modules = BTreeMap::new();
        let project_files = self.project_files.read().await;
        for (m, s) in project_files.iter() {
            if let Some((fr, txt)) = self.files.read().await.get(m).map(|fr| (*fr, s.as_str())) {
                modules.insert(fr, txt);
            }
        }

        let compile_result = {
            let compile_ctx: &mut CompileCtx = &mut *self.context.write().await;
            compile_hir(modules, compile_ctx)
        };
        let diagnostics = match compile_result {
            Ok(ast) => self.annotate_reused_expressions(&ast).await,
            Err(err) => {
                self.sand_diagnostics(self.context.read().await.deref(), err)
                    .await
            }
        };

        self.publish_diagnostics(diagnostics).await;
    }

    pub async fn check_file(&self, uri: Url) {
        if let Some((text, ctx)) = self.standalone_files.write().await.get_mut(&uri) {
            let file_ref = match ctx.default_file(uri.clone()) {
                Ok(file_ref) => file_ref,
                Err(err) => {
                    self.log(MessageType::ERROR, err.message).await;
                    return;
                }
            };
            let _module = Map::from([(file_ref, text.as_str())]);

            todo!()
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
        }
    }
}
