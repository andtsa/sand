//! display implementations for inspecting the different IRs,
//! and formatting parameters
//!
//! todo: move fmt params somewhere configurable (probably by the user)

pub mod ast;
pub mod mir;
pub mod prog;
pub mod typed_expr;

use crate::compiler::context::CompileCtx;
use crate::ir_types::typed_hir::MatchPattern;

/// by default, use 4 spaces for indentation
pub const INDENT: &str = "    ";

/// maximum line length before wrapping
pub const MAX_LINE_LENGTH: usize = 80;

/// Render a [`MatchPattern`] to a source-like string. Shared by the AST dumper
/// and the typed-expression formatter (which previously had byte-identical
/// copies of this).
pub(crate) fn fmt_match_pattern<'tcx>(
    pattern: &MatchPattern<'tcx>,
    ctx: &CompileCtx<'tcx>,
) -> String {
    match pattern {
        MatchPattern::Variant {
            enum_ref,
            variant_idx,
            payload,
            ..
        } => {
            let tag = ctx.enum_display(*enum_ref, *variant_idx);
            match payload {
                Some((_, p)) => format!("{tag}({})", fmt_match_pattern(p, ctx)),
                None => tag,
            }
        }
        MatchPattern::Tuple { elems, .. } => format!(
            "({})",
            elems
                .iter()
                .map(|p| fmt_match_pattern(p, ctx))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        MatchPattern::IntLit(n) => n.to_string(),
        MatchPattern::BoolLit(b) => b.to_string(),
        MatchPattern::Binding { var, .. } => ctx.uniq_variable_name(var),
        MatchPattern::Wildcard => "_".to_string(),
    }
}
