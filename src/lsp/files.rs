//! file management

use std::path::Path;
use std::path::PathBuf;

use tower_lsp::lsp_types::Diagnostic;
use tower_lsp::lsp_types::Url;

use crate::compiler::structure::Map;
use crate::lsp::Backend;

impl Backend<'_> {
    pub async fn register_file(&self, uri: Url, content: String) {
        self.project_files
            .write()
            .await
            .insert(uri.clone(), content);
        let fr = match self.context.write().await.register_file(uri.clone()) {
            Ok(fr) => fr,
            Err(e) => {
                self.project_files.write().await.remove(&uri);
                self.client
                    .publish_diagnostics(
                        uri,
                        vec![Diagnostic {
                            range: Default::default(),
                            message: e.to_string(),
                            severity: None,
                            ..Default::default()
                        }],
                        None,
                    )
                    .await;
                return;
            }
        };
        self.files.write().await.insert(uri, fr);
    }
}

pub async fn discover_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_directory(root, &mut files).await?;
    Ok(files)
}

pub async fn read_discovered_files(files: Vec<PathBuf>) -> std::io::Result<Map<Url, String>> {
    let mut map = Map::new();
    for file in files {
        let url = Url::from_file_path(&file).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid file path")
        })?;
        map.insert(url, std::fs::read_to_string(&file)?);
    }
    Ok(map)
}

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
