//! The [`Project`] struct and related types.
//!
//! This is the main entry point for the compiler,
//! and is responsible for managing the source files and their contents,
//! as well as orchestrating the compilation process.

pub mod init;

use std::path::PathBuf;

use url::Url;

use crate::SandLangError;
use crate::Stage;
use crate::compiler::context::CompileCtx;
use crate::compiler::context::ProjectCtx;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::UriError;
use crate::internal_bug;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir::TypedProgram;
use crate::run;
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

    /// Run the pipeline up to `target` and return the full [`Compilation`]
    /// (every reached stage's program + accumulated diagnostics). This is the
    /// general entry point; [`Self::check`] and [`Self::check_ide`] are
    /// convenience wrappers for the two common targets.
    ///
    /// Stateless and may be called repeatedly.
    pub fn check_to(&self, target: Stage) -> Compilation {
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

        let out = run(modules, &mut ctx, target);
        Compilation {
            diagnostics: out.diagnostics,
            mono: out.mono,
            typed: out.typed,
            qualified: out.qualified,
            reached: out.reached,
            first_error: out.first_error,
            ctx,
        }
    }

    /// Compile to the fully monomorphised program (for MIR lowering, codegen,
    /// interpretation). All-or-nothing, as a [`CheckResult`].
    pub fn check(&self) -> CheckResult {
        let c = self.check_to(Stage::Monomorphised);
        let diagnostics = c.diagnostics;
        match c.mono {
            Some(ast) => CheckResult::Success {
                ast,
                diagnostics,
                ctx: c.ctx,
            },
            None => CheckResult::Failure {
                error: c
                    .first_error
                    .unwrap_or_else(|| internal_bug!("compile failed without an error")),
                diagnostics,
                ctx: c.ctx,
            },
        }
    }

    /// Compile only as far as IDE features need: the **pre-monomorphisation**,
    /// source-faithful typed program (so generic functions keep their real
    /// signatures and unused generics remain visible), produced *before* heap
    /// lowering while still running ownership for its diagnostics. On any fatal
    /// error this is a `Failure` (the caller's `last_good` keeps the previous
    /// good analysis alive).
    pub fn check_ide(&self) -> CheckResult {
        let c = self.check_to(Stage::Owned);
        let diagnostics = c.diagnostics;
        match c.typed {
            // `typed` is captured pre-heap-lower/pre-mono; require no fatal error
            // so a borrow-check failure surfaces (and falls back to `last_good`).
            Some(ast) if c.first_error.is_none() => CheckResult::Success {
                ast,
                diagnostics,
                ctx: c.ctx,
            },
            _ => CheckResult::Failure {
                error: c
                    .first_error
                    .unwrap_or_else(|| internal_bug!("ide check failed without an error")),
                diagnostics,
                ctx: c.ctx,
            },
        }
    }
}

/// The full result of running the pipeline (via [`Project::check_to`]): the
/// program at each reached stage, plus accumulated diagnostics. `ctx` owns the
/// arena the programs borrow, so it is declared **last** (dropped after the
/// borrowing fields).
pub struct Compilation {
    pub diagnostics: SandDiagnostics,
    pub mono: Option<TypedProgram<'static>>,
    pub typed: Option<TypedProgram<'static>>,
    pub qualified: Option<qhir::Program<'static>>,
    pub reached: Stage,
    pub first_error: Option<SandLangError<'static>>,
    pub ctx: CompileCtx<'static>,
}

pub enum CheckResult {
    // Field order is load-bearing: `ast`/`error` borrow the arena that `ctx`
    // owns and frees on `Drop`. Struct fields drop in declaration order, so the
    // borrower must come *before* `ctx`; otherwise the arena would be freed
    // while the borrowing value is still being dropped. (The borrowers are
    // `Copy`/trivial-drop today, so this is defensive, but it makes the drop
    // order correct by construction rather than by that invariant.)
    //
    // `diagnostics` carries *every* accumulated diagnostic (not just the single
    // fatal `error`), so consumers can surface multiple errors / warnings at
    // once. It owns no arena borrow, so its drop position is immaterial.
    Success {
        ast: TypedProgram<'static>,
        diagnostics: SandDiagnostics,
        ctx: CompileCtx<'static>,
    },
    Failure {
        error: SandLangError<'static>,
        diagnostics: SandDiagnostics,
        ctx: CompileCtx<'static>,
    },
}

