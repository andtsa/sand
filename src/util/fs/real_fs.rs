use std::fs;
use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::debug;
use tracing::trace;

use crate::ctx;
use crate::internal_bug;
use crate::util::fs::FileOperations;
use crate::util::fs::error::Context;
use crate::util::fs::error::FsError;

pub struct FileSystem {
    pub dry_run: bool,
}

impl FileOperations for FileSystem {
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, FsError> {
        fs::read(path).with_context(ctx!(
            "Could not read the file {path:?}", ;
            "Ensure that the file exists and you have permissions to access it",
        ))
    }

    fn read_utf8(&self, path: &Path) -> Result<String, FsError> {
        String::from_utf8(self.read_bytes(path)?).with_context(ctx!(
            "{path:?} is not valid UTF-8", ;
            "The file doesn't seem to be human readable?",
        ))
    }

    fn read_file(&self, path: &Path) -> Result<(String, String), FsError> {
        let pb = self.canonicalize(path)?;
        if !pb.is_file() {
            return Err(FsError::new("File not found")).with_context(ctx!(
                "Path {} does not point to a file", path.display();
                "Ensure that the file exists and you have permissions to access it",
            ));
        }
        let name = pb
            .file_stem()
            .unwrap_or_else(|| internal_bug!("file name not found after checking .is_file"))
            .to_string_lossy()
            .to_string();
        let contents = self.read_utf8(path)?;
        Ok((name, contents))
    }

    fn try_read_toml<T: DeserializeOwned>(&self, path: &Path) -> Result<T, FsError> {
        toml::from_str::<T>(&self.read_utf8(path)?).with_context(ctx!(
            "Could not deserialize toml file {path:?}", ;
            "Ensure that the file is valid toml",
        ))
    }

    fn try_write_toml<T: Serialize>(&self, path: &Path, data: &T) -> Result<(), FsError> {
        self.write_utf8_truncate(
            path,
            &toml::to_string::<T>(data).with_context(ctx!(
                "Could not serialize toml file {path:?}", ;
                "Ensure that the struct is valid toml",
            ))?,
        )
    }

    fn write_bytes_truncate(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        if self.dry_run {
            debug!("Would have written to {path:?} (dry)");
            return Ok(());
        }

        fs::write(self.truncate_and_canonicalize(path)?, bytes).with_context(ctx!(
          "Could not write to the file {path:?}", ;
          "Ensure that you have permissions to write it",
        ))?;

        Ok(())
    }

    fn write_utf8_truncate(&self, path: &Path, data: &str) -> Result<(), FsError> {
        self.write_bytes_truncate(path, data.as_bytes())
    }

    fn truncate_and_canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        if self.dry_run {
            if let Some(parent) = path.parent() {
                trace!("Would have created {parent:?} (dry)");
            }

            trace!("Would have created {path:?} (dry)");
            return Ok(path.to_path_buf());
        }

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                debug!("Creating directories for {parent:?}");
            }

            fs::create_dir_all(parent).with_context(ctx!(
              "Could not create parent directories for {parent:?}", ;
              "Ensure that you have sufficient permissions",
            ))?;
        }

        debug!("Creating a file at {path:?}");
        fs::File::create(path).with_context(ctx!(
           "Could not create {path:?}", ;
           "Ensure that you have sufficient permissions",
        ))?;

        self.canonicalize(path)
    }

    fn truncate_and_canonicalize_folder(&self, path: &Path) -> Result<PathBuf, FsError> {
        if self.dry_run {
            debug!("Would have created {path:?} (dry)");
            return Ok(path.to_path_buf());
        }

        debug!("Creating directories for {path:?}");
        fs::create_dir_all(path).with_context(ctx!(
           "Could not create {path:?}", ;
           "Ensure that you have sufficient permissions",
        ))?;

        self.canonicalize(path)
    }

    fn set_permissions(&self, path: &Path, perms: u32) -> Result<(), FsError> {
        if self.dry_run {
            debug!("Would have made {path:?} executable (dry)");
            return Ok(());
        }

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, Permissions::from_mode(perms)).with_context(ctx!(
                "Could not make {path:?} executable", ;
                "Ensure that you have sufficient permissions",
            ))
        }
        #[cfg(not(unix))]
        {
            Ok(())
        }
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        PathBuf::from(
            shellexpand::tilde(
                path.to_str()
                    .ok_or(FsError::new(&format!("{path:?} is not valid utf8")))?,
            )
            .to_string(),
        )
        .canonicalize()
        .with_context(ctx!(
            "Could not canonicalize {path:?}", ;
            "Ensure that you provided a valid path",
        ))
    }

    fn delete_file(&self, path: &Path) -> Result<(), FsError> {
        if self.dry_run {
            debug!("Would have deleted {path:?} (dry)");
            return Ok(());
        }

        fs::remove_file(path).with_context(ctx!(
            "Could not delete {path:?}", ;
            "Ensure that you have sufficient permissions",
        ))
    }
}
