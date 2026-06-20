//! file discovery utilities

use std::path::Path;
use std::path::PathBuf;

use url::Url;

use crate::compiler::structure::Map;
use crate::compiler::structure::UriError;
use crate::util::fs::error::FsError;

pub fn discover_files(root: PathBuf) -> Result<Vec<PathBuf>, FsError> {
    let mut files = Vec::new();
    walk(
        &root,
        &|p| p.extension().is_some_and(|e| e == "sand"),
        &mut files,
    )?;
    Ok(files)
}

/// Recursively finds all `sand.toml` config files under `root`.
pub fn discover_configs(root: PathBuf) -> Result<Vec<PathBuf>, FsError> {
    let mut configs = Vec::new();
    walk(
        &root,
        &|p| p.file_name().is_some_and(|n| n == "sand.toml"),
        &mut configs,
    )?;
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

/// Directories never worth descending into during discovery.
const SKIP_DIRS: &[&str] = &["node_modules", ".git", "target"];

/// Recursively collect every file under `dir` for which `keep` returns true,
/// skipping [`SKIP_DIRS`].
fn walk(dir: &Path, keep: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let skip = path
                .file_name()
                .is_some_and(|n| SKIP_DIRS.contains(&n.to_string_lossy().as_ref()));
            if !skip {
                walk(&path, keep, out)?;
            }
        } else if keep(&path) {
            out.push(path);
        }
    }
    Ok(())
}
