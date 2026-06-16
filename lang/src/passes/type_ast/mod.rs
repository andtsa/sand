//! take a parsed and uniquified AST,
//! annotate expressions with their types,
//! check them for correctness,
//! and output a TypedProgram AST

mod check;
pub(crate) mod errors;
pub(crate) mod generics;
mod infer;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::UniqVar;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedFunction;
use crate::lang::types::Kind;
use crate::lang::types::Region;
use crate::lang::types::Ty;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::errors::TypeError;
use crate::passes::type_ast::infer::infer_function;

/// Type-checking environment: each in-scope variable maps to its type, the kind
/// of the value bound to it, whether it is mutable, and the *home region*: the
/// lexical scope (function or block) it was bound in. The home region drives
/// the borrow escape check: a borrow `&v` lives in `v`'s home region,
/// and a block may not yield a value borrowing a region introduced inside it
/// (Calculus: The Escape Check).
type TypeEnv<'tcx> = im::HashMap<UniqVar<'tcx>, (Ty<'tcx>, Kind, bool, Region)>;

impl<'tcx> typed_hir::TypedProgram<'tcx> {
    /// Type-check every function, recovering at function granularity: a
    /// function that fails to check is dropped from the returned program
    /// but does **not** abort the others, and *all* of its errors are
    /// collected. The caller (`pipeline::run`) reports every error, and,
    /// only when the error list is empty, lets the (now complete) program
    /// flow on to heap-lowering / mono. The partial program is still useful
    /// to IDE / formatting consumers, which read it pre-mono.
    ///
    /// Returns the (possibly partial) program together with one [`TypeError`]
    /// per function that failed.
    pub fn from_ast_program(
        ctx: &mut CompileCtx<'tcx>,
        ast: qhir::Program<'tcx>,
    ) -> (Self, Vec<TypeError<'tcx>>) {
        // sequential loop (rather than `.map`) because `infer_function` needs
        // `&mut CompileCtx` (type checking interns fresh `TyKind::Tuple`s as
        // it encounters tuple literals, so the interner must be writable
        // while the pass runs.
        let mut fn_list: Vec<(FunRef<'tcx>, TypedFunction<'tcx>)> =
            Vec::with_capacity(ast.functions.len());
        let mut errors: Vec<TypeError<'tcx>> = Vec::new();
        for f in ast.functions.values() {
            match infer_function(ctx, f) {
                Ok(typed) => fn_list.push(typed),
                Err(e) => errors.push(e),
            }
        }

        let functions = fn_list.into_iter().collect::<Map<_, _>>();

        (typed_hir::TypedProgram { functions }, errors)
    }
}
