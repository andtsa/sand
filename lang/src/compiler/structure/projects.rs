//! types relating to projects and their structure

use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;

use thiserror::Error;
use url::Url;

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
    pub(in crate::compiler) uri: url::Url,
    pub(in crate::compiler) name: FileName,
    pub(in crate::compiler) index: FileRef,
    pub(in crate::compiler) default_module: Option<ModuleRef>,
}

impl CodeFile {
    pub fn local_path(&self) -> Option<PathBuf> {
        if self.uri.scheme() == "file" {
            self.uri.to_file_path().ok()
        } else {
            None
        }
    }

    pub fn module_name(&self) -> String {
        self.name.to_string()
    }

    pub fn file_name(&self) -> String {
        match &self.name {
            FileName::Simple(s) => format!("{s}.sand"),
            _ => self.name.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, thiserror::Error)]
pub enum FileName {
    #[error("{0}")]
    Simple(String),
    #[error("<virtual:{0}>")]
    Virtual(String),
    #[error("<dummy_file>")]
    Dummy,
    #[error("<unknown>")]
    Unknown,
}

impl From<Option<String>> for FileName {
    fn from(value: Option<String>) -> Self {
        value.map(FileName::Simple).unwrap_or(FileName::Unknown)
    }
}

impl FileName {
    pub fn try_from_uri(uri: &Url) -> Result<Self, UriError> {
        Self::extract(uri)
            .map(FileName::Simple)
            .ok_or_else(|| UriError::name_fail(uri))
    }

    pub fn dummy() -> Self {
        FileName::Dummy
    }

    pub fn virt(name: &str) -> Self {
        FileName::Virtual(name.to_string())
    }

    /// not sure if there's a point to this since file name is needed for module
    /// references
    #[allow(dead_code)]
    fn from_uri(uri: &Url) -> Self {
        Self::extract(uri).into()
    }

    fn extract(uri: &Url) -> Option<String> {
        let name = uri
            .path_segments()?
            .next_back()?
            .split('.')
            .next()?
            .to_string();
        Some(name)
    }
}

/// a reference to a specific code file.
/// implemented as an index into the `context.code_files`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileRef(pub(in crate::compiler) usize);

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize)]
pub struct ProjectConfig {
    //todo
    pub tracked_files: Vec<Url>,
}

#[derive(Debug, Error)]
#[error("there was an error with the uri {uri}: {message}")]
pub struct UriError {
    uri: String,
    message: String,
}
impl UriError {
    pub fn new(uri: Url, message: String) -> Self {
        Self {
            uri: uri.to_string(),
            message,
        }
    }

    pub fn name_fail(uri: &Url) -> Self {
        Self {
            uri: uri.to_string(),
            message: "Cannot extract file name from URI".into(),
        }
    }

    pub fn to_path(uri: &Url) -> Self {
        Self {
            uri: uri.to_string(),
            message: "Cannot convert URI to file path".into(),
        }
    }

    pub fn from_path(path: &Path) -> Self {
        Self {
            uri: path.to_string_lossy().to_string(),
            message: "Cannot convert file path to URI".into(),
        }
    }
}

impl Display for ModuleInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
