//! LSP hover action implementation

use lang::castles::project::Project;
use lang::compiler::context::CompileCtx;
use lang::compiler::context::DefTarget;
use lang::compiler::structure::FileRef;
use lang::compiler::structure::RegionParam;
use lang::compiler::structure::TypeParam;
use lang::compiler::structure::TypeclassRef;
use lang::ir_types::typed_hir::Expr;
use lang::ir_types::typed_hir::Expression;
use lang::ir_types::typed_hir::TypedFunction;
use lang::ir_types::typed_hir::TypedProgram;
use lang::lang::intrinsics::INTRINSICS;
use lang::lang::types::AdtRef;
use lang::lang::types::Kind;
use lang::lang::types::Ty;
use lang::lang::types::Variance;
use tower_lsp::lsp_types::Hover;
use tower_lsp::lsp_types::HoverContents;
use tower_lsp::lsp_types::MarkupContent;
use tower_lsp::lsp_types::MarkupKind;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::Url;

use crate::util::find_in_expr;
use crate::util::pos_from_lsp_position;
use crate::util::range_contains;

/// Step budget for running `main` in a hover preview. high enough for any
/// reasonable program, low enough that an accidental infinite loop aborts fast.
const HOVER_RUN_STEP_BUDGET: u64 = 5_000_000;

pub fn hover_at_position<'tcx>(
    lsp_pos: Position,
    uri: &Url,
    ctx: &CompileCtx<'tcx>,
    ast: &TypedProgram<'tcx>,
    project: &Project,
) -> Option<Hover> {
    let file_ref: FileRef = project.is_tracked(uri)?;
    let text = project.text_for_file(file_ref)?;
    let pos = pos_from_lsp_position(text, lsp_pos);

    // A type / typeclass *name* in a signature, annotation, payload, or
    // `impl`/`where`/`requires` head: show the full declaration. Checked first
    // because these positions fall within a function's header span.
    if let Some(target) = ctx.type_ref_at(file_ref, pos) {
        return Some(make_hover(render_def_target(ctx, target)));
    }

    for fun in ast.functions.values() {
        if ctx.file_of_module(fun.src_module) != file_ref {
            continue;
        }
        // if cursor is on the function name itself,
        // show signature, and run if main
        if range_contains(fun.range, pos) {
            return Some(format_function_hover(fun, ctx, ast));
        }
        // cursor on a parameter
        for param in &fun.parameters {
            if range_contains(param.range, pos) {
                let name = ctx.uniq_variable_name(&param.name);
                let mutability = if param.is_mutable { "mut " } else { "" };
                let mut s = format!(
                    "```sand\n{mutability}{name}: {}\n```\nparameter",
                    fmt_ty(ctx, param.ty)
                );
                append_kind(&mut s, param.ty);
                return Some(make_hover(s));
            }
        }
        if let Some(expr) = find_in_expr(&fun.body, pos) {
            return Some(format_hover(expr, ctx));
        }
    }
    None
}

/// A one-line description of an ownership kind, for hover. `Owned` is the
/// common case and gets no annotation (to avoid noise); the others are
/// surfaced.
fn kind_note(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Borrowed => Some("shared borrow (`&`)"),
        Kind::BorrowedMut => Some("exclusive borrow (`&mut`)"),
        Kind::Never => Some("diverges (never returns)"),
        // `Owned` (the common case) and the type-constructor kinds get no note.
        _ => None,
    }
}

/// Append an ownership note inferred from a type's outermost shape (used where
/// only a type, not an expression kind, is available — e.g. parameters).
fn append_kind(s: &mut String, ty: Ty<'_>) {
    use lang::lang::types::TyKind;
    let note = match ty.kind() {
        TyKind::Ref(..) => Some("shared borrow (`&`)"),
        TyKind::RefMut(..) => Some("exclusive borrow (`&mut`)"),
        _ => None,
    };
    if let Some(n) = note {
        s.push_str(&format!("\n\n*{n}*"));
    }
}

