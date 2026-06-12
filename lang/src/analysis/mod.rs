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
pub struct ProgramAnnotations<'tcx> {
    /// Map from each expression to all its occurrences in the source code
    pub expr_occurrences: HashMap<Expr<'tcx>, HashSet<(ModuleRef<'tcx>, Range)>>,

    /// Available-expressions set at each CFG node
    pub available_at: HashMap<NodeIndex, HashSet<Expr<'tcx>>>,
}

#[derive(Debug, Clone)]
pub struct AnnotatedExpression<'tcx> {
    pub expr: Expr<'tcx>,
    /// which variables does this expression depend on
    pub depends_on: HashSet<UniqVar<'tcx>>,
    /// which variables does this expression mutate
    pub mutates: HashSet<UniqVar<'tcx>>,
    /// which module does this expression belong to
    pub module: ModuleRef<'tcx>,
}

pub fn analyse<'tcx>(ctx: &CompileCtx<'tcx>, ast: &TypedProgram<'tcx>) -> ProgramAnnotations<'tcx> {
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
    let annotations: ProgramAnnotations<'tcx> = find_interactions(cfg);

    annotations
}

pub fn visualise_cfg<'tcx>(ctx: &CompileCtx<'tcx>, program: &TypedProgram<'tcx>) -> String {
    // construct the CFG
    let cfg = cfg::construct_cfg(ctx, program);

    // convert to dot format
    let dot = petgraph::dot::Dot::new(&cfg);

    format!("{dot:?}")
}

impl PartialEq for AnnotatedExpression<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl Eq for AnnotatedExpression<'_> {}

impl Hash for AnnotatedExpression<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expr.hash(state);
    }
}

impl std::fmt::Display for AnnotatedExpression<'_> {
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

// `Expr` keys reach an enum payload `Cell` through arena references, but hash
// by structural/pointer identity that never reads it — see `find_interactions`.
#[allow(clippy::mutable_key_type)]
pub fn flipped_occurence_map<'a, 'tcx>(
    map: &'a HashMap<Expr<'tcx>, HashSet<(ModuleRef<'tcx>, Range)>>,
) -> HashMap<ModuleRef<'tcx>, HashMap<&'a Expr<'tcx>, HashSet<Range>>> {
    let mut r = HashMap::new();
    for (e, occ) in map {
        for (mr, range) in occ {
            r.entry(*mr)
                .and_modify(|hm: &mut HashMap<&Expr<'tcx>, HashSet<Range>>| {
                    hm.entry(e)
                        .and_modify(|s: &mut HashSet<Range>| {
                            s.insert(*range);
                        })
                        .or_insert(HashSet::from([*range]));
                })
                .or_insert(HashMap::from([(e, HashSet::from([*range]))]));
        }
    }

    r
}
