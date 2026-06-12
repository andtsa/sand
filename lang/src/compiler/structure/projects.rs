//! types relating to projects and their structure

use std::cmp::Ordering;
use std::fmt::Display;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use thiserror::Error;
use url::Url;

use crate::util::fs::FileOperations;
use crate::util::fs::expand_to_files;

/// A `Copy` handle to an arena-allocated [`CodeModule`]. Equality/hashing by
/// pointer identity, ordering by the monotonic registration `id`.
#[derive(Copy, Clone)]
pub struct ModuleRef<'tcx>(pub(in crate::compiler) &'tcx CodeModule);

impl PartialEq for ModuleRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}
impl Eq for ModuleRef<'_> {}
impl Hash for ModuleRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as *const CodeModule).hash(state);
    }
}
impl PartialOrd for ModuleRef<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ModuleRef<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.id.cmp(&other.0.id)
    }
}
impl std::fmt::Debug for ModuleRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ModuleRef({}, {})", self.0.id, self.0.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CodeModule {
    pub(in crate::compiler) name: String,
    pub(in crate::compiler) from_file: FileRef,
    pub(in crate::compiler) id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleInfo<'tcx> {
    pub name: String,
    pub index: ModuleRef<'tcx>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodeFile {
    /// file can be file:///path/to/file or http://remote/file or ssh://remote/file
    pub(in crate::compiler) uri: url::Url,
    pub(in crate::compiler) name: FileName,
    pub(in crate::compiler) index: FileRef,
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
    pub project: ProjectSection,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize)]
pub struct ProjectSection {
    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Error)]
#[error("there was an error with the uri {uri}: {message}")]
pub struct UriError {
    uri: String,
    message: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    GlobPatternError(#[from] glob::PatternError),
    #[error(transparent)]
    GlobError(#[from] glob::GlobError),
    #[error(transparent)]
    UriError(#[from] UriError),
    #[error("could not read path {0:?}")]
    PathError(String),
}

pub fn as_url<P: AsRef<Path>>(path: P) -> Result<Url, UriError> {
    Url::from_file_path(path.as_ref()).map_err(|_| UriError::from_path(path.as_ref()))
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

impl Display for ModuleInfo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl ProjectConfig {
    /// try to parse each source as a [`Url`].
    ///
    /// if that fails, treat it as a glob pattern / path and expand it via
    /// [`expand_to_files`], recursing into any matched directories.
    ///
    /// relative patterns (anything that isn't absolute or `~`-relative) are
    /// resolved against `root` — the directory containing the project's
    /// `sand.toml` — rather than the process's current working directory, so
    /// that `sand-cli --config some/dir/sand.toml` works regardless of where
    /// it's invoked from.
    ///
    /// convert all resulting paths to [`Url`]s, and return the combined list.
    pub fn urls(&self, fs: &impl FileOperations, root: &Path) -> Result<Vec<Url>, ConfigError> {
        let mut urls = Vec::new();

        for file in &self.project.sources {
            // a remote/absolute URL (http://, ssh://, file://, ...).
            // reject single-letter "schemes" like `C:` (Windows drive letters)
            // which `Url::from_str` would otherwise misparse.
            if let Ok(url) = Url::from_str(file)
                && url.scheme().len() > 1
            {
                urls.push(url);
                continue;
            }

            // resolve relative (non-`~`) patterns against the project root
            let resolved: PathBuf = if file.starts_with('~') || Path::new(file).is_absolute() {
                PathBuf::from(file)
            } else {
                root.join(file)
            };
            let pattern = resolved.to_string_lossy().to_string();

            let paths = expand_to_files(fs, &pattern)
                .map_err(|_| ConfigError::PathError(file.to_string()))?;

            if paths.is_empty() {
                return Err(ConfigError::PathError(file.to_string()));
            }

            for path in paths {
                urls.push(as_url(&path)?);
            }
        }

        Ok(urls)
    }
}
