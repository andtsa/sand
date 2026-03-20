//! different analyses of the program

pub mod annotate;
pub mod cfg;
pub mod interactions;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;

use petgraph::graph::NodeIndex;

use crate::analysis::interactions::find_interactions;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::Expr;
use crate::ir_types::typed_hir::TypedProgram;

#[derive(Debug, Clone, Default)]
pub struct ProgramAnnotations {
    /// Map from each expression to all its occurrences in the source code
    pub expr_occurrences: HashMap<Expr, HashSet<(ModuleRef, Range)>>,

    /// Available-expressions set at each CFG node
    pub available_at: HashMap<NodeIndex, HashSet<Expr>>,
}

#[derive(Debug, Clone)]
pub struct AnnotatedExpression {
    pub expr: Expr,
    /// which variables does this expression depend on
    pub depends_on: HashSet<UniqVar>,
    /// which variables does this expression mutate
    pub mutates: HashSet<UniqVar>,
    /// which module does this expression belong to
    pub module: ModuleRef,
}

pub fn analyse(ctx: &CompileCtx, ast: &TypedProgram) -> ProgramAnnotations {
    // create the "control flow graph" - the order in which expressions are
    // evaluated. for example:
    // ```
    // let x = {
    //   let y = 5;
    //   y + 2
    // };
    // x * 3
    // ```
    // here the order of evaluation is
    // // 1. `let y = 5;`
    // // 2. `y + 2`
    // // 3. `let x = { ... };`
    // // 4. `x * 3`
    //
    // note that we need to traverse the AST recursively,
    // meaning that in the AST `a + (b * c)` we need to consider all of
    // `a`, `b`, `c`, `b * c`, and `a + (b * c)` as separate expressions.
    //
    // additionally, the control flow graph should branch for conditionals and
    // loops, and indicate indirection for function calls.
    let cfg = cfg::construct_cfg(ctx, ast);

    // we iterate through the above graph,
    // and for every expression we count how many times it appeared,
    // keeping track of whether the variables it depends on are in the
    // same state as the other instances of the expression.
    let annotations: ProgramAnnotations = find_interactions(cfg);

    annotations
}

pub fn visualise_cfg(ctx: &CompileCtx, program: &TypedProgram) -> String {
    // construct the CFG
    let cfg = cfg::construct_cfg(ctx, program);

    // convert to dot format
    let dot = petgraph::dot::Dot::new(&cfg);

    format!("{dot:?}")
}

impl PartialEq for AnnotatedExpression {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl Eq for AnnotatedExpression {}

impl Hash for AnnotatedExpression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

impl std::fmt::Display for AnnotatedExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.expr)?;

        if !self.depends_on.is_empty() || !self.mutates.is_empty() {
            write!(f, " [")?;

            if !self.depends_on.is_empty() {
                write!(
                    f,
                    "deps: {}",
                    self.depends_on
                        .iter()
                        .map(|x| format!("{x:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }

            if !self.mutates.is_empty() {
                if !self.depends_on.is_empty() {
                    write!(f, "; ")?;
                }
                write!(
                    f,
                    "muts: {}",
                    self.mutates
                        .iter()
                        .map(|x| format!("{x:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }

            write!(f, "]")?;
        }

        Ok(())
    }
}
