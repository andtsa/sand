//! sand.toml etc
use std::path::Path;

use crate::compiler::structure::ProjectConfig;
use crate::util::fs::FileOperations;
use crate::util::fs::error::FsError;
use crate::util::fs::real_fs::FileSystem;

impl ProjectConfig {
    pub fn load(fs: &FileSystem, root_path: &Path) -> Result<Option<Self>, FsError> {
        // look for `sand.toml`,
        // if found, parse it and return the config
        let config_path = root_path.join("sand.toml");
        if !config_path.exists() {
            return Ok(None);
        }
        let config = fs.read_utf8(&config_path)?;
        Ok(Some(fs.try_read_toml(&config)?))
    }
}
