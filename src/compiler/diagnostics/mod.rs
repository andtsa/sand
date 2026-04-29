//! User-facing diagnostics

pub mod convert;

use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleInfo;
use crate::compiler::structure::Range;

// todo: unimplement clone
#[derive(Debug, Clone)]
pub struct Diagnostics<K: Eq + Ord, V: Clone> {
    pub map: Map<K, Vec<V>>,
}

impl<K: Eq + Ord, V: Clone> Default for Diagnostics<K, V> {
    fn default() -> Self {
        Self {
            map: Map::default(),
        }
    }
}

impl<K, V> Diagnostics<K, V>
where
    K: Eq + Ord + Clone,
    V: Clone,
{
    pub fn add(&mut self, uri: K, mut diagnostics: Vec<V>) {
        self.map
            .entry(uri.clone())
            .and_modify(|e| e.append(&mut diagnostics))
            .or_insert(diagnostics);
    }

    pub fn add_one(&mut self, uri: K, diagnostic: V) {
        self.map
            .entry(uri)
            .and_modify(|e| e.push(diagnostic.clone()))
            .or_insert(vec![diagnostic]);
    }

    pub fn single(uri: K, diagnostic: V) -> Self {
        Self {
            map: Map::from([(uri, vec![diagnostic])]),
        }
    }
}

pub type SandDiagnostics = Diagnostics<FileRef, SandDiagnostic>;

#[derive(Debug, Copy, Clone)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    /// diagnostics meant for debugging the compiler itself,
    /// unrelated to user code
    CompilerDebug,
}

/// A diagnostic message emitted by the compiler.
///
/// - this can only be converted to an LSP diagnostic if there's access to the
///   full text, since [`crate::lsp::util::lsp_range_from_pest`] needs it
#[derive(Debug, Clone)]
pub struct SandDiagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,

    pub range: Range,
    pub file: FileRef,
    pub module: Option<ModuleInfo>,

    pub related: Vec<SdRelatedInfo>,
}

#[derive(Debug, Clone)]
pub struct SdRelatedInfo {
    pub file: FileRef,
    pub range: Range,
    pub message: String,
}
