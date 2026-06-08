//! turn a typed [`Expr`] into a stream of strings
//! in between which any formatter can insert line breaks
//! and indentations

use crate::compiler::context::CompileCtx;
use crate::ir_types::typed_hir::*;

impl Expression {
    pub fn format<'fmt, 'run>(
        &'fmt self,
        ctx: &'fmt CompileCtx<'run>,
    ) -> TypedExprFormatter<'fmt, 'run>
    where
        'run: 'fmt,
    {
        TypedExprFormatter {
            stack: vec![Either::Exp(self)],
            ctx,
        }
    }

    pub fn fmt_inline(&self, ctx: &CompileCtx) -> String {
        let mut out = String::new();
        for (token, opt) in self.format(ctx) {
            out.push_str(&token);
            match opt {
                FormatOpt::Space | FormatOpt::Whitespace | FormatOpt::Any => out.push(' '),
                FormatOpt::Newline(_) => out.push(' '),
                FormatOpt::Nothing => {}
            }
        }
        out.trim_end().to_string()
    }
}

impl Expr {
    pub fn format<'fmt, 'run>(
        &'fmt self,
        ctx: &'fmt CompileCtx<'run>,
    ) -> TypedExprFormatter<'fmt, 'run>
    where
        'run: 'fmt,
    {
        self.expr.format(ctx)
    }

    pub fn fmt_inline(&self, ctx: &CompileCtx) -> String {
        self.expr.fmt_inline(ctx)
    }
}

