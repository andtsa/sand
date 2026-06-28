//! LSP go-to-definition implementation

use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::compiler::context::DefTarget;
use lang::ir_types::typed_hir::Expression;
use lang::ir_types::typed_hir::TypedProgram;
use tower_lsp::lsp_types::Location;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::Url;

use crate::util::file_and_pos;
use crate::util::find_in_expr;
use crate::util::lsp_range_from_pest;
use crate::util::range_contains;
use crate::util::url_of_module;

pub fn definition_at_position<'tcx>(
    lsp_pos: Position,
    uri: &Url,
    ctx: &CompileCtx<'tcx>,
    ast: &TypedProgram<'tcx>,
    project: &Project,
) -> Option<Location> {
    let (file_ref, pos) = file_and_pos(project, uri, lsp_pos)?;

    // Type / typeclass name references (signatures, annotations, payloads,
    // `impl`/`where`/`requires` heads) live outside the expression tree, so they
    // are resolved from a side table recorded during AST building. Checked first
    // because a type name in a signature falls within the function's header span.
    if let Some(target) = ctx.type_ref_at(file_ref, pos) {
        return def_location_for_target(target, ctx, project);
    }

    for fun in ast.functions.values() {
        if ctx.file_of_module(fun.src_module) != file_ref {
            continue;
        }

        // Cursor on the function name/header, already at the definition, nothing to
        // jump to.
        if range_contains(fun.range, pos) {
            return None;
        }

        // Cursor on a parameter, also already at the definition site.
        if fun.parameters.iter().any(|p| range_contains(p.range, pos)) {
            return None;
        }

        let Some(expr) = find_in_expr(&fun.body, pos) else {
            continue;
        };

        return match &expr.expr {
            Expression::Var(uv) => {
                // Variables are always local to their enclosing function's file.
                let def_range = ctx.uniq_var_declaration(uv);
                let var_file = ctx.file_of_module(fun.src_module);
                let var_text = project.text_for_file(var_file)?;
                let var_uri = project.uri_of_file(var_file);
                Some(Location {
                    uri: var_uri,
                    range: lsp_range_from_pest(var_text, def_range),
                })
            }

            Expression::Call { fn_name, .. } => {
                let orig = ctx.original_fun(fn_name);
                let def_uri = url_of_module(orig.module, ctx, project)?;
                let def_file = ctx.file_of_module(orig.module);
                let def_text = project.text_for_file(def_file)?;
                Some(Location {
                    uri: def_uri,
                    range: lsp_range_from_pest(def_text, orig.declaration),
                })
            }

            Expression::Constructor { enum_ref, .. } => {
                let enum_def = ctx.get_enum(*enum_ref);
                let def_uri = url_of_module(enum_def.src_module, ctx, project)?;
                let def_file = ctx.file_of_module(enum_def.src_module);
                let def_text = project.text_for_file(def_file)?;
                Some(Location {
                    uri: def_uri,
                    range: lsp_range_from_pest(def_text, enum_def.range),
                })
            }

            // A polymorphic typeclass-method call (one whose receiver is still a
            // type parameter): jump to the method's declaration in its
            // typeclass. A method call on a *concrete* receiver was already
            // rewritten to a direct `Call` of the impl method (handled above), so
            // that jumps to the impl instead.
            Expression::MethodCall { class, method, .. } => {
                let tc = ctx.get_typeclass(*class);
                let mdef = tc.methods.get(method)?;
                let def_uri = url_of_module(tc.src_module, ctx, project)?;
                let def_file = ctx.file_of_module(tc.src_module);
                let def_text = project.text_for_file(def_file)?;
                Some(Location {
                    uri: def_uri,
                    range: lsp_range_from_pest(def_text, mdef.range),
                })
            }

            // Intrinsics and literals have no source definition to jump to.
            _ => None,
        };
    }
    None
}

/// The declaration location of an ADT or typeclass a type-name reference points
/// at.
fn def_location_for_target<'tcx>(
    target: DefTarget<'tcx>,
    ctx: &CompileCtx<'tcx>,
    project: &Project,
) -> Option<Location> {
    let (module, decl_range) = match target {
        DefTarget::Adt(er) => {
            let def = ctx.get_enum(er);
            (def.src_module, def.range)
        }
        DefTarget::Typeclass(tref) => {
            let tc = ctx.get_typeclass(tref);
            (tc.src_module, tc.range)
        }
    };
    let def_uri = url_of_module(module, ctx, project)?;
    let def_file = ctx.file_of_module(module);
    let def_text = project.text_for_file(def_file)?;
    Some(Location {
        uri: def_uri,
        range: lsp_range_from_pest(def_text, decl_range),
    })
}
