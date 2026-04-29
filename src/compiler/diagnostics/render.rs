use std::fmt::Display;

use crate::compiler::diagnostics::DiagnosticSeverity;
use crate::compiler::diagnostics::SandDiagnostic;

impl SandDiagnostic {
    /// render the diagnostic as a human-readable string, for now just for
    /// debugging purposes
    pub fn render(&self) -> String {
        // todo: nicely readable diagnostic message,
        // like the ones emitted by rustc.
        format!("{}: {}", self.severity, self.message)
    }
}

impl DiagnosticSeverity {
    /// todo: different rendering styles for severities.
    pub fn render(&self, _ansi: bool) -> &'static str {
        match self {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
            DiagnosticSeverity::CompilerDebug => "compiler debug",
        }
    }
}

impl Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render(false))
    }
}
