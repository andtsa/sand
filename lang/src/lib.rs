//! the sand compiler
#![allow(clippy::result_large_err)]

use thiserror::Error;

use crate::compiler::context::CompileCtx;
use crate::compiler::diagnostics::SandDiagnostic;
use crate::compiler::diagnostics::SandDiagnostics;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::ir_types::hhir;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedProgram;

pub mod analysis;
pub mod castles;
pub mod compiler;
pub mod interpreter;
pub mod ir_types;
pub mod lang;
pub mod passes;
pub mod util;

pub use util::bugs::*;

#[derive(Debug, Error)]
#[error("compilation error: {kind}")]
pub struct SandLangError<'tcx> {
    pub context: SandLangErrorContext<'tcx>,
    pub kind: SandLangErrorSource<'tcx>,
}

#[derive(Debug, Default)]
pub struct SandLangErrorContext<'tcx> {
    pub module: Option<ModuleRef<'tcx>>,
    pub file: Option<FileRef>,
}

#[derive(Debug, Error)]
pub enum SandLangErrorSource<'tcx> {
    #[error("parse error: {0}")]
    AstParseError(#[from] passes::build_ast::AstError),
    #[error("qualify error: {0}")]
    QualifyError(passes::qualify::error::QualifyError<'tcx>),
    #[error("type error: {0}")]
    TypeError(passes::type_ast::AstTypeError<'tcx>),
    #[error("ownership error: {0}")]
    OwnershipError(#[from] passes::ownership::errors::OwnershipError),
    /// A pass panicked (a compiler bug / unhandled invariant). Caught at the
    /// pass boundary in [`run`] and turned into this so it surfaces as a
    /// diagnostic instead of unwinding the whole process.
    #[error("internal compiler error: {0}")]
    InternalError(String),
}

impl<'tcx> From<passes::type_ast::AstTypeError<'tcx>> for SandLangErrorSource<'tcx> {
    fn from(e: passes::type_ast::AstTypeError<'tcx>) -> Self {
        SandLangErrorSource::TypeError(e)
    }
}

impl<'tcx> From<passes::qualify::error::QualifyError<'tcx>> for SandLangErrorSource<'tcx> {
    fn from(e: passes::qualify::error::QualifyError<'tcx>) -> Self {
        SandLangErrorSource::QualifyError(e)
    }
}

const CORE_SRC: &str = include_str!("core.sand");

/// A point in the compilation pipeline a consumer can run *up to*. Ordered, so
/// `run(.., target)` executes every stage `<= target`. `Start` is the
/// pre-parse zero value (never a valid target).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Start,
    /// every file parsed into HHIR.
    Parsed,
    /// names resolved (`qhir::Program`).
    Qualified,
    /// type-checked (`TypedProgram`), *before* heap-lowering — the
    /// source-faithful form IDE features want.
    Typed,
    /// heap-lowered + ownership-checked (pre-monomorphisation).
    Owned,
    /// monomorphised — concrete types only, ready for MIR / codegen.
    Monomorphised,
}

/// Best-effort output of the pipeline: whatever programs were produced up to
/// the requested (or last reachable) stage, plus accumulated diagnostics. A
/// stage's `Option` is `Some` once that stage is reached and its output
/// retained.
pub struct PipelineOutput<'tcx> {
    /// name-resolved program (retained only when `target == Qualified`).
    pub qualified: Option<qhir::Program<'tcx>>,
    /// type-checked, pre-heap-lower / pre-mono program: the IDE-facing form.
    pub typed: Option<TypedProgram<'tcx>>,
    /// fully monomorphised program for MIR lowering / codegen / interpreting.
    pub mono: Option<TypedProgram<'tcx>>,
    pub diagnostics: SandDiagnostics,
    /// The furthest stage actually completed.
    pub reached: Stage,
    /// The first fatal error, retained for the `Result`-returning
    /// [`compile_hir`] shim and the legacy `CheckResult::Failure` path.
    pub first_error: Option<SandLangError<'tcx>>,
}

/// Record a fatal error: convert it to diagnostics (merged into the sink) and
/// keep it as `first_error`.
fn record_error<'tcx>(
    out: &mut PipelineOutput<'tcx>,
    ctx: &CompileCtx<'tcx>,
    err: SandLangError<'tcx>,
) {
    let diags = SandDiagnostic::from_compiler_error(ctx, &err);
    for (file, ds) in diags.map {
        out.diagnostics.add(file, ds);
    }
    if out.first_error.is_none() {
        out.first_error = Some(err);
    }
}

