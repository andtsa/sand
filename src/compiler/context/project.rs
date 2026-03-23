//! # the project context
//! the project context holds the project-wide configuration and other
//! data that persists across compilation runs

use bimap::BiBTreeMap;
use tower_lsp::lsp_types::Url;

use crate::compiler::structure::CodeFile;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::ProjectConfig;
use crate::compiler::structure::UriError;
use crate::compiler::structure::uri_name;

/// the project context
pub struct ProjectCtx {
    // project and files
    pub project_config: ProjectConfig,
    code_files: Vec<CodeFile>,
    pub files: BiBTreeMap<Url, FileRef>,

    default_file: Option<FileRef>,
}

impl ProjectCtx {
    pub fn initial() -> Self {
        Self {
            project_config: ProjectConfig::default(),
            code_files: vec![],
            files: BiBTreeMap::new(),
            default_file: None,
        }
    }

    // ============================ Files ==============================
    pub fn register_file(&mut self, uri: Url) -> Result<FileRef, UriError> {
        if let Some(fr) = self.files.get_by_left(&uri) {
            // file already registered, just return the pointer
            Ok(*fr)
        } else {
            let idx = self.code_files.len();
            let fr = FileRef(idx);
            let name = uri_name(&uri)?;
            let cf = CodeFile {
                uri: uri.clone(),
                name,
                index: fr,
                default_module: None,
            };
            self.code_files.push(cf);
            self.files.insert(uri, fr);

            Ok(fr)
        }
    }

    pub fn register_dummy_file(&mut self) -> FileRef {
        let idx = self.code_files.len();
        let fr = FileRef(idx);
        let cf = CodeFile {
            uri: Url::parse("dummy:///tmp/internal/sand_dummy_file.sand").unwrap(),
            name: "sand_dummy_file".to_string(),
            index: fr,
            default_module: None,
        };
        self.code_files.push(cf);
        fr
    }

    pub fn default_file(&mut self, uri: Url) -> Result<FileRef, UriError> {
        if let Some(fr) = self.default_file {
            Ok(fr)
        } else {
            let fr = self.register_file(uri)?;
            self.default_file = Some(fr);
            Ok(fr)
        }
    }

    pub fn url_of_file(&self, file: FileRef) -> Url {
        self.code_files[file.0].uri.clone()
    }
}
