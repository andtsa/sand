//! The [`Project`] struct and related types.
//!
//! This is the main entry point for the compiler,
//! and is responsible for managing the source files and their contents,
//! as well as orchestrating the compilation process.

pub mod init;

use std::path::PathBuf;

use url::Url;

use crate::SandLangError;
use crate::compile_hir;
use crate::compiler::context::CompileCtx;
use crate::compiler::context::ProjectCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::UriError;
use crate::ir_types::typed_hir::TypedProgram;
use crate::util::fs::real_fs::FileSystem;

pub struct Project {
    ctx: ProjectCtx,
    fs: FileSystem,
    /// content keyed by FileRef (the post-registration canonical key)
    pub file_contents: Map<FileRef, String>,
    config_src: Option<PathBuf>,
}

impl Project {
    pub fn empty() -> Self {
        Self {
            ctx: ProjectCtx::initial(),
            fs: FileSystem { dry_run: false },
            file_contents: Map::new(),
            config_src: None,
        }
    }

    /// Register or update a file by URI. Returns the stable FileRef.
    /// This replaces lsp/files.rs::register_file and the CLI's manual loop.
    pub fn insert_file(&mut self, uri: Url, content: String) -> Result<FileRef, UriError> {
        let fr = self.ctx.register_file(uri)?;
        self.file_contents.insert(fr, content);
        Ok(fr)
    }

    /// Create a virtual file by directly providing the file contents.
    /// Using this FileRef in the LSP module will raise an error when
    /// trying to convert the URL to a [`std::fs::PathBuf`].
    pub fn create_virtual_file(&mut self, content: String, module_name: &str) -> FileRef {
        let fr = self.ctx.register_virtual_file(module_name);
        self.file_contents.insert(fr, content);
        fr
    }

    /// look up source text by [`FileRef`]
    pub fn text_for_file(&self, fr: FileRef) -> Option<&str> {
        // sentinel FileRefs (e.g. the core library) are not in file_contents
        self.file_contents.get(&fr).map(String::as_str)
    }

    /// returns whether this FileRef refers to a synthetic (compiler-internal)
    /// file that has no on-disk representation, such as the core standard
    /// library
    pub fn is_synthetic_file(&self, fr: FileRef) -> bool {
        !self.file_contents.contains_key(&fr)
    }

    /// we need a name for this module to use for function qualifying. we will
    /// use the filename (without extension) as the module name, but this is not
    /// guaranteed to be unique. we will check for duplicates and warn about
    /// them, but we will still allow them for now.
    pub fn default_modname_for_file(&self, fr: FileRef) -> String {
        let cf = self.ctx.code_file(fr);
        cf.module_name()
    }

    pub fn file_name(&self, fr: FileRef) -> String {
        self.ctx.code_file(fr).file_name()
    }

    pub fn file_count(&self) -> usize {
        self.file_contents.len()
    }

    pub fn uri_of_file(&self, fr: FileRef) -> Url {
        if self.is_synthetic_file(fr) {
            // synthetic files (core library) have no real URI; return a placeholder
            return Url::parse("sand:/__core__").unwrap();
        }
        self.ctx.url_of_file(fr)
    }

    pub fn is_tracked(&self, uri: &Url) -> Option<FileRef> {
        self.ctx.files.get_by_left(uri).copied()
    }

    pub fn config_path(&self) -> Option<&PathBuf> {
        self.config_src.as_ref()
    }

    pub fn config_url(&self) -> Option<Url> {
        self.config_src
            .as_ref()
            .and_then(|p| Url::from_file_path(p).ok())
    }

    /// Compile the entire project to the typed AST.
    /// If a program makes it to this point, it is considered syntactically
    /// valid, and later passes should not produce any errors.
    ///
    /// this call is stateless, and may be called repeatedly.
    pub fn check(&self) -> CheckResult {
        let mut ctx = CompileCtx::initial();
        // map each file to its &content
        let modules: Map<FileRef, &str> = self
            .file_contents
            .iter()
            .map(|(&fr, s)| {
                ctx.create_default_module(fr, &self.default_modname_for_file(fr));
                (fr, s.as_str())
            })
            .collect();

        tracing::debug!(
            "project contains modules: {:?}",
            modules
                .keys()
                .map(|fr| self.default_modname_for_file(*fr))
                .collect::<Vec<_>>()
        );

        match compile_hir(modules, &mut ctx) {
            Ok(ast) => CheckResult::Success { ctx, ast },
            Err(error) => CheckResult::Failure { ctx, error },
        }
    }
}

pub enum CheckResult {
    Success {
        ctx: CompileCtx<'static>,
        ast: TypedProgram<'static>,
    },
    Failure {
        ctx: CompileCtx<'static>,
        error: SandLangError<'static>,
    },
}

impl CheckResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, CheckResult::Success { .. })
    }

    pub fn is_err(&self) -> bool {
        matches!(self, CheckResult::Failure { .. })
    }

    pub fn ctx_err(self) -> Option<(CompileCtx<'static>, SandLangError<'static>)> {
        match self {
            CheckResult::Success { .. } => None,
            CheckResult::Failure { ctx, error } => Some((ctx, error)),
        }
    }

    pub fn result(
        self,
    ) -> Result<
        (CompileCtx<'static>, TypedProgram<'static>),
        (CompileCtx<'static>, SandLangError<'static>),
    > {
        match self {
            CheckResult::Success { ctx, ast } => Ok((ctx, ast)),
            CheckResult::Failure { ctx, error } => Err((ctx, error)),
        }
    }

    pub fn result_leaked(
        self,
    ) -> Result<(CompileCtx<'static>, TypedProgram<'static>), SandLangError<'static>> {
        match self {
            CheckResult::Success { ctx, ast } => Ok((ctx, ast)),
            CheckResult::Failure { ctx, error } => {
                // See `err`: leak `ctx` so the arena outlives the borrowed error.
                std::mem::forget(ctx);
                Err(error)
            }
        }
    }

    pub fn ctx(self) -> CompileCtx<'static> {
        match self {
            CheckResult::Success { ctx, ast: _ } => ctx,
            CheckResult::Failure { ctx, error: _ } => ctx,
        }
    }
}
