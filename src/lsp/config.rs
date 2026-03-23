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
        use tower_lsp::lsp_types::MessageType;

        self.log(
            MessageType::LOG,
            format!(
                "applying config with {} tracked files",
                config.tracked_files.len()
            ),
        )
        .await;

        let mut new_files = Map::new();
        for f in &config.tracked_files {
            match std::fs::read_to_string(
                f.to_file_path()
                    .map_err(|_| anyhow!("uri {f} is not a path"))?,
            ) {
                Ok(content) => {
                    new_files.insert(f.clone(), content);
                    self.log(MessageType::LOG, format!("loaded config file: {}", f))
                        .await;
                }
                Err(e) => {
                    self.log(
                        MessageType::WARNING,
                        format!("failed to load config file {}: {}", f, e),
                    )
                    .await;
                    return Err(e.into());
                }
            }
        }

        self.log(
            MessageType::LOG,
            format!("registering {} files from config", new_files.len()),
        )
        .await;

        for (uri, content) in new_files {
            self.register_file(uri, content).await;
        }

        self.log(MessageType::LOG, "config applied successfully".to_string())
            .await;
        Ok(())
    }
}
