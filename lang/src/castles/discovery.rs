//! file discovery utilities

use std::path::Path;
use std::path::PathBuf;

use url::Url;

use crate::compiler::structure::Map;
use crate::compiler::structure::UriError;
use crate::util::fs::error::FsError;

pub fn discover_files(root: PathBuf) -> Result<Vec<PathBuf>, FsError> {
    let mut files = Vec::new();
    walk_directory_sync(&root, &mut files)?;
    Ok(files)
}

/// Recursively finds all `sand.toml` config files under `root`.
pub fn discover_configs(root: PathBuf) -> Result<Vec<PathBuf>, FsError> {
    let mut configs = Vec::new();
    walk_configs_sync(&root, &mut configs)?;
    Ok(configs)
}

pub fn read_discovered_files(files: Vec<PathBuf>) -> Result<Map<Url, String>, FsError> {
    let mut map = Map::new();
    for file in files {
        let url = Url::from_file_path(&file).map_err(|_| UriError::from_path(&file))?;
        map.insert(url, std::fs::read_to_string(&file)?);
    }
    Ok(map)
}

fn walk_configs_sync(dir: &Path, configs: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let skip = path
                .file_name()
                .map(|n| {
                    matches!(
                        n.to_string_lossy().as_ref(),
                        "node_modules" | ".git" | "target"
                    )
                })
                .unwrap_or(false);
            if !skip {
                walk_configs_sync(&path, configs)?;
            }
        } else if path.file_name().is_some_and(|n| n == "sand.toml") {
            configs.push(path);
        }
    }
    Ok(())
}

fn walk_directory_sync(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let skip = path
                .file_name()
                .map(|n| {
                    matches!(
                        n.to_string_lossy().as_ref(),
                        "node_modules" | ".git" | "target"
                    )
                })
                .unwrap_or(false);
            if !skip {
                walk_directory_sync(&path, files)?;
            }
        } else if path.extension().is_some_and(|e| e == "sand") {
            files.push(path);
        }
    }
    Ok(())
}

#[allow(dead_code)]
async fn walk_directory(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.is_dir() {
            // Skip common directories we don't want to scan
            if let Some(name) = path.file_name() {
                let name = name.to_string_lossy();
                if name == "node_modules" || name == ".git" || name == "target" {
                    continue;
                }
            }

            Box::pin(walk_directory(&path, files)).await?;
        } else if let Some(ext) = path.extension() {
            // Match files by extension
            if ext == "sand" {
                files.push(path);
            }
        }
    }

    Ok(())
}
