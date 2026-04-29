use tower_lsp::lsp_types;

use crate::castles::project::init::SetupWarning;

impl SetupWarning {
    pub fn to_diagnostic(&self) -> lsp_types::Diagnostic {
        use lsp_types::Diagnostic;
        use lsp_types::DiagnosticSeverity;
        Diagnostic {
            range: Default::default(),
            severity: Some(DiagnosticSeverity::WARNING),
            message: self.message.clone(),
            ..Default::default()
        }
    }
}
