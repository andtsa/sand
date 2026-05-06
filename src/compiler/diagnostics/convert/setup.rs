use crate::castles::project::init::SetupWarning;
use crate::compiler::diagnostics::DiagnosticSeverity;
use crate::compiler::diagnostics::SandDiagnostic;

impl SetupWarning {
    pub fn to_diagnostic(&self) -> SandDiagnostic {
        SandDiagnostic {
            severity: DiagnosticSeverity::Warning,
            message: self.message.clone(),
            url: Some(self.url.clone()),
            ..Default::default()
        }
    }
}
