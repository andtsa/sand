//! # the project context
//! the project context holds the project-wide configuration and other
//! data that persists across compilation runs

use bimap::BiBTreeMap;
use url::Url;

use crate::compiler::structure::CodeFile;
use crate::compiler::structure::FileName;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::ProjectConfig;
use crate::compiler::structure::UriError;

/// the project context
pub struct ProjectCtx {
    // project and files
    pub project_config: ProjectConfig,
    code_files: Vec<CodeFile>,
    pub files: BiBTreeMap<Url, FileRef>,

    default_file: Option<FileRef>,

    dummy_file: Option<FileRef>,
}

impl ProjectCtx {
    pub fn initial() -> Self {
        Self {
            project_config: ProjectConfig::default(),
            code_files: vec![],
            files: BiBTreeMap::new(),
            default_file: None,
            dummy_file: None,
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
            let cf = CodeFile {
                uri: uri.clone(),
                name: FileName::try_from_uri(&uri)?,
                index: fr,
            };
            self.code_files.push(cf);
            self.files.insert(uri, fr);

            Ok(fr)
        }
    }

    pub fn code_file(&self, fr: FileRef) -> &CodeFile {
        &self.code_files[fr.0]
    }

    pub fn code_file_mut(&mut self, fr: FileRef) -> &mut CodeFile {
        &mut self.code_files[fr.0]
    }

    /// registers a file without having to provide a valid URL.
    ///
    /// **Panics:** this function will panic if `name` causes
    /// `"virtual:///tmp/internal/{name}.sand"` being an invalid [`Url`]
    pub fn register_virtual_file(&mut self, name: &str) -> FileRef {
        let idx = self.code_files.len();
        let fr = FileRef(idx);
        let cf = CodeFile {
            uri: Url::parse(&format!("virtual:///tmp/internal/{name}.sand")).unwrap(),
            name: FileName::virt(name),
            index: fr,
        };
        self.code_files.push(cf);
        self.dummy_file = Some(fr);
        fr
    }

    pub fn dummy_file(&mut self) -> FileRef {
        if let Some(fr) = self.dummy_file {
            fr
        } else {
            let idx = self.code_files.len();
            let fr = FileRef(idx);
            let cf = CodeFile {
                uri: Url::parse("dummy:///tmp/internal/sand_dummy_file.sand").unwrap(),
                name: FileName::dummy(),
                index: fr,
            };
            self.code_files.push(cf);
            self.dummy_file = Some(fr);
            fr
        }
    }

    pub fn set_default_file(&mut self, uri: Url) -> Result<(FileRef, bool), UriError> {
        if let Some(fr) = self.default_file {
            Ok((fr, false))
        } else {
            let fr = self.register_file(uri)?;
            self.default_file = Some(fr);
            Ok((fr, true))
        }
    }

    pub fn get_default_file(&self) -> Option<FileRef> {
        self.default_file
    }

    pub fn url_of_file(&self, file: FileRef) -> Url {
        self.code_files[file.0].uri.clone()
    }
}
