//! take a parsed and uniquified AST,
//! annotate expressions with their types,
//! check them for correctness,
//! and output a TypedProgram AST

mod check;
mod errors;
mod generics;
mod infer;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::UniqVar;
use crate::ir_types::qhir;
use crate::ir_types::typed_hir;
use crate::ir_types::typed_hir::TypedFunction;
use crate::lang::types::Ty;
pub use crate::passes::type_ast::errors::AstTypeError;
use crate::passes::type_ast::errors::TypeError;
use crate::passes::type_ast::infer::infer_function;

type TypeEnv<'tcx> = im::HashMap<UniqVar<'tcx>, (Ty<'tcx>, bool)>; // (type, is_mutable)

impl<'tcx> typed_hir::TypedProgram<'tcx> {
    pub fn from_ast_program(
        ctx: &mut CompileCtx<'tcx>,
        ast: qhir::Program<'tcx>,
    ) -> Result<Self, TypeError<'tcx>> {
        // sequential loop (rather than `.map`) because `infer_function` needs
        // `&mut CompileCtx` (type checking interns fresh `TyKind::Tuple`s as
        // it encounters tuple literals, so the interner must be writable
        // while the pass runs — see the interior-mutability note in
        // ENUM_PAYLOADS_TUPLES.todo.md §1).
        let mut fn_list: Vec<(FunRef<'tcx>, TypedFunction<'tcx>)> =
            Vec::with_capacity(ast.functions.len());
        for f in ast.functions.values() {
            fn_list.push(infer_function(ctx, f)?);
        }

        let functions = fn_list.into_iter().collect::<Map<_, _>>();

        Ok(typed_hir::TypedProgram { functions })
    }
}
