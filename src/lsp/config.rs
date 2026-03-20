//! project configuration files

use std::path::Path;

use anyhow::anyhow;

use crate::compiler::structure::Map;
use crate::compiler::structure::ProjectConfig;
use crate::lsp::Backend;

pub async fn load_config(root_path: &Path) -> anyhow::Result<Option<ProjectConfig>> {
    // look for `sand.toml`,
    // if found, parse it and return the config
    let config_path = root_path.join("sand.toml");
    if !config_path.exists() {
        return Ok(None);
    }
    let config = std::fs::read_to_string(&config_path)?;
    Ok(Some(toml::from_str(&config)?))
}

impl Backend<'_> {
    pub async fn apply_config(&self, config: &ProjectConfig) -> anyhow::Result<()> {
        let mut new_files = Map::new();
        for f in &config.tracked_files {
            new_files.insert(
                f.clone(),
                std::fs::read_to_string(
                    f.to_file_path()
                        .map_err(|_| anyhow!("uri {f} is not a path"))?,
                )?,
            );
        }
        for (uri, content) in new_files {
            self.register_file(uri, content).await;
        }
        Ok(())
    }
}