/// Run `f`, catching any panic (a compiler bug in a pass) and returning the
/// panic message instead of unwinding. The closure is `AssertUnwindSafe`: on a
/// caught panic the pipeline stops immediately and only *reads* `ctx` afterward
/// (to render the diagnostic), so a partially-mutated context is acceptable.
fn catch_ice<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|payload| {
        if let Some(s) = payload.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        }
    })
}

/// Record a caught internal compiler error (a panic in `pass`), anchored to
/// `anchor` (no source span is available for an ICE).
fn record_ice<'tcx>(
    out: &mut PipelineOutput<'tcx>,
    ctx: &CompileCtx<'tcx>,
    anchor: FileRef,
    pass: &str,
    msg: String,
) {
    let ectx = SandLangErrorContext {
        module: None,
        file: Some(anchor),
    };
    let err = ectx.wrap_err(SandLangErrorSource::InternalError(format!("{pass}: {msg}")));
    record_error(out, ctx, err);
}

/// Run the compilation pipeline up to `target`, then drain any non-fatal
/// diagnostics (warnings emitted by passes via [`CompileCtx::warn`]) into the
/// sink. The drain happens here, after every early-return path, so warnings
/// survive regardless of where [`run_pipeline`] stopped.
pub fn run<'proj>(
    code: Map<FileRef, &'_ str>,
    ctx: &mut CompileCtx<'proj>,
    target: Stage,
) -> PipelineOutput<'proj> {
    let mut out = run_pipeline(code, ctx, target);
    for d in ctx.diagnostics.drain(..) {
        if let Some(file) = d.file {
            out.diagnostics.add_one(file, d);
        }
    }
    out
}

