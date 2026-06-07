//! Initialising a project

use std::path::Path;
use std::path::PathBuf;

use url::Url;

use crate::castles::project::Project;
use crate::compiler::structure::ConfigError;
use crate::compiler::structure::ProjectConfig;
use crate::compiler::structure::UriError;
use crate::internal_bug;
use crate::util::fs::FileOperations;
use crate::util::fs::error::FsError;

pub struct SetupWarning {
    pub kind: SetupWarningKind,
    pub message: String,
    pub url: Url,
}

#[derive(Debug, Clone)]
pub enum SetupWarningKind {
    MissingTrackedFile { uri: Url },
    UnreadableFile { uri: Url, reason: String },
}

pub struct ProjectCreationResult {
    pub project: Project,
    pub warnings: Vec<SetupWarning>,
}

impl ProjectCreationResult {
    pub fn ok(self) -> Project {
        self.project
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FatalProjectCreationError {
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("project config file not found at {path:?}")]
    MissingConfig { path: PathBuf },
    #[error(transparent)]
    Config(#[from] ConfigError),
}

impl Project {
    pub fn from_rootdir<P: AsRef<Path>>(
        config: P,
    ) -> Result<ProjectCreationResult, FatalProjectCreationError> {
        let config_path = config.as_ref().join("sand.toml");
        Self::from_config(config_path)
    }

    pub fn from_config<P: AsRef<Path>>(
        config: P,
    ) -> Result<ProjectCreationResult, FatalProjectCreationError> {
        let mut project = Project::empty();
        let mut warnings = Vec::new();
        let config_path = config.as_ref().to_path_buf();
        tracing::debug!("loading project from config: {config_path:?}");

        let config = ProjectConfig::load(&project.fs, &config_path)?.ok_or_else(|| {
            FatalProjectCreationError::MissingConfig {
                path: config_path.clone(),
            }
        })?;
        let project_root = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        project.config_src = Some(config_path);

        let urls = config.urls(&project.fs, &project_root)?;
        for url in &urls {
            match url
                .to_file_path()
                .map_err(|_| UriError::to_path(url))
                .map(|p| {
                    project
                        .fs
                        .read_utf8(p)
                        .map(|content| project.insert_file(url.clone(), content))
                }) /* UriToPath(ParseUtf8(UriToName(Fr))) */ {

                Ok(Ok(Ok(fr))) => {
                    tracing::debug!("loaded file {url} as {fr:?}");
                }
                Ok(Ok(Err(e))) | Err(e) => {
                    warnings.push(SetupWarning {
                        kind: SetupWarningKind::UnreadableFile {
                            uri: url.clone(),
                            reason: e.to_string(),
                        },
                        message: format!("Invalid file url `{}`: {}", url, e),
                        url: url.clone(),
                    });
                }
                Ok(Err(e)) => {
                    warnings.push(SetupWarning {
                        kind: SetupWarningKind::UnreadableFile { uri: url.clone(), reason: e.to_string() },
                        message: format!("Failed to read tracked file {}: {}", url, e),
                        url: url.clone(),
                    });
                }
            }
        }

        Ok(ProjectCreationResult { project, warnings })
    }

    pub fn from_paths(
        paths: &[PathBuf],
    ) -> Result<ProjectCreationResult, FatalProjectCreationError> {
        let span = tracing::debug_span!("loading project from paths");
        let _g1 = span.enter();
        let mut project = Project::empty();
        let mut warnings = Vec::new();
        for path in paths {
            let pb = project.fs.canonicalize(path)?;
            let Ok(uri) = Url::from_file_path(&pb) else {
                internal_bug!("url from path failed after canonicalize");
            };
            tracing::debug!("loading file: {path:?}");
            let content = project.fs.read_utf8(pb)?;
            if let Err(e) = project.insert_file(uri.clone(), content) {
                warnings.push(SetupWarning {
                    kind: SetupWarningKind::UnreadableFile {
                        uri: uri.clone(),
                        reason: e.to_string(),
                    },
                    message: format!("Failed to read file {}: {}", path.display(), e),
                    url: uri,
                });
            }
        }

        Ok(ProjectCreationResult { project, warnings })
    }
}
