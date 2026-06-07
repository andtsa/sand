use std::fs;
use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::debug;
use tracing::trace;

use crate::ctx;
use crate::util::fs::FileOperations;
use crate::util::fs::error::Context;
use crate::util::fs::error::FsError;

pub struct FileSystem {
    pub dry_run: bool,
}

impl FileOperations for FileSystem {
    fn read_bytes<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>, FsError> {
        fs::read(&path).with_context(ctx!(
            "Could not read the file {:?}", path.as_ref();
            "Ensure that the file exists and you have permissions to access it",
        ))
    }

    fn read_utf8<P: AsRef<Path>>(&self, path: P) -> Result<String, FsError> {
        String::from_utf8(self.read_bytes(&path)?).with_context(ctx!(
            "{:?} is not valid UTF-8", path.as_ref();
            "The file doesn't seem to be human readable?",
        ))
    }

    fn try_read_toml<T: DeserializeOwned, P: AsRef<Path>>(&self, path: P) -> Result<T, FsError> {
        toml::from_str::<T>(&self.read_utf8(&path)?).with_context(ctx!(
            "Could not deserialize toml file {:?}", path.as_ref();
            "Ensure that the file is valid toml",
        ))
    }

    fn try_write_toml<T: Serialize, P: AsRef<Path>>(
        &self,
        path: P,
        data: &T,
    ) -> Result<(), FsError> {
        self.write_utf8_truncate(
            &path,
            &toml::to_string::<T>(data).with_context(ctx!(
                "Could not serialize toml file {:?}", path.as_ref();
                "Ensure that the struct is valid toml",
            ))?,
        )
    }

    fn write_bytes_truncate<P: AsRef<Path>>(&self, path: P, bytes: &[u8]) -> Result<(), FsError> {
        if self.dry_run {
            debug!("Would have written to {:?} (dry)", path.as_ref());
            return Ok(());
        }

        fs::write(self.truncate_and_canonicalize(&path)?, bytes).with_context(ctx!(
          "Could not write to the file {:?}", path.as_ref();
          "Ensure that you have permissions to write it",
        ))?;

        Ok(())
    }

    fn write_utf8_truncate<P: AsRef<Path>>(&self, path: P, data: &str) -> Result<(), FsError> {
        self.write_bytes_truncate(path, data.as_bytes())
    }

    fn truncate_and_canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, FsError> {
        if self.dry_run {
            if let Some(parent) = path.as_ref().parent() {
                trace!("Would have created {parent:?} (dry)");
            }

            trace!("Would have created {:?} (dry)", path.as_ref());
            return Ok(path.as_ref().to_path_buf());
        }

        if let Some(parent) = path.as_ref().parent() {
            if !parent.exists() {
                debug!("Creating directories for {parent:?}");
            }

            fs::create_dir_all(parent).with_context(ctx!(
              "Could not create parent directories for {parent:?}", ;
              "Ensure that you have sufficient permissions",
            ))?;
        }

        debug!("Creating a file at {:?}", path.as_ref());
        fs::File::create(&path).with_context(ctx!(
           "Could not create {:?}", path.as_ref();
           "Ensure that you have sufficient permissions",
        ))?;

        self.canonicalize(path)
    }

    fn truncate_and_canonicalize_folder<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<PathBuf, FsError> {
        if self.dry_run {
            debug!("Would have created {:?} (dry)", path.as_ref());
            return Ok(path.as_ref().to_path_buf());
        }

        debug!("Creating directories for {:?}", path.as_ref());
        fs::create_dir_all(&path).with_context(ctx!(
           "Could not create {:?}", path.as_ref();
           "Ensure that you have sufficient permissions",
        ))?;

        self.canonicalize(path)
    }

    fn set_permissions<P: AsRef<Path>>(&self, path: P, perms: u32) -> Result<(), FsError> {
        if self.dry_run {
            debug!("Would have made {:?} executable (dry)", path.as_ref());
            return Ok(());
        }

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, Permissions::from_mode(perms)).with_context(ctx!(
                "Could not make {:?} executable", path.as_ref();
                "Ensure that you have sufficient permissions",
            ))
        }
        #[cfg(not(unix))]
        {
            warn!(
                "file permissions cannot be set in non-unix environments. this call to set_permissions has no effect."
            );
            Ok(())
        }
    }

    fn canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, FsError> {
        PathBuf::from(
            shellexpand::tilde(path.as_ref().to_str().ok_or(FsError::new(&format!(
                "{:?} is not valid utf8",
                path.as_ref()
            )))?)
            .to_string(),
        )
        .canonicalize()
        .with_context(ctx!(
            "Could not canonicalize {:?}", path.as_ref();
            "Ensure that you provided a valid path",
        ))
    }

    fn delete_file<P: AsRef<Path>>(&self, path: P) -> Result<(), FsError> {
        if self.dry_run {
            debug!("Would have deleted {:?} (dry)", path.as_ref());
            return Ok(());
        }

        fs::remove_file(&path).with_context(ctx!(
            "Could not delete {:?}", path.as_ref();
            "Ensure that you have sufficient permissions",
        ))
    }

    fn is_file<P: AsRef<Path>>(&self, path: P) -> bool {
        path.as_ref().is_file()
    }

    fn is_dir<P: AsRef<Path>>(&self, path: P) -> bool {
        path.as_ref().is_dir()
    }

    fn read_dir<P: AsRef<Path>>(&self, path: P) -> Result<Vec<PathBuf>, FsError> {
        fs::read_dir(&path)
            .with_context(ctx!(
                "Could not read directory {:?}", path.as_ref();
                "Ensure that the directory exists and you have permissions to access it",
            ))?
            .map(|entry| {
                entry.map(|e| e.path()).with_context(ctx!(
                    "Could not read an entry of directory {:?}", path.as_ref();
                    "Ensure that you have permissions to access it",
                ))
            })
            .collect()
    }

    fn glob_expand(&self, pattern: &str) -> Result<Vec<PathBuf>, FsError> {
        let expanded = shellexpand::tilde(pattern).to_string();
        glob::glob(&expanded)
            .with_context(ctx!(
                "{:?} is not a valid glob pattern", expanded;
                "Ensure that the pattern is well-formed",
            ))?
            .map(|res| {
                res.with_context(ctx!(
                    "Could not read a path matched by glob pattern {:?}", expanded;
                    "Ensure that you have permissions to access it",
                ))
            })
            .collect()
    }
}