/// Run the compilation pipeline up to `target` (or until a fatal error stops
/// it), accumulating diagnostics and retaining each reached stage's output.
/// This is the single place the passes are strung together; consumers pick the
/// stage they need ([`compile_hir`] = `Monomorphised`).
fn run_pipeline<'proj>(
    code: Map<FileRef, &'_ str>,
    ctx: &mut CompileCtx<'proj>,
    target: Stage,
) -> PipelineOutput<'proj> {
    let span = tracing::warn_span!("pipeline", ?target);
    let _enter = span.enter();

    // Capture user files before `code` is consumed by the parse loop; an ICE in
    // a later pass has no source span, so it's anchored to the first user file
    // (falling back to the core file if there are none).
    let user_files: Vec<FileRef> = code.keys().copied().collect();

    let mut out = PipelineOutput {
        qualified: None,
        typed: None,
        mono: None,
        diagnostics: SandDiagnostics::default(),
        reached: Stage::Start,
        first_error: None,
    };

    // ── parse ────────────────────────────────────────────────────────────────
    let core_file = ctx.ensure_core_module();
    let core_modules = match hhir::ProgramModule::parse_source_file(ctx, CORE_SRC, core_file) {
        Ok(m) => m,
        Err(e) => {
            record_error(&mut out, ctx, SandLangErrorContext::default().wrap_err(e));
            return out;
        }
    };
    let mut modules = core_modules;
    for (file, source) in code {
        match hhir::ProgramModule::parse_source_file(ctx, source, file) {
            Ok(mut m) => modules.append(&mut m),
            Err(e) => {
                let ectx = SandLangErrorContext {
                    module: None,
                    file: Some(file),
                };
                record_error(&mut out, ctx, ectx.wrap_err(e));
                return out;
            }
        }
    }
    out.reached = Stage::Parsed;
    if target <= Stage::Parsed {
        return out;
    }

    // ── qualify ──────────────────────────────────────────────────────────────
    let program = match qhir::Program::combine(ctx, modules) {
        Ok(p) => p,
        Err(e) => {
            ctx.entrypoint = None;
            let ectx = SandLangErrorContext::with_module(e.source_module().index);
            record_error(&mut out, ctx, ectx.wrap_err(e));
            return out;
        }
    };
    out.reached = Stage::Qualified;
    if target <= Stage::Qualified {
        out.qualified = Some(program);
        return out;
    }

    // ── type-check ───────────────────────────────────────────────────────────
    // Function-granular recovery: `from_ast_program` checks every function and
    // returns the ones that succeeded plus *all* the errors. We always reach
    // `Typed` (the partial program is still useful to IDE / formatting), but a
    // non-empty error list halts the pipeline before heap-lowering, since the
    // downstream passes assume a complete, well-typed program.
    let (typed, type_errors) = typed_hir::TypedProgram::from_ast_program(ctx, program);
    out.reached = Stage::Typed;
    if !type_errors.is_empty() {
        ctx.entrypoint = None;
        for e in type_errors {
            let ectx = SandLangErrorContext::with_module(e.module);
            record_error(&mut out, ctx, ectx.wrap_err(e.error));
        }
        out.typed = Some(typed);
        return out;
    }
    if target <= Stage::Typed {
        out.typed = Some(typed);
        return out;
    }
    // Retain the pre-mono program for IDE consumers, then lower a copy. (Heap
    // lowering and monomorphisation transform/erase the program, so the
    // source-faithful form must be captured here.)
    out.typed = Some(typed.clone());

    let ice_anchor = user_files.first().copied().unwrap_or(core_file);

    // ── heap-lower + ownership ───────────────────────────────────────────────
    // Heap lowering rewrites every `deriving Heaped` enum into a `Unique<Node>`
    // handle *before* ownership (uniform drops) and *before* mono (so the
    // injected `unique_*` calls instantiate normally).
    let lowered = match catch_ice(|| passes::heap_lower::lower(ctx, typed)) {
        Ok(l) => l,
        Err(msg) => {
            // ICE in heap-lowering: `out.typed` (the pre-mono program) is already
            // retained above, so IDE features survive; record the bug and stop.
            ctx.entrypoint = None;
            record_ice(&mut out, ctx, ice_anchor, "heap lowering", msg);
            return out;
        }
    };
    let owned = match passes::ownership::check(ctx, lowered) {
        Ok(o) => o,
        Err(errs) => {
            // Ownership already yields *all* its errors — record every one.
            for e in errs {
                let ectx = SandLangErrorContext::with_module(e.module);
                record_error(&mut out, ctx, ectx.wrap_err(e.error));
            }
            return out;
        }
    };
    out.reached = Stage::Owned;
    if target <= Stage::Owned {
        return out;
    }

    // ── monomorphise ─────────────────────────────────────────────────────────
    let mono = match catch_ice(|| passes::mono::monomorphise(ctx, &owned)) {
        Ok(m) => m,
        Err(msg) => {
            ctx.entrypoint = None;
            record_ice(&mut out, ctx, ice_anchor, "monomorphisation", msg);
            return out;
        }
    };
    out.reached = Stage::Monomorphised;
    out.mono = Some(mono);
    out
}

/// Compile to the fully monomorphised program. Thin `Result`-returning shim
/// over [`run`] for consumers (CLI, tests) that want all-or-nothing behaviour.
pub fn compile_hir<'proj>(
    code: Map<FileRef, &'_ str>,
    ctx: &mut CompileCtx<'proj>,
) -> Result<TypedProgram<'proj>, SandLangError<'proj>> {
    let out = run(code, ctx, Stage::Monomorphised);
    match out.mono {
        Some(program) => Ok(program),
        None => Err(out
            .first_error
            .unwrap_or_else(|| internal_bug!("pipeline produced no program and no error"))),
    }
}

impl<'tcx> SandLangErrorContext<'tcx> {
    pub fn with_module(module: ModuleRef<'tcx>) -> Self {
        Self {
            module: Some(module),
            file: None,
        }
    }

    pub fn wrap_err<E: Into<SandLangErrorSource<'tcx>>>(self, err: E) -> SandLangError<'tcx> {
        SandLangError {
            context: self,
            kind: err.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::catch_ice;

    #[test]
    fn catch_ice_turns_a_panic_into_a_message_and_passes_through_success() {
        // Silence the default panic hook's backtrace for the deliberate panic
        // below, then restore it.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let panicked = catch_ice(|| -> i32 { panic!("L {}", 42) });
        std::panic::set_hook(prev);
        assert_eq!(panicked, Err("L 42".to_string()));

        assert_eq!(catch_ice(|| 7), Ok(7));
    }
}