/// Render the declaration a type/typeclass name reference points at.
fn render_def_target<'tcx>(ctx: &CompileCtx<'tcx>, target: DefTarget<'tcx>) -> String {
    match target {
        DefTarget::Adt(er) => render_adt_decl(ctx, er),
        DefTarget::Typeclass(tref) => render_typeclass_decl(ctx, tref),
    }
}

/// Reconstruct an ADT's declaration (`type Name<params> = V1 | V2(payload) …`),
/// plus its `deriving` clause and defining module.
fn render_adt_decl<'tcx>(ctx: &CompileCtx<'tcx>, er: AdtRef<'tcx>) -> String {
    let def = ctx.get_enum(er);
    let params = render_decl_params(ctx, &def.region_params, &def.type_params);
    let variants = def
        .variants
        .iter()
        .map(|v| match v.payload.get() {
            Some(p) => format!("{}({})", v.name, fmt_ty(ctx, p)),
            None => v.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let deriving = if def.heaped_strategy().is_some() {
        " deriving Heaped"
    } else {
        ""
    };
    let module = ctx.module_info(&def.src_module);
    format!(
        "```sand\ntype {}{} = {}{}\n```\nDefined in module `{}`",
        def.name, params, variants, deriving, module.name
    )
}

/// Reconstruct a typeclass's declaration (name, superclasses, method
/// signatures).
fn render_typeclass_decl<'tcx>(ctx: &CompileCtx<'tcx>, tref: TypeclassRef) -> String {
    let tc = ctx.get_typeclass(tref);
    let param = ctx.type_param_name(tc.param);
    let requires = if tc.superclasses.is_empty() {
        String::new()
    } else {
        let names = tc
            .superclasses
            .iter()
            .map(|s| ctx.get_typeclass(*s).name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        format!(" requires {names}")
    };
    let methods = tc
        .method_order
        .iter()
        .filter_map(|m| tc.methods.get(m))
        .map(|md| {
            let generics = if md.type_params.is_empty() {
                String::new()
            } else {
                format!(
                    "<{}>",
                    md.type_params
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let args = md
                .param_tys
                .iter()
                .map(|t| fmt_ty(ctx, *t))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "    def {}{generics}({args}): {}",
                md.name,
                fmt_ty(ctx, md.ret_ty)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let module = ctx.module_info(&tc.src_module);
    format!(
        "```sand\ntypeclass {}<{}>{} {{\n{}\n}}\n```\nDefined in module `{}`",
        tc.name, param, requires, methods, module.name
    )
}

/// Render a declaration's `<region params, type params>` clause, e.g.
/// `<'r, +a, b>`. Region parameters come first (the lifetimes-first
/// convention). Explicit variance (`+`/`-`) is shown; the defaulted variance is
/// not.
fn render_decl_params(
    ctx: &CompileCtx<'_>,
    regions: &[RegionParam],
    types: &[TypeParam],
) -> String {
    if regions.is_empty() && types.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = regions.iter().map(|r| format!("'{}", r.name)).collect();
    for tp in types {
        let prefix = match (tp.explicit_variance, tp.variance) {
            (true, Variance::Covariant) => "+",
            (true, Variance::Contravariant) => "-",
            _ => "",
        };
        // Surface a higher-kinded parameter's constructor kind.
        let kind = if matches!(ctx.type_param_kind(tp.id), Kind::Arrow(_)) {
            " : Owned -> Owned"
        } else {
            ""
        };
        parts.push(format!("{prefix}{}{kind}", tp.name));
    }
    format!("<{}>", parts.join(", "))
}

fn format_function_hover<'tcx>(
    fun: &TypedFunction<'tcx>,
    ctx: &CompileCtx<'tcx>,
    ast: &TypedProgram<'tcx>,
) -> Hover {
    let name = ctx.original_fun_name(fun.name);
    // Build the signature from the function's own parameters/return type (always
    // present) rather than `fun_sig`, which a default/impl method may lack.
    let args = fun
        .parameters
        .iter()
        .map(|p| format!("{}: {}", ctx.uniq_variable_name(&p.name), fmt_ty(ctx, p.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let generics = render_decl_params(ctx, &fun.region_params, &fun.type_params);
    let where_clause = render_where_clause(ctx, fun);
    let extern_kw = if ctx.is_extern(fun.name) {
        "extern "
    } else {
        ""
    };
    let sig_line = format!(
        "```sand\n{extern_kw}def {name}{generics}({args}) -> {}{where_clause}\n```",
        fmt_ty(ctx, fun.ret_type)
    );

    if ctx.is_main(fun.name) {
        let mut output_buf: Vec<u8> = Vec::new();
        // Running arbitrary user code on a passive hover is risky: it may loop
        // forever, recurse without bound, or panic. The bounded runner caps
        // steps + recursion depth and turns a panic into an `Err`, so neither a
        // runaway program nor a compiler bug can hang or crash the server.
        let run_result =
            ast.interpret_with_output_bounded(ctx, &mut output_buf, HOVER_RUN_STEP_BUDGET);
        let printed = String::from_utf8_lossy(&output_buf);

        let content = match run_result {
            Ok(val) => {
                let mut s = sig_line;
                if !printed.is_empty() {
                    s.push_str("\n\n## Output:\n```\n");
                    s.push_str(printed.trim_end());
                    s.push_str("\n```");
                }
                s.push_str(&format!("\n\n## Returned:\n`{}`", fmt_expr_val(&val, ctx)));
                s
            }
            Err(e) => format!("{sig_line}\n\n⚠ Runtime error: {e}"),
        };
        make_hover(content)
    } else {
        let orig = ctx.original_fun(&fun.name);
        let module = ctx.module_info(&orig.module);
        let mut s = format!(
            "{sig_line}\nDefined in module `{}` (line {})",
            module.name, orig.declaration.start.line
        );
        if let Some(sym) = ctx.extern_symbol(fun.name) {
            s.push_str(&format!("\n\nExternal (FFI) — bound to C symbol `{sym}`"));
        }
        make_hover(s)
    }
}

/// Render a function's `where` clause for hover (typeclass + outlives
/// constraints), or the empty string when it has none.
fn render_where_clause<'tcx>(ctx: &CompileCtx<'tcx>, fun: &TypedFunction<'tcx>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for tc in &fun.type_constraints {
        parts.push(format!(
            "{} : {}",
            ctx.type_param_name(tc.param),
            ctx.get_typeclass(tc.class).name
        ));
    }
    let region_name = |r| match r {
        lang::lang::types::Region::Static => "'static".to_string(),
        lang::lang::types::Region::Var(rv) => fun
            .region_params
            .iter()
            .find(|p| p.region == rv)
            .map(|p| format!("'{}", p.name))
            .unwrap_or_else(|| "'_".to_string()),
    };
    for c in &fun.where_constraints {
        parts.push(format!(
            "{} >= {}",
            region_name(c.longer),
            region_name(c.shorter)
        ));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" where {}", parts.join(", "))
    }
}

fn fmt_expr_val<'tcx>(val: &Expression<'tcx>, ctx: &CompileCtx<'tcx>) -> String {
    match val {
        Expression::Int(n) => n.to_string(),
        Expression::Bool(b) => b.to_string(),
        Expression::Unit => "()".to_string(),
        Expression::Constructor {
            enum_ref,
            variant_idx,
            payload,
        } => {
            let tag = ctx.enum_display(*enum_ref, *variant_idx);
            match payload {
                Some(p) => format!("{tag}({})", fmt_expr_val(&p.expr, ctx)),
                None => tag,
            }
        }
        Expression::Tuple(elems) => format!(
            "({})",
            elems
                .iter()
                .map(|e| fmt_expr_val(&e.expr, ctx))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => "<value>".to_string(),
    }
}

fn format_hover<'tcx>(expr: &Expr<'tcx>, ctx: &CompileCtx<'tcx>) -> Hover {
    let mut content = match &expr.expr {
        Expression::Var(uv) => {
            let name = ctx.uniq_variable_name(uv);
            let decl = ctx.uniq_var_declaration(uv);
            format!(
                "```sand\n{}: {}\n```\nlocal — declared at line {}, col {}",
                name,
                fmt_ty(ctx, expr.ty),
                decl.start.line,
                decl.start.col
            )
        }
        Expression::Call { fn_name, .. } => {
            let name = ctx.original_fun_name(*fn_name);
            // A method call rewritten to a `Call` of an impl/default method may
            // reference a `FunRef` with no registered signature; fall back to the
            // call's result type rather than panicking.
            match ctx.try_fun_sig(fn_name) {
                Some(sig) => {
                    let args = fmt_sig_args(&sig.args, ctx);
                    let orig = ctx.original_fun(fn_name);
                    let module = ctx.module_info(&orig.module);
                    format!(
                        "```sand\ndef {name}({args}) -> {}\n```\nDefined in module `{}`",
                        fmt_ty(ctx, sig.ret_ty),
                        module.name
                    )
                }
                None => format!("```sand\n{name}\n```\n: {}", fmt_ty(ctx, expr.ty)),
            }
        }
        // A typeclass method call whose receiver stays polymorphic (resolved at
        // monomorphisation): show the class and the method signature.
        Expression::MethodCall { class, method, .. } => {
            let tc = ctx.get_typeclass(*class);
            match tc.methods.get(method) {
                Some(md) => {
                    let args = md
                        .param_tys
                        .iter()
                        .map(|t| fmt_ty(ctx, *t))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "```sand\ndef {method}({args}) -> {}\n```\nmethod of typeclass `{}`",
                        fmt_ty(ctx, md.ret_ty),
                        tc.name
                    )
                }
                None => format!("method `{method}` of `{}`", tc.name),
            }
        }
        // An enum value: show which variant, and the full type declaration.
        Expression::Constructor {
            enum_ref,
            variant_idx,
            ..
        } => {
            let def = ctx.get_enum(*enum_ref);
            let variant = &def.variants[*variant_idx].name;
            format!(
                "**variant `{variant}` of `{}`** : {}\n\n{}",
                def.name,
                fmt_ty(ctx, expr.ty),
                render_adt_decl(ctx, *enum_ref)
            )
        }
        Expression::IntrinsicCall { fn_name, .. } => {
            if let Some((_, sig)) = INTRINSICS.get(fn_name) {
                let (resolved_args, resolved_ret) = sig.resolve(&ctx.types);
                let args = resolved_args
                    .iter()
                    .map(|&t| fmt_ty(ctx, t).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "```sand\n{fn_name}({args}) -> {}\n```\nbuilt-in intrinsic",
                    fmt_ty(ctx, resolved_ret)
                )
            } else {
                format!("**intrinsic {fn_name}**")
            }
        }
        _ => format!(": {}", fmt_ty(ctx, expr.ty)),
    };
    // Surface the expression's ownership kind (borrow / divergence) — the
    // owned-value case is left unannotated to avoid noise.
    if let Some(note) = kind_note(expr.kind) {
        content.push_str(&format!("\n\n*{note}*"));
    }
    make_hover(content)
}

fn fmt_sig_args<'tcx>(
    args: &[(lang::compiler::structure::UniqVar<'tcx>, Ty<'tcx>)],
    ctx: &CompileCtx<'tcx>,
) -> String {
    args.iter()
        .map(|(uv, ty)| format!("{}: {}", ctx.uniq_variable_name(uv), fmt_ty(ctx, *ty)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_ty<'tcx>(ctx: &CompileCtx<'tcx>, ty: Ty<'tcx>) -> String {
    ctx.display_ty(ty).to_string()
}

fn make_hover(value: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    }
}

#[cfg(test)]
mod tests {
    use lang::castles::project::CheckResult;
    use lang::castles::project::Project;
    use tower_lsp::lsp_types::Position;
    use tower_lsp::lsp_types::Url;

    /// 0-based (line, UTF-16 col) of the first occurrence of `needle` (ASCII).
    fn position_of(src: &str, needle: &str) -> Position {
        let idx = src.find(needle).expect("needle present");
        let prefix = &src[..idx];
        let line = prefix.matches('\n').count() as u32;
        let col = (idx - prefix.rfind('\n').map(|i| i + 1).unwrap_or(0)) as u32;
        Position::new(line, col)
    }

    // A call to a typeclass *default* method lowers to a `Call` of a `FunRef`
    // that has no registered `FunSig`. Hovering it used to panic in `fun_sig`.
    #[test]
    fn hover_over_default_method_call_does_not_panic() {
        let src = "\
typeclass Greet<T> {
    def hello(x: T): Int := 0
    def loud(x: T): Int
}
type P = Mk
impl Greet for P { def loud(x: P): Int := 1 }
def main(): Int := hello(P#Mk) + loud(P#Mk)
";
        let uri = Url::parse("file:///t.sand").unwrap();
        let mut proj = Project::empty();
        proj.insert_file(uri.clone(), src.to_string()).unwrap();
        let result = proj.check();
        let CheckResult::Success { ctx, ast, .. } = &result else {
            panic!("expected success");
        };
        // Hover on the `hello(...)` call — must produce something, not panic.
        let pos = position_of(src, "hello(P#Mk)");
        let hov = super::hover_at_position(pos, &uri, ctx, ast, &proj);
        assert!(
            hov.is_some(),
            "hover over default-method call returned None"
        );
        std::mem::forget(result);
    }

    /// The markdown text of a hover result.
    fn hover_text(h: Option<tower_lsp::lsp_types::Hover>) -> String {
        match h.expect("hover").contents {
            tower_lsp::lsp_types::HoverContents::Markup(m) => m.value,
            _ => panic!("expected markup hover"),
        }
    }

    #[test]
    fn rich_hover_for_types_classes_and_constructors() {
        let src = "\
typeclass Show<T> { def show(x: T): Int }
type Box<a> = Empty | Full(a)
def wrap<T>(x: T): Box<T> where T : Show := Box#Full(x)
impl Show for Int { def show(x: Int): Int := x }
def main(): Int := match wrap(3) { Box#Full(v) => show(v), Box#Empty => 0 }
";
        let uri = Url::parse("file:///t.sand").unwrap();
        let mut proj = Project::empty();
        proj.insert_file(uri.clone(), src.to_string()).unwrap();
        // The LSP uses the pre-monomorphisation program (`check_ide` → the
        // `Owned`-stage `typed`); use the same here so generic functions keep
        // their real signatures.
        let c = proj.check_to(lang::Stage::Owned);
        let ast = c.typed.as_ref().expect("typed program");
        let ctx = &c.ctx;
        let hover_at = |needle: &str| {
            hover_text(super::hover_at_position(
                position_of(src, needle),
                &uri,
                ctx,
                ast,
                &proj,
            ))
        };

        // Type name in a signature → the full ADT declaration.
        let ty = hover_at("Box<T>");
        assert!(ty.contains("type Box"), "type hover: {ty}");
        assert!(
            ty.contains("Empty") && ty.contains("Full"),
            "type hover: {ty}"
        );

        // Typeclass name in a `where` clause → the class declaration.
        let cls = hover_at("Show :=");
        assert!(cls.contains("typeclass Show"), "class hover: {cls}");

        // A constructor expression → its variant + the enum declaration.
        let ctor = hover_at("Box#Full(x)");
        assert!(ctor.contains("variant `Full`"), "ctor hover: {ctor}");

        // A *generic* function header → its real source signature with the
        // `where` clause intact (pre-mono — no `wrap$Int` specialisation).
        let f = hover_at("wrap<T>");
        assert!(
            f.contains("def wrap<") && f.contains("where") && f.contains(": Show"),
            "fn hover: {f}"
        );

        std::mem::forget(c);
    }
}
