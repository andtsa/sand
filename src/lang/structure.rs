//! types for structuring projects

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleRef {
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileRef {
    path: PathBuf,
    name: String,
    module: ModuleRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnName(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Project {
    pub config: ProjectConfig,
    pub files: Vec<FileRef>,
    pub modules: Vec<ModuleRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectConfig {
    //todo
}
