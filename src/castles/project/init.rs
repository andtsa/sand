//! Initialising a project

use std::path::Path;
use std::path::PathBuf;

use url::Url;

use crate::castles::project::Project;
use crate::compiler::structure::ProjectConfig;
use crate::compiler::structure::UriError;
use crate::internal_bug;
use crate::util::fs::FileOperations;
use crate::util::fs::error::FsError;

pub struct SetupWarning {
    pub kind: SetupWarningKind,
    pub message: String,
}

pub enum SetupWarningKind {
    MissingTrackedFile { uri: Url },
    UnreadableFile { uri: Url, reason: String },
}

pub struct ProjectCreationResult {
    pub project: Project,
    pub warnings: Vec<SetupWarning>,
}

#[derive(Debug, thiserror::Error)]
pub enum FatalProjectCreationError {
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("project config file not found at {path:?}")]
    MissingConfig { path: PathBuf },
}

impl Project {
    pub fn from_config<P: AsRef<Path>>(
        root: P,
    ) -> Result<ProjectCreationResult, FatalProjectCreationError> {
        let mut project = Project::empty();
        let mut warnings = Vec::new();
        // look for `sand.toml` in the root directory,
        let config_path = root.as_ref().join("sand.toml");

        let config = ProjectConfig::load(&project.fs, &config_path)?.ok_or_else(|| {
            FatalProjectCreationError::MissingConfig {
                path: config_path.clone(),
            }
        })?;
        project.config_src = Some(config_path);

        for uri in &config.tracked_files {
            match uri
                .to_file_path()
                .map_err(|_| UriError::to_path(uri))
                .map(|p| {
                    project
                        .fs
                        .read_utf8(p)
                        .map(|content| project.insert_file(uri.clone(), content))
                }) /* UriToPath(ParseUtf8(UriToName(Fr))) */ {

                Ok(Ok(Ok(fr))) => {
                    tracing::debug!("loaded file {uri} as {fr:?}");
                }
                Ok(Ok(Err(e))) | Err(e) => {
                    warnings.push(SetupWarning {
                        kind: SetupWarningKind::UnreadableFile {
                            uri: uri.clone(),
                            reason: e.to_string(),
                        },
                        message: format!("Invalid file url `{}`: {}", uri, e),
                    });
                }
                Ok(Err(e)) => {
                    warnings.push(SetupWarning {
                        kind: SetupWarningKind::UnreadableFile { uri: uri.clone(), reason: e.to_string() },
                        message: format!("Failed to read tracked file {}: {}", uri, e),
                    });
                }
            }
        }

        Ok(ProjectCreationResult { project, warnings })
    }

    pub fn from_paths(
        paths: &[PathBuf],
    ) -> Result<ProjectCreationResult, FatalProjectCreationError> {
        let mut project = Project::empty();
        let mut warnings = Vec::new();
        for path in paths {
            let pb = project.fs.canonicalize(path)?;
            let Ok(uri) = Url::from_file_path(&pb) else {
                internal_bug!("url from path failed after canonicalize");
            };
            let content = project.fs.read_utf8(pb)?;
            if let Err(e) = project.insert_file(uri.clone(), content) {
                warnings.push(SetupWarning {
                    kind: SetupWarningKind::UnreadableFile {
                        uri,
                        reason: e.to_string(),
                    },
                    message: format!("Failed to read file {}: {}", path.display(), e),
                });
            }
        }

        Ok(ProjectCreationResult { project, warnings })
    }
}
