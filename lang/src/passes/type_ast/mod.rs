//! take a parsed and uniquified AST,
//! annotate expressions with their types,
//! check them for correctness,
//! and output a TypedProgram AST

mod check;
mod errors;
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

type TypeEnv = im::HashMap<UniqVar, (Ty, bool)>; // (type, is_mutable)

impl typed_hir::TypedProgram {
    pub fn from_ast_program(ctx: &CompileCtx, ast: qhir::Program) -> Result<Self, TypeError> {
        let fn_list = ast
            .functions
            .values()
            .map(|f| infer_function(ctx, f))
            .collect::<Result<Vec<(FunRef, TypedFunction)>, _>>()?;

        let functions = fn_list.into_iter().collect::<Map<_, _>>();

        Ok(typed_hir::TypedProgram { functions })
    }
}
