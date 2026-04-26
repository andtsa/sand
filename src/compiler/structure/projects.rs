//! types relating to projects and their structure

use std::fmt::Display;

use thiserror::Error;
use tower_lsp::lsp_types::Url;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleRef(pub(in crate::compiler) usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CodeModule {
    pub(in crate::compiler) name: String,
    pub(in crate::compiler) from_file: FileRef,
    pub(in crate::compiler) index: ModuleRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleInfo {
    pub name: String,
    pub index: ModuleRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodeFile {
    /// file can be file:///path/to/file or http://remote/file or ssh://remote/file
    pub(in crate::compiler) uri: tower_lsp::lsp_types::Url,
    pub(in crate::compiler) name: String,
    pub(in crate::compiler) index: FileRef,
    pub(in crate::compiler) default_module: Option<ModuleRef>,
}

/// a reference to a specific code file.
/// implemented as an index into the `context.code_files`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileRef(pub(in crate::compiler) usize);

impl FileRef {
    pub fn test_new(_idx: usize) -> Self {
        #[cfg(not(feature = "testing"))]
        unreachable!("unsafe reference initialisation outside tests");
        #[cfg(feature = "testing")]
        Self(_idx)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize)]
pub struct ProjectConfig {
    //todo
    pub tracked_files: Vec<Url>,
}

#[derive(Debug, Error)]
#[error("there was an error with the uri {uri}: {message}")]
pub struct UriError {
    pub uri: tower_lsp::lsp_types::Url,
    pub message: String,
}
impl UriError {
    pub fn new(uri: Url, message: String) -> Self {
        Self { uri, message }
    }
}

pub fn uri_name(uri: &tower_lsp::lsp_types::Url) -> Result<String, UriError> {
    let mut path_iter = uri.path_segments().ok_or_else(|| {
        UriError::new(
            uri.clone(),
            format!("provided uri {uri} cannot be turned into segments"),
        )
    })?;
    let name = path_iter
        .next_back()
        .ok_or_else(|| UriError::new(uri.clone(), format!("provided uri {uri} seems empty")))?
        .split(".")
        .next()
        .ok_or_else(|| {
            UriError::new(
                uri.clone(),
                format!("provided uri {uri} doesn't end with a file"),
            )
        })?
        .to_string();
    Ok(name)
}

impl Display for ModuleInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
