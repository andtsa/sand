//! wrapper for interacting with some file system
//!
//! The code in this module is borrowed from [the `gourd` project](https://github.com/ConSol-Lab/gourd),
//! with this specific code originally authored by Mikołaj Gazeel and Lukáš
//! Chladek

pub mod error;
pub mod real_fs;

use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::util::fs::error::FsError;

pub trait FileOperations {
    /// Read a file into raw bytes.
    fn read_bytes<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>, FsError>;

    /// Read a file into a utf8 string.
    fn read_utf8<P: AsRef<Path>>(&self, path: P) -> Result<String, FsError>;

    /// Try to deserialize a toml file into a struture `T`.
    fn try_read_toml<T: DeserializeOwned, P: AsRef<Path>>(&self, path: P) -> Result<T, FsError>;

    /// Try to serialize a struct `T` into a toml file.
    fn try_write_toml<T: Serialize, P: AsRef<Path>>(
        &self,
        path: P,
        data: &T,
    ) -> Result<(), FsError>;

    /// Write all bytes to a file.
    fn write_bytes_truncate<P: AsRef<Path>>(&self, path: P, bytes: &[u8]) -> Result<(), FsError>;

    /// Write a [String] to a file.
    fn write_utf8_truncate<P: AsRef<Path>>(&self, path: P, data: &str) -> Result<(), FsError>;

    /// Truncates the file and then runs [FileOperations::canonicalize].
    fn truncate_and_canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, FsError>;

    /// Truncates the folder and then runs [FileOperations::canonicalize].
    fn truncate_and_canonicalize_folder<P: AsRef<Path>>(&self, path: P)
    -> Result<PathBuf, FsError>;

    /// Make a file possible to execute.
    fn set_permissions<P: AsRef<Path>>(&self, path: P, perms: u32) -> Result<(), FsError>;

    /// Given a path try to canonicalize it.
    ///
    /// This will fail for files that do not exist.
    fn canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, FsError>;

    /// Delete a file.
    fn delete_file<P: AsRef<Path>>(&self, path: P) -> Result<(), FsError>;

    /// Check whether `path` points to a regular file.
    fn is_file<P: AsRef<Path>>(&self, path: P) -> bool;

    /// Check whether `path` points to a directory.
    fn is_dir<P: AsRef<Path>>(&self, path: P) -> bool;

    /// List the immediate entries of a directory.
    fn read_dir<P: AsRef<Path>>(&self, path: P) -> Result<Vec<PathBuf>, FsError>;

    /// Expand a glob pattern (after `~` expansion) into the list of matching
    /// paths. Patterns containing no glob metacharacters simply match
    /// themselves, if they exist.
    fn glob_expand(&self, pattern: &str) -> Result<Vec<PathBuf>, FsError>;
}

/// recursively collect all files under `dir`, depth-first
pub fn collect_files_recursive(
    fs: &impl FileOperations,
    dir: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), FsError> {
    for entry in fs.read_dir(dir)? {
        if fs.is_file(&entry) {
            // canonicalize so callers always get absolute paths (required
            // e.g. to build `file://` URLs via `Url::from_file_path`)
            out.push(fs.canonicalize(&entry)?);
        } else if fs.is_dir(&entry) {
            collect_files_recursive(fs, &entry, out)?;
        }
    }
    Ok(())
}

/// expand a glob pattern (or literal/relative path, or `~`-relative path) to
/// the list of files it refers to, recursing into any matched directories.
///
/// returns an empty vec if nothing matches.
pub fn expand_to_files(fs: &impl FileOperations, pattern: &str) -> Result<Vec<PathBuf>, FsError> {
    let mut out = Vec::new();

    let matches = fs.glob_expand(pattern)?;
    if !matches.is_empty() {
        for path in matches {
            if fs.is_file(&path) {
                // `glob` preserves the (possibly relative) form of the
                // pattern; canonicalize so callers always get absolute paths
                out.push(fs.canonicalize(&path)?);
            } else if fs.is_dir(&path) {
                collect_files_recursive(fs, &path, &mut out)?;
            }
        }
        return Ok(out);
    }

    // fall back: treat as a literal path (handles non-glob relative/`~`
    // paths that `glob` may not resolve, e.g. paths that don't exist yet
    // relative to cwd but do once canonicalized)
    if let Ok(canonical) = fs.canonicalize(pattern) {
        if fs.is_file(&canonical) {
            out.push(canonical);
        } else if fs.is_dir(&canonical) {
            collect_files_recursive(fs, &canonical, &mut out)?;
        }
    }

    Ok(out)
}