impl CheckResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, CheckResult::Success { .. })
    }

    pub fn is_err(&self) -> bool {
        matches!(self, CheckResult::Failure { .. })
    }

    /// Every accumulated diagnostic (errors *and* warnings), regardless of
    /// success or failure.
    pub fn diagnostics(&self) -> &SandDiagnostics {
        match self {
            CheckResult::Success { diagnostics, .. } | CheckResult::Failure { diagnostics, .. } => {
                diagnostics
            }
        }
    }

    pub fn ctx_err(self) -> Option<(CompileCtx<'static>, SandLangError<'static>)> {
        match self {
            CheckResult::Success { .. } => None,
            CheckResult::Failure { ctx, error, .. } => Some((ctx, error)),
        }
    }

    pub fn result(
        self,
    ) -> Result<
        (CompileCtx<'static>, TypedProgram<'static>),
        (CompileCtx<'static>, SandLangError<'static>),
    > {
        match self {
            CheckResult::Success { ctx, ast, .. } => Ok((ctx, ast)),
            CheckResult::Failure { ctx, error, .. } => Err((ctx, error)),
        }
    }

    pub fn result_leaked(
        self,
    ) -> Result<(CompileCtx<'static>, TypedProgram<'static>), SandLangError<'static>> {
        match self {
            CheckResult::Success { ctx, ast, .. } => Ok((ctx, ast)),
            CheckResult::Failure { ctx, error, .. } => {
                // See `err`: leak `ctx` so the arena outlives the borrowed error.
                std::mem::forget(ctx);
                Err(error)
            }
        }
    }

    pub fn ctx(self) -> CompileCtx<'static> {
        match self {
            CheckResult::Success { ctx, .. } | CheckResult::Failure { ctx, .. } => ctx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Function-granular recovery: two functions that each fail to type-check
    /// must yield *two* diagnostics (one per function), not just the first,
    /// and the pipeline must still reach the `Typed` stage.
    #[test]
    fn multiple_type_errors_are_all_reported() {
        let mut proj = Project::empty();
        proj.create_virtual_file(
            "def f(): Int := true\ndef g(): Int := false".to_string(),
            "m",
        );

        let c = proj.check_to(Stage::Typed);
        assert_eq!(c.reached, Stage::Typed, "should still reach Typed");
        let total: usize = c.diagnostics.map.values().map(Vec::len).sum();
        assert_eq!(total, 2, "expected one diagnostic per broken function");
        assert!(c.typed.is_some(), "partial typed program retained");
    }

    /// A broken function must not suppress checking of the *following* ones: a
    /// function defined after the broken one still ends up in the partial typed
    /// program (the old `?`-on-first-error path would have discarded it).
    #[test]
    fn a_function_after_a_broken_one_still_type_checks() {
        let mut proj = Project::empty();
        proj.create_virtual_file(
            "def bad(): Int := true\ndef after(x: Int): Int := x".to_string(),
            "m",
        );

        let c = proj.check_to(Stage::Typed);
        let total: usize = c.diagnostics.map.values().map(Vec::len).sum();
        assert_eq!(total, 1, "only the broken function errors");
        let typed = c.typed.expect("partial typed program");
        assert!(
            typed
                .functions
                .values()
                .any(|f| c.ctx.original_fun_name(f.name) == "after"),
            "the function defined after the broken one survives recovery"
        );
    }

    /// Statement-level recovery: two independently ill-typed statements in the
    /// *same* function both produce a diagnostic (the old first-error path
    /// would have reported only the first).
    #[test]
    fn multiple_type_errors_in_one_function_are_all_reported() {
        let mut proj = Project::empty();
        proj.create_virtual_file(
            "def main(): Int := {\n\
             \x20  let x: Int = true;\n\
             \x20  let y: Bool = 5;\n\
             \x20  0\n\
             }"
            .to_string(),
            "m",
        );

        let c = proj.check_to(Stage::Typed);
        assert_eq!(c.reached, Stage::Typed);
        let total: usize = c.diagnostics.map.values().map(Vec::len).sum();
        assert_eq!(total, 2, "both bad statements should be reported");
    }

    /// two functions whose signatures each
    /// reference an unknown type both produce a diagnostic; the pipeline halts
    /// at `Parsed` (a partial declaration set is not safe to qualify).
    #[test]
    fn multiple_build_errors_are_all_reported() {
        let mut proj = Project::empty();
        proj.create_virtual_file(
            "def f(x: Bogus): Int := 0\n\
             def g(y: Alsobad): Int := 0\n\
             def main(): Int := 0"
                .to_string(),
            "m",
        );

        let c = proj.check_to(Stage::Typed);
        assert_eq!(c.reached, Stage::Parsed, "build errors halt at Parsed");
        let total: usize = c.diagnostics.map.values().map(Vec::len).sum();
        assert_eq!(total, 2, "both unknown-type signatures should be reported");
    }

    /// A function that fails to build does not suppress the others: a good
    /// function still parses past a bad one (and the bad one is reported).
    #[test]
    fn a_good_function_survives_a_bad_build() {
        let mut proj = Project::empty();
        proj.create_virtual_file(
            "def bad(x: Bogus): Int := 0\n\
             def good(): Int := 0"
                .to_string(),
            "m",
        );

        let c = proj.check_to(Stage::Typed);
        let total: usize = c.diagnostics.map.values().map(Vec::len).sum();
        assert_eq!(total, 1, "only the bad function errors");
    }

    /// A failed `let` binds its name at `Top`, so a later use does not cascade
    /// into a spurious second error.
    #[test]
    fn failed_declaration_does_not_cascade() {
        let mut proj = Project::empty();
        proj.create_virtual_file(
            "def main(): Int := {\n\
             \x20  let x: Int = true;\n\
             \x20  x\n\
             }"
            .to_string(),
            "m",
        );

        let c = proj.check_to(Stage::Typed);
        let total: usize = c.diagnostics.map.values().map(Vec::len).sum();
        assert_eq!(total, 1, "the use of `x` must not add a cascade error");
    }
}