enum Either<'fmt> {
    Exp(&'fmt Expression),
    Stm(&'fmt Statement),
    /// a pure spacing signal with no text; the consumer applies the
    /// [`FormatOpt`] and emits nothing for the token itself. used to inject
    /// spacing between sub-expressions whose last/first tokens can't know
    /// their own context.
    Sep(FormatOpt),
    Token(String, FormatOpt),
}

/// what the consumer of [`TypedExprFormatter`] is expected to put *after* this
/// token.
///
/// Example — given:
/// ```sand
/// i = 1;
/// if x then f(x) else g()
/// ```
/// the iterator yields:
/// ```ignore
/// ("i", Space), ("=", Space), ("1", Nothing), (";", Newline(Same)),
/// ("if", Whitespace), ("x", Whitespace), ("then", Whitespace),
/// ("f", Nothing), ("(", Any), ("x", Any), (")", Whitespace),
/// ("else", Whitespace), ("g", Nothing), ("(", Nothing), (")", Nothing)
/// ```
pub enum FormatOpt {
    Newline(Indent),
    /// some spacing is recommended
    Space,
    /// some whitespace character is **required**
    Whitespace,
    /// no space is recommended
    Nothing,
    /// no space is required but a line break may be freely inserted
    Any,
}

pub enum Indent {
    Increase,
    Same,
    Decrease,
}

/// an iterator over the expression's tokens,
/// each paired with a [`FormatOpt`] hinting what the consumer should place
/// after it.
pub struct TypedExprFormatter<'fmt, 'run> {
    stack: Vec<Either<'fmt>>,
    ctx: &'fmt CompileCtx<'run>,
}

impl<'fmt, 'run> Iterator for TypedExprFormatter<'fmt, 'run> {
    type Item = (String, FormatOpt);

    fn next(&mut self) -> Option<Self::Item> {
        use Either::*;
        use Expression::*;
        use FormatOpt::*;
        use Indent::*;
        use Statement as Stmt;

        // loop so that arms that only push onto the stack (compound statements,
        // BinOp, etc.) don't have to recurse (they just `continue`)
        loop {
            match self.stack.pop()? {
                Token(s, f) => return Some((s, f)),
                Sep(f) => return Some((String::new(), f)),

                Stm(stmt) => match stmt {
                    Stmt::Declaration { name, ty, val, .. } => {
                        // emission order: let name : ty = val ;
                        // push in reverse (LIFO):
                        self.stack.push(Token(";".into(), Newline(Same)));
                        self.stack.push(Exp(&val.expr));
                        self.stack.push(Token("=".into(), Space));
                        self.stack
                            .push(Token(self.ctx.display_ty(*ty).to_string(), Space));
                        self.stack.push(Token(":".into(), Space));
                        // name carries Nothing so there is no space before ":"
                        self.stack
                            .push(Token(self.ctx.uniq_variable_name(name), Nothing));
                        self.stack.push(Sep(Whitespace));
                        self.stack.push(Token("let".into(), Whitespace));
                        continue;
                    }
                    Stmt::Assignment { name, val, .. } => {
                        // emission order: name = val ;
                        self.stack.push(Token(";".into(), Newline(Same)));
                        self.stack.push(Exp(&val.expr));
                        self.stack.push(Token("=".into(), Space));
                        self.stack
                            .push(Token(self.ctx.uniq_variable_name(name), Space));
                        continue;
                    }
                    Stmt::Expr(e) => {
                        // a bare expression used as a statement still ends with ";"
                        self.stack.push(Token(";".into(), Newline(Same)));
                        self.stack.push(Exp(&e.expr));
                        continue;
                    }
                },

                Exp(expr) => match &expr {
                    // --- terminals ---
                    Unit => return Some(("()".into(), Nothing)),
                    Bool(x) => return Some((x.to_string(), Nothing)),
                    Int(x) => return Some((x.to_string(), Nothing)),
                    Var(x) => return Some((self.ctx.uniq_variable_name(x), Nothing)),

                    // --- compound expressions ---
                    If { cond, t, f } => {
                        // emission order: if cond then t else f
                        self.stack.push(Exp(&f.expr));
                        self.stack.push(Token("else".into(), Whitespace));
                        self.stack.push(Sep(Whitespace)); // space after t's last token
                        self.stack.push(Exp(&t.expr));
                        self.stack.push(Token("then".into(), Whitespace));
                        self.stack.push(Sep(Whitespace)); // space after cond's last token
                        self.stack.push(Exp(&cond.expr));
                        self.stack.push(Sep(Whitespace));
                        return Some(("if".into(), Whitespace));
                    }

                    While { cond, body } => {
                        // emission order: while cond do body
                        self.stack.push(Exp(&body.expr));
                        self.stack.push(Token("do".into(), Whitespace));
                        self.stack.push(Sep(Whitespace)); // space after cond's last token
                        self.stack.push(Exp(&cond.expr));
                        return Some(("while".into(), Whitespace));
                    }

                    BinOp { left, op, right } => {
                        // emission order: left op right
                        // Sep(Space) injects the space before the operator that the
                        // left sub-expression's last token can't know to add itself.
                        self.stack.push(Exp(&right.expr));
                        self.stack.push(Token(op.to_string(), Space));
                        self.stack.push(Sep(Space)); // space before op
                        self.stack.push(Exp(&left.expr));
                        continue;
                    }

                    UnOp { op, right } => {
                        // emission order: op right  (no space, e.g. "!x", "-1")
                        self.stack.push(Exp(&right.expr));
                        return Some((op.to_string(), Nothing));
                    }

                    Call { fn_name, args } => {
                        // emission order: name ( arg, arg, ... )
                        self.stack.push(Token(")".into(), Nothing));
                        // push args in reverse; insert a ", " separator between them
                        for (i, arg) in args.iter().enumerate().rev() {
                            self.stack.push(Exp(&arg.expr));
                            if i > 0 {
                                self.stack.push(Token(",".into(), Space));
                            }
                        }
                        self.stack.push(Token("(".into(), Any));
                        return Some((self.ctx.original_fun_name(*fn_name), Nothing));
                    }

                    IntrinsicCall { fn_name, args } => {
                        self.stack.push(Token(")".into(), Nothing));
                        for (i, arg) in args.iter().enumerate().rev() {
                            self.stack.push(Exp(&arg.expr));
                            if i > 0 {
                                self.stack.push(Token(",".into(), Space));
                            }
                        }
                        self.stack.push(Token("(".into(), Any));
                        return Some((fn_name.to_string(), Nothing));
                    }

                    Block { statements, expr } => {
                        // emission order: { [newline+indent] stmts... [tail_expr] [dedent] }
                        //
                        // note for the consumer: if no tail_expr, the last statement already
                        // emits Newline(Same), followed immediately by Sep(Newline(Decrease)).
                        // the consumer should treat a Newline(Decrease) that follows a Newline(*)
                        // as "just change the indent level" rather than emitting a second blank
                        // line.
                        self.stack.push(Token("}".into(), Nothing));
                        self.stack.push(Sep(Newline(Decrease)));
                        if let Some(tail) = expr {
                            self.stack.push(Exp(&tail.expr));
                        }
                        for stmt in statements.iter().rev() {
                            self.stack.push(Stm(stmt));
                        }
                        return Some(("{".into(), Newline(Increase)));
                    }

                    Constructor {
                        enum_ref,
                        variant_idx,
                    } => {
                        return Some((self.ctx.enum_display(*enum_ref, *variant_idx), Nothing));
                    }

                    Match { scrutinee, arms } => {
                        // emission order: match scrutinee { arm1 arm2 ... }
                        self.stack.push(Token("}".into(), Nothing));
                        self.stack.push(Sep(Newline(Decrease)));
                        // push arms in reverse order
                        for arm in arms.iter().rev() {
                            let pattern_str = match &arm.pattern {
                                crate::ir_types::typed_hir::MatchPattern::Variant {
                                    enum_ref,
                                    variant_idx,
                                } => self.ctx.enum_display(*enum_ref, *variant_idx),
                                crate::ir_types::typed_hir::MatchPattern::Wildcard => {
                                    "_".to_string()
                                }
                            };
                            self.stack.push(Token(",".into(), Newline(Same)));
                            self.stack.push(Exp(&arm.body.expr));
                            self.stack.push(Token("=>".into(), Whitespace));
                            self.stack.push(Sep(Whitespace));
                            self.stack.push(Token(pattern_str, Nothing));
                        }
                        self.stack.push(Exp(&scrutinee.expr));
                        self.stack.push(Sep(Whitespace));
                        return Some(("match".into(), Whitespace));
                    }
                },
            }
        }
    }
}
