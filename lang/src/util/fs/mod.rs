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
}
