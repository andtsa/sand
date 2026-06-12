//! reconstruct the original program from the parsed & typed AST,
//! properly formatted.

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::ir_types::display::INDENT;
use crate::ir_types::display::MAX_LINE_LENGTH;
use crate::ir_types::display::typed_expr::FormatOpt;
use crate::ir_types::display::typed_expr::Indent;
use crate::ir_types::typed_hir::*;

macro_rules! push {
    ($f:ident, $($arg:tt)*) => {
        $f.push_str(format!($($arg)*).as_str())
    };
}

impl<'tcx> TypedProgram<'tcx> {
    /// print the original program from the AST,
    /// consistenly formatted and syntactically valid
    pub fn format(&self, ctx: &CompileCtx<'tcx>) -> Map<FileRef, String> {
        let mut formatted = Map::new();

        // group the functions by module and module by file.
        // only 1 module will be printed per file.
        let mut mod_contents = Map::new();
        for fr in self.functions.keys() {
            mod_contents
                .entry(ctx.original_fun(fr).module)
                .and_modify(|e: &mut Vec<FunRef>| e.push(*fr))
                .or_insert(vec![*fr]);
        }

        // we build the text only once per file,
        // so now we find which files contain which modules
        let mut files = Map::new();
        for mr in mod_contents.keys() {
            files
                .entry(ctx.file_of_module(*mr))
                .and_modify(|e: &mut Vec<ModuleRef>| e.push(*mr))
                .or_insert(vec![*mr]);
        }

        // now we can start building the formatted code per file
        for (file, modules) in files {
            // do not print the core file stub
            if ctx.is_core_module(file) {
                continue;
            }

            // todo: initialise the string builder with a gross estimate of the final size
            // based on the AST's allocation size
            let mut file_out = String::new();
            for mr in modules {
                format_module(&mut file_out, self, ctx, mr, &mod_contents[&mr]);
            }

            formatted.insert(file, file_out);
        }

        formatted
    }
}

// writer functions

fn format_module<'tcx>(
    f: &mut String,
    prog: &TypedProgram<'tcx>,
    ctx: &CompileCtx<'tcx>,
    mr: ModuleRef<'tcx>,
    funs: &[FunRef<'tcx>],
) {
    // print module heading
    push!(f, "module {};\n\n", ctx.module_info(&mr).name);

    // format all the functions
    for fr in funs {
        format_function(f, prog, ctx, fr);
    }
}

/// both the parameter list and the body can wrap if they don't fit on one line
fn format_function<'tcx>(
    f: &mut String,
    prog: &TypedProgram<'tcx>,
    ctx: &CompileCtx<'tcx>,
    fr: &FunRef<'tcx>,
) {
    let fun = &prog.functions[fr];

    let args: Vec<String> = fun
        .parameters
        .iter()
        .map(|p| format_parameter(ctx, p))
        .collect();

    push!(
        f,
        "def {}({}): {} := ",
        ctx.original_fun_name(*fr),
        args.join(", "),
        ctx.display_ty(fun.ret_type)
    );

    let mut indent_level: usize = 0;
    let mut line_len = f.lines().last().map_or(0, |l| l.len());
    let mut pending = FormatOpt::Nothing;

    for (token, opt) in fun.body.format(ctx) {
        if token.is_empty() {
            // sep tokens carry no text,
            // just update the pending spacing hint.
            //
            // this lets Sep(Newline(Decrease)) override a previous Newline(Same)
            // from a semicolon without emitting a blank line
            pending = opt;
            continue;
        }

        // apply the *previous* token's spacing hint before emitting this one
        match &pending {
            FormatOpt::Newline(indent) => {
                match indent {
                    Indent::Increase => indent_level += 1,
                    Indent::Decrease => indent_level = indent_level.saturating_sub(1),
                    Indent::Same => {}
                }
                f.push('\n');
                let pad = INDENT.repeat(indent_level);
                f.push_str(&pad);
                line_len = pad.len();
            }
            FormatOpt::Space | FormatOpt::Whitespace => {
                f.push(' ');
                line_len += 1;
            }
            FormatOpt::Any => {
                if line_len + token.len() > MAX_LINE_LENGTH {
                    f.push('\n');
                    let pad = INDENT.repeat(indent_level);
                    f.push_str(&pad);
                    line_len = pad.len();
                } /*else {
                    f.push(' ');
                    line_len += 1;
                }*/
            }
            FormatOpt::Nothing => {}
        }

        f.push_str(&token);
        line_len += token.len();
        pending = opt; // save for next iteration
    }

    f.push('\n');
}

fn format_parameter<'tcx>(ctx: &CompileCtx<'tcx>, param: &Parameter<'tcx>) -> String {
    format!(
        "{}: {}",
        ctx.uniq_variable_name(&param.name),
        ctx.display_ty(param.ty)
    )
}
