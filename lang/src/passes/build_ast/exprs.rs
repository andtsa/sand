#![allow(clippy::result_large_err)]

use pest::iterators::Pair;

use super::*;
use crate::compiler::context::CompileCtx;
use crate::compiler::structure::Range;
use crate::internal_bug;
use crate::passes::parse::Rule;

pub(crate) fn build_statement<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Statement<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::statement);
    // statement = ((declaration | assignment | expression) ~ ";")
    // capture pair span before moving
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let first = inner.next().missing("statement beginning", range)?;

    let inner_range = Range::from(&first);
    match first.as_rule() {
        Rule::declaration => {
            let mut decl_inner = first.into_inner();
            let first_child = decl_inner.next().missing("declaration body", inner_range)?;

            // Check for constructor-pattern binding: `let E#V(payload) = expr else
            // fallback`
            if first_child.as_rule() == Rule::let_constructor {
                let pattern = build_let_constructor(ctx, first_child)?;
                // Optional type annotation.
                let next = decl_inner
                    .next()
                    .missing("let_constructor declaration body", inner_range)?;
                let (ty, expr_pair) = if next.as_rule() == Rule::type_ {
                    let ty = build_type(ctx, next)?;
                    let ep = decl_inner
                        .next()
                        .missing("let_constructor declaration expression", inner_range)?;
                    (Some(ty), ep)
                } else {
                    (None, next)
                };
                let val = build_expr(ctx, expr_pair, src)?;
                // The `else` expression is mandatory for refutable patterns;
                // the type checker enforces this, here we just require it.
                let else_pair = decl_inner
                    .next()
                    .missing("let_constructor else expression", inner_range)?;
                let else_branch = build_expr(ctx, else_pair, src)?;
                return Ok(Statement::LetPattern {
                    pattern,
                    ty,
                    val,
                    else_branch,
                    range: inner_range,
                });
            }

            // Check for tuple-pattern binding: `let (a, mut b) = expr`
            if first_child.as_rule() == Rule::let_tuple {
                // Parse each element of the tuple pattern.
                let mut elems: Vec<(HirVar, bool, Range)> = Vec::new();
                for elem_pair in first_child.into_inner() {
                    // elem_pair matches `let_tuple_elem = { mut_kw? ~ identifier }`
                    let elem_range = Range::from(&elem_pair);
                    let mut elem_inner = elem_pair.into_inner();
                    let first_elem_child = elem_inner
                        .next()
                        .missing("let_tuple_elem body", elem_range)?;
                    let (is_mutable, ident_pair) = if first_elem_child.as_rule() == Rule::mut_kw {
                        (
                            true,
                            elem_inner
                                .next()
                                .missing("let_tuple_elem identifier", elem_range)?,
                        )
                    } else {
                        (false, first_elem_child)
                    };
                    // Register the element variable (using declaration context).
                    let var =
                        HirVar::Decl(ctx.new_original_variable(&ident_pair, Rule::declaration)?);
                    elems.push((var, is_mutable, elem_range));
                }
                // Optional type annotation, then the RHS expression.
                let next = decl_inner
                    .next()
                    .missing("let_tuple declaration body", inner_range)?;
                let (ty, expr_pair) = if next.as_rule() == Rule::type_ {
                    let ty = build_type(ctx, next)?;
                    let expr_pair = decl_inner
                        .next()
                        .missing("let_tuple declaration expression", inner_range)?;
                    (Some(ty), expr_pair)
                } else {
                    (None, next)
                };
                let expr = build_expr(ctx, expr_pair, src)?;
                return Ok(Statement::LetTuple {
                    elems,
                    ty,
                    val: expr,
                    range: inner_range,
                });
            }

            // Borrow binding `let &x : T = e` (shared) or `let &mut x : T = e`
            // (exclusive) (Calculus, the `Let` rules): desugar to `let x : &T = &e` /
            // `let x : &mut T = &mut e`, reusing the borrow-expression
            // machinery (`e` is borrowed, not consumed, and `x` holds the
            // reference). A `&mut` binding is assignable (`x = e` writes through
            // the borrow), so it is marked mutable.
            if first_child.as_rule() == Rule::borrow_binding {
                let mut bb_inner = first_child.into_inner().peekable();
                let mutable = if bb_inner.peek().map(|p| p.as_rule()) == Some(Rule::mut_kw) {
                    bb_inner.next();
                    true
                } else {
                    false
                };
                let name_pair = bb_inner
                    .next()
                    .missing("borrow binding name", inner_range)?;
                let var = HirVar::Decl(ctx.new_original_variable(&name_pair, Rule::declaration)?);
                let next = decl_inner
                    .next()
                    .missing("borrow declaration body", inner_range)?;
                let (ty, expr_pair) = if next.as_rule() == Rule::type_ {
                    let inner_ty = build_type(ctx, next)?;
                    let region = ctx.anon_region();
                    let ref_ty = if mutable {
                        ctx.ref_mut_ty(region, inner_ty)
                    } else {
                        ctx.ref_ty(region, inner_ty)
                    };
                    (
                        Some(ref_ty),
                        decl_inner
                            .next()
                            .missing("borrow declaration expression", inner_range)?,
                    )
                } else {
                    (None, next)
                };
                let inner_expr = build_expr(ctx, expr_pair, src)?;
                let expr_range = inner_expr.range;
                let borrowed = Expr {
                    expr: Expression::Borrow(Box::new(inner_expr), mutable),
                    range: expr_range,
                };
                return Ok(Statement::Declaration {
                    name: var,
                    range: inner_range,
                    ty,
                    is_mutable: mutable,
                    val: borrowed,
                });
            }

            // Regular single-binding declaration.
            let (is_mutable, name_pair) = if first_child.as_rule() == Rule::mut_kw {
                (
                    true,
                    decl_inner.next().missing("declaration name", inner_range)?,
                )
            } else {
                (false, first_child)
            };
            let var = HirVar::Decl(ctx.new_original_variable(&name_pair, Rule::declaration)?);
            tracing::trace!("declaration name: {}", name_pair.as_str());
            let next = decl_inner.next().missing("declaration body", inner_range)?;
            let (ty, expr_pair) = if next.as_rule() == Rule::type_ {
                let ty = build_type(ctx, next)?;
                let expr_pair = decl_inner
                    .next()
                    .missing("declaration expression", inner_range)?;
                (Some(ty), expr_pair)
            } else {
                (None, next)
            };
            let expr = build_expr(ctx, expr_pair, src)?;
            Ok(Statement::Declaration {
                name: var,
                range: inner_range,
                ty,
                is_mutable,
                val: expr,
            })
        }
        Rule::assignment => {
            let mut a_inner = first.into_inner();
            let target = a_inner.next().missing("assignment target", inner_range)?;
            match target.as_rule() {
                Rule::identifier => {
                    let name = target.as_str().to_string();
                    let expr = build_expr(
                        ctx,
                        a_inner.next().missing("assignment value", inner_range)?,
                        src,
                    )?;
                    Ok(Statement::Assignment {
                        name: HirVar::Unqualified(name),
                        range: inner_range,
                        val: expr,
                    })
                }
                // `*r = e`: write-through. The reference is the deref's inner.
                Rule::deref_expr => {
                    let ref_pair = target
                        .into_inner()
                        .next()
                        .missing("dereference target", inner_range)?;
                    let reference = build_primary(ctx, ref_pair, src)?;
                    let value = build_expr(
                        ctx,
                        a_inner.next().missing("assignment value", inner_range)?,
                        src,
                    )?;
                    Ok(Statement::DerefAssign {
                        reference,
                        value,
                        range: inner_range,
                    })
                }
                other => internal_bug!(
                    "assignment target was neither identifier nor deref_expr: {other:?}"
                ),
            }
        }
        Rule::expression => {
            let expr = build_expr(ctx, first, src)?;
            Ok(Statement::Expr(expr))
        }
        other => {
            // use the statement pair span for location
            Err(AstError::UnexpectedRule {
                expected: "declaration | assignment | expression",
                got: other,
                range: inner_range,
            })
        }
    }
}

// === expressions ===
// rule hierarchy: expression -> logic_or -> logic_xor -> logic_and -> equality
// -> comparison -> add_sub -> mul_div -> power -> unary -> primary

pub(crate) fn build_expr<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    match pair.as_rule() {
        Rule::expression => {
            // expression wraps logic_or
            let inner = pair.into_inner().next().missing("expression body", range)?;
            build_expr(ctx, inner, src)
        }
        Rule::logic_or => build_logic_or(ctx, pair, src),
        Rule::logic_xor => build_logic_xor(ctx, pair, src),
        Rule::logic_and => build_logic_and(ctx, pair, src),
        Rule::equality => build_equality(ctx, pair, src),
        Rule::comparison => build_comparison(ctx, pair, src),
        Rule::add_sub => build_add_sub(ctx, pair, src),
        Rule::mul_div => build_mul_div(ctx, pair, src),
        Rule::power => build_power(ctx, pair, src),
        Rule::unary => build_unary(ctx, pair, src),
        Rule::primary => build_primary(ctx, pair, src),
        Rule::lambda_expr => build_lambda(ctx, pair, src),
        other => Err(AstError::UnexpectedRule {
            expected: "expression-like rule",
            got: other,
            range,
        }),
    }
}

/// Build a lambda `fn (x: T) -> e`.
/// `lambda_expr = { "fn" ~ lambda_param ~ "->" ~ expression }`,
/// `lambda_param = { "(" ~ mut_kw? ~ identifier ~ ":" ~ type_ ~ ")" }`.
pub(crate) fn build_lambda<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::lambda_expr);
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();
    let param_pair = inner.next().missing("lambda parameter", range)?;
    let arrow = inner.next().missing("lambda arrow", range)?;
    let mode = build_fn_arrow(&arrow);
    let body_pair = inner.next().missing("lambda body", range)?;

    // lambda_param = { "(" ~ mut_kw? ~ identifier ~ ":" ~ type_ ~ ")" }
    let prange = Range::from(&param_pair);
    let mut pparts = param_pair.into_inner().peekable();
    let is_mutable = pparts.peek().map(|p| p.as_rule()) == Some(Rule::mut_kw);
    if is_mutable {
        pparts.next();
    }
    let name = pparts.next().missing("lambda parameter name", prange)?;
    let ty_pair = pparts.next().missing("lambda parameter type", prange)?;
    let ty = build_type(ctx, ty_pair)?;
    let var = HirVar::Decl(ctx.new_original_variable(&name, Rule::parameter)?);
    let param = Parameter {
        name: var,
        ty,
        range: prange,
        is_mutable,
    };

    let body = Box::new(build_expr(ctx, body_pair, src)?);
    Ok(Expr {
        expr: Expression::Lambda { param, body, mode },
        range,
    })
}

/// The lang-item name of the monadic bind that `do`-notation desugars to.
pub(crate) const BIND_FN: &str = "bind";

/// Desugar a block that uses do-notation (contains a top-level `<-`) into
/// nested `bind` calls. Called by [`build_block`] when a block has any
/// `monadic_bind` child; an ordinary block (no `<-`) never reaches here.
///
/// `{ x: T <- e; <rest> }` becomes `bind(e, fn (x: T) -> <rest>)`, applied
/// right-to-left so the *rest of the block* is the continuation. Ordinary
/// statements (`let`, ..) between binds are gathered into a `Block` that wraps
/// the continuation, and the trailing expression is the innermost result. The
/// result is plain HHIR (`Call`/`Lambda`/`Block`), so no downstream pass needs
/// to know do-notation ever existed.
///
/// A block using `<-` **must** end in a trailing expression (its monadic
/// result), otherwise there is nothing for a final bind to continue into.
///
/// **Error reporting.** Every synthesised node is given an actual source range:
/// the `bind` call and its continuation lambda point at the originating
/// `x <- e;` line, the lambda parameter points at the bound identifier, and `e`
/// / the trailing expression keep their own spans. So a type error in `e`, a
/// missing `Monad` instance, or a wrong continuation type all land on source
/// the user actually wrote. Node construction is funnelled through this one
/// function so a future "in this `<-` expansion" provenance note can be
/// attached in a single place. (For now the desugaring is mandatory-annotation
/// only; the `T` in `x: T <-` is what lets the continuation lambda type-check
/// without lambda-parameter inference.)
pub(crate) fn build_monadic_block<'run>(
    ctx: &mut CompileCtx<'run>,
    mut children: Vec<Pair<Rule>>,
    block_range: Range,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    // The final child must be the trailing expression (the monadic result).
    if children.last().map(|c| c.as_rule()) != Some(Rule::expression) {
        return Err(AstError::UnexpectedRule {
            expected: "trailing expression (a block using `<-` must end with its monadic result)",
            got: children
                .last()
                .map(|c| c.as_rule())
                .unwrap_or(Rule::monadic_bind),
            range: block_range,
        });
    }
    let tail_pair = children.pop().expect("checked non-empty above");
    let mut acc = build_expr(ctx, tail_pair, src)?;
    let items = children;

    // Walk the leading items (binds and statements) in reverse, folding each into
    // the accumulating continuation. Consecutive plain statements are buffered
    // (in reverse) and flushed into one `Block` when a bind or the start is hit.
    let mut pending: Vec<Statement<'run>> = Vec::new();
    let flush = |pending: &mut Vec<Statement<'run>>, acc: Expr<'run>| -> Expr<'run> {
        if pending.is_empty() {
            return acc;
        }
        pending.reverse();
        let stmts = std::mem::take(pending);
        let range = acc.range;
        Expr {
            expr: Expression::Block {
                statements: stmts,
                expr: Some(Box::new(acc)),
            },
            range,
        }
    };

    for item in items.into_iter().rev() {
        match item.as_rule() {
            Rule::monadic_bind => {
                // statements *after* this bind belong to the continuation body.
                acc = flush(&mut pending, acc);

                // monadic_bind = { identifier ~ ":" ~ type_ ~ "<-" ~ expression ~ ";" }
                let bind_range = Range::from(&item);
                let mut parts = item.into_inner();
                let name = parts.next().missing("do-bind variable", bind_range)?;
                let ty_pair = parts.next().missing("do-bind type", bind_range)?;
                let e_pair = parts.next().missing("do-bind expression", bind_range)?;

                let param_range = Range::from(&name);
                let ty = build_type(ctx, ty_pair)?;
                let var = HirVar::Decl(ctx.new_original_variable(&name, Rule::parameter)?);
                let param = Parameter {
                    name: var,
                    ty,
                    range: param_range,
                    is_mutable: false,
                };
                let bound = build_expr(ctx, e_pair, src)?;

                // bind(e, fn (x: T) -> <continuation>)
                let cont = Expr {
                    expr: Expression::Lambda {
                        param,
                        body: Box::new(acc),
                        mode: crate::lang::types::FnMode::Reusable,
                    },
                    range: bind_range,
                };
                acc = Expr {
                    expr: Expression::Call {
                        fn_name: HirFnCall::Local(BIND_FN.to_string()),
                        args: vec![bound, cont],
                        type_args: Vec::new(),
                    },
                    range: bind_range,
                };
            }
            Rule::statement => pending.push(build_statement(ctx, item, src)?),
            other => {
                return Err(AstError::UnexpectedRule {
                    expected: "monadic_bind | statement in do-block",
                    got: other,
                    range: Range::from(&item),
                });
            }
        }
    }
    Ok(flush(&mut pending, acc))
}

// generic left-assoc binary fold helper
pub(crate) fn binop_fold<'run, F>(
    ctx: &mut CompileCtx<'run>,
    mut inner: pest::iterators::Pairs<'_, Rule>,
    mut next_level: F,
    src: &str,
    parent_range: Range,
) -> Result<Expr<'run>, AstError>
where
    F: FnMut(&mut CompileCtx<'run>, Pair<Rule>, &str) -> Result<Expr<'run>, AstError>,
{
    let first_pair = inner.next().missing("left operand", parent_range)?;
    let mut expr = next_level(ctx, first_pair, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("right operand", parent_range)?;
        let rhs = next_level(ctx, rhs_pair, src)?;
        let op = bop_from_rule(op_pair.as_rule());

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range: parent_range,
        };
    }

    Ok(expr)
}

// Maps every left-associative binary operator token to its `Bop`, for the
// `binop_fold` precedence levels. (`pow` is right-associative and handled
// directly in `build_power`.)
pub(crate) fn bop_from_rule(rule: Rule) -> Bop {
    match rule {
        Rule::or => Bop::Or,
        Rule::xor => Bop::Xor,
        Rule::logand => Bop::And,
        Rule::bitand => Bop::BitAnd,
        Rule::eq => Bop::Comp(CompOp::Eq),
        Rule::ne => Bop::Comp(CompOp::Ne),
        Rule::gt => Bop::Comp(CompOp::Gt),
        Rule::lt => Bop::Comp(CompOp::Lt),
        Rule::ge => Bop::Comp(CompOp::Ge),
        Rule::le => Bop::Comp(CompOp::Le),
        Rule::add => Bop::Plus,
        Rule::subtract => Bop::Minus,
        Rule::multiply => Bop::Mult,
        Rule::divide => Bop::Div,
        _ => internal_bug!("unexpected bop_from_rule: {rule:?}"),
    }
}

// logic_or = { logic_xor ~ (or ~ logic_xor)* }
pub(crate) fn build_logic_or<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_logic_xor, src, range)
}

// logic_xor = { logic_and ~ (xor ~ logic_and)* }
pub(crate) fn build_logic_xor<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_logic_and, src, range)
}

// logic_and = { equality ~ (and ~ equality)* }
pub(crate) fn build_logic_and<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_equality, src, range)
}

// equality = { comparison ~ ( (eq | ne) ~ comparison )* }
pub(crate) fn build_equality<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    binop_fold(ctx, pair.into_inner(), build_comparison, src, range)
}

// comparison = { add_sub ~ ( (gt | lt | ge | le) ~ add_sub )* }
pub(crate) fn build_comparison<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    binop_fold(ctx, pair.into_inner(), build_add_sub, src, range)
}

// add_sub = { mul_div ~ ( (add | subtract) ~ mul_div )* }
pub(crate) fn build_add_sub<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    binop_fold(ctx, pair.into_inner(), build_mul_div, src, range)
}

// mul_div = { power ~ ( (multiply | divide) ~ power )* }
pub(crate) fn build_mul_div<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    binop_fold(ctx, pair.into_inner(), build_power, src, range)
}

// power = { unary ~ (pow ~ power)? }  -> right-assoc
pub(crate) fn build_power<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let left_pair = inner.next().missing("power expression", range)?;
    let left = build_unary(ctx, left_pair, src)?;

    if let Some(_op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("power right", range)?;
        let rhs = build_power(ctx, rhs_pair, src)?;
        Ok(Expr {
            expr: Expression::BinOp {
                left: Box::new(left),
                op: Bop::Pow,
                right: Box::new(rhs),
            },
            range,
        })
    } else {
        Ok(left)
    }
}

// unary = { (unary_operand ~ unary) | primary }
pub(crate) fn build_unary<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::unary);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let first = inner.next().missing("unary expr", range)?;

    match first.as_rule() {
        Rule::unary_operand => {
            let op_pair = first.into_inner().next().missing("unary operator", range)?;
            let rhs = build_unary(ctx, inner.next().missing("unary rhs", range)?, src)?;

            let op = match op_pair.as_rule() {
                Rule::subtract => Uop::Neg,
                Rule::negate => Uop::Not,
                other => {
                    return Err(AstError::UnexpectedRule {
                        expected: "subtract | negate",
                        got: other,
                        range: Range::from(&op_pair),
                    });
                }
            };

            Ok(Expr {
                expr: Expression::UnOp {
                    op,
                    right: Box::new(rhs),
                },
                range,
            })
        }
        Rule::subtract => {
            let rhs = build_unary(ctx, inner.next().missing("subtract rhs", range)?, src)?;
            Ok(Expr {
                expr: Expression::UnOp {
                    op: Uop::Neg,
                    right: Box::new(rhs),
                },
                range,
            })
        }
        Rule::negate => {
            let rhs = build_unary(ctx, inner.next().missing("negate rhs", range)?, src)?;
            Ok(Expr {
                expr: Expression::UnOp {
                    op: Uop::Not,
                    right: Box::new(rhs),
                },
                range,
            })
        }
        _ => build_primary(ctx, first, src),
    }
}

pub(crate) fn build_primary<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::primary);
    let range = Range::from(&pair);

    let s = pair.as_str();
    if s.starts_with('{') {
        let children: Vec<Pair<Rule>> = pair.into_inner().collect();

        // do-notation: a top-level `<-` anywhere in the block makes the whole
        // block monadic (desugars to nested `bind`); otherwise it is ordinary.
        if children.iter().any(|c| c.as_rule() == Rule::monadic_bind) {
            return build_monadic_block(ctx, children, range, src);
        }

        let mut statements = Vec::new();
        let mut expr: Option<Box<Expr>> = None;
        for inner in children {
            match inner.as_rule() {
                Rule::statement => statements.push(build_statement(ctx, inner, src)?),
                Rule::expression => expr = Some(Box::new(build_expr(ctx, inner, src)?)),
                other => {
                    return Err(AstError::UnexpectedRule {
                        expected: "statement | expression in block",
                        got: other,
                        range: Range::from(&inner),
                    });
                }
            }
        }

        return Ok(Expr {
            expr: Expression::Block { statements, expr },
            range,
        });
    }

    let inner = pair
        .into_inner()
        .next()
        .missing("inner expression", range)?;
    match inner.as_rule() {
        Rule::borrow_expr => {
            // borrow_expr = { "&" ~ mut_kw? ~ primary }
            let inner_range = Range::from(&inner);
            let mut parts = inner.into_inner().peekable();
            let mutable = if parts.peek().map(|p| p.as_rule()) == Some(Rule::mut_kw) {
                parts.next();
                true
            } else {
                false
            };
            let target = parts
                .next()
                .missing("borrow target expression", inner_range)?;
            let e = build_primary(ctx, target, src)?;
            Ok(Expr {
                expr: Expression::Borrow(Box::new(e), mutable),
                range,
            })
        }
        Rule::deref_expr => {
            // deref_expr = { "*" ~ primary }
            let inner_range = Range::from(&inner);
            let target = inner
                .into_inner()
                .next()
                .missing("dereference target expression", inner_range)?;
            let e = build_primary(ctx, target, src)?;
            Ok(Expr {
                expr: Expression::Deref(Box::new(e)),
                range,
            })
        }
        Rule::expression => build_expr(ctx, inner, src),
        Rule::ifstatement => build_if(ctx, inner, src),
        Rule::whileloop => build_while(ctx, inner, src),
        Rule::function_call | Rule::external_function_call => build_call(ctx, inner, src),
        Rule::external_constructor_expr => {
            // external_constructor_expr = { identifier ~ "::" ~ identifier ~ "#" ~
            // identifier ~ ("(" ~ expression ~ ")")? }
            let inner_range = Range::from(&inner);
            let mut parts = inner.into_inner();
            let mod_name = parts
                .next()
                .missing("module name in external constructor", inner_range)?
                .as_str()
                .to_string();
            let type_name = parts
                .next()
                .missing("type name in external constructor", inner_range)?
                .as_str()
                .to_string();
            let variant = parts
                .next()
                .missing("variant in external constructor", inner_range)?
                .as_str()
                .to_string();
            let payload = build_payload_expr(ctx, parts, src, inner_range)?;
            Ok(Expr {
                expr: Expression::ExternalConstructor {
                    mod_name,
                    type_name,
                    variant,
                    payload,
                },
                range: inner_range,
            })
        }
        Rule::constructor_expr => {
            // constructor_expr = { identifier ~ "#" ~ identifier ~ ("(" ~ expression ~
            // ")")? }
            let inner_range = Range::from(&inner);
            let mut parts = inner.into_inner();
            let type_name = parts
                .next()
                .missing("constructor type name", inner_range)?
                .as_str()
                .to_string();
            let variant = parts
                .next()
                .missing("constructor variant", inner_range)?
                .as_str()
                .to_string();
            let payload = build_payload_expr(ctx, parts, src, inner_range)?;
            Ok(Expr {
                expr: Expression::Constructor {
                    type_name,
                    variant,
                    payload,
                },
                range: inner_range,
            })
        }
        Rule::tuple_expr => {
            // tuple_expr = { "(" ~ expression ~ ("," ~ expression)+ ~ ")" }, arity >= 2
            let inner_range = Range::from(&inner);
            let elems = inner
                .into_inner()
                .map(|p| build_expr(ctx, p, src))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr {
                expr: Expression::Tuple(elems),
                range: inner_range,
            })
        }
        Rule::tag_expr => {
            // tag_expr = { "#" ~ identifier ~ ("(" ~ expression ~ ")")? }
            let inner_range = Range::from(&inner);
            let mut children = inner.into_inner();
            let variant = children
                .next()
                .missing("tag variant", inner_range)?
                .as_str()
                .to_string();
            // optional payload expression(s); more than one desugar to a tuple payload.
            let payload = build_payload_expr(ctx, children, src, inner_range)?;
            Ok(Expr {
                expr: Expression::Tag { variant, payload },
                range: inner_range,
            })
        }
        Rule::match_expr => build_match(ctx, inner, src),
        Rule::number => {
            let s = inner.as_str().to_string();
            let v = s.parse::<i64>().map_err(|e| AstError::InvalidInteger {
                got: s.clone(),
                range: Range::from(&inner),
                source: e,
            })?;

            Ok(Expr {
                expr: Expression::Int(v),
                range: Range::from(&inner),
            })
        }
        Rule::boolean => {
            let b = match inner.as_str() {
                "true" => true,
                "false" => false,
                other => internal_bug!("invalid boolean literal: {other}"),
            };

            Ok(Expr {
                expr: Expression::Bool(b),
                range: Range::from(&inner),
            })
        }
        Rule::identifier => Ok(Expr {
            expr: Expression::Var(HirVar::Unqualified(inner.as_str().to_string())),
            range: Range::from(&inner),
        }),
        other => Err(AstError::UnexpectedRule {
            expected: "primary inner",
            got: other,
            range: Range::from(&inner),
        }),
    }
}

pub(crate) fn build_if<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::ifstatement);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let cond_pair = inner.next().missing("if condition", range)?;
    let then_pair = inner.next().missing("then branch", range)?;
    let else_pair = inner.next();

    let cond = build_expr(ctx, cond_pair, src)?;
    let then_e = build_expr(ctx, then_pair, src)?;
    let else_e = match else_pair {
        Some(p) => Some(Box::new(build_expr(ctx, p, src)?)),
        None => None,
    };

    Ok(Expr {
        expr: Expression::If {
            cond: Box::new(cond),
            t: Box::new(then_e),
            f: else_e,
        },
        range,
    })
}

pub(crate) fn build_while<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::whileloop);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let cond_pair = inner.next().missing("while condition", range)?;
    let body_pair = inner.next().missing("while body", range)?;

    let cond = build_expr(ctx, cond_pair, src)?;
    let body = build_expr(ctx, body_pair, src)?;

    Ok(Expr {
        expr: Expression::While {
            cond: Box::new(cond),
            body: Box::new(body),
        },
        range,
    })
}

pub(crate) fn build_match<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::match_expr);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let scrutinee_pair = inner.next().missing("match scrutinee", range)?;
    let scrutinee = build_expr(ctx, scrutinee_pair, src)?;

    let mut arms = Vec::new();
    for arm_pair in inner {
        assert_eq!(arm_pair.as_rule(), Rule::match_arm);
        let arm_range = Range::from(&arm_pair);
        let mut arm_inner = arm_pair.into_inner();
        let pattern_pair = arm_inner.next().missing("match arm pattern", arm_range)?;
        let body_pair = arm_inner.next().missing("match arm body", arm_range)?;

        let pattern = build_pattern(ctx, pattern_pair)?;
        let body = build_expr(ctx, body_pair, src)?;
        arms.push(HirMatchArm {
            pattern,
            body,
            range: arm_range,
        });
    }

    Ok(Expr {
        expr: Expression::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        },
        range,
    })
}

/// Parse a `let_constructor` node (the outermost constructor in a `let E#V(...)
/// = ...`).
///
/// `let_constructor = { identifier ~ "#" ~ identifier ~ ("(" ~ let_destructure
/// ~ ")")? }`
pub(crate) fn build_let_constructor<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<HirPattern<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::let_constructor);
    let range = Range::from(&pair);
    let mut parts = pair.into_inner();
    let type_name = parts
        .next()
        .missing("let_constructor type name", range)?
        .as_str()
        .to_string();
    let variant = parts
        .next()
        .missing("let_constructor variant name", range)?
        .as_str()
        .to_string();
    // Multiple sub-patterns desugar to a single tuple sub-pattern:
    // `let Cons(x, rest) = …` ≡ `let Cons((x, rest)) = …`.
    let mut subs = parts
        .map(|p| build_let_destructure(ctx, p))
        .collect::<Result<Vec<_>, _>>()?;
    let payload = match subs.len() {
        0 => None,
        1 => Some(Box::new(subs.pop().unwrap())),
        _ => Some(Box::new(HirPattern::Tuple(subs))),
    };
    Ok(HirPattern::Constructor {
        type_name,
        variant,
        payload,
    })
}

/// Parse a `let_destructure` node: a sub-pattern inside a `let_constructor`.
///
/// `let_destructure = { let_constructor | let_binding_tuple | let_binding_elem
/// }` where `let_binding_elem = { identifier | empty_identifier }` so wildcards
/// (`_`) are allowed.
///
/// All bindings here are **immutable** (no `mut_kw` in sub-patterns).
pub(crate) fn build_let_destructure<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<HirPattern<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::let_destructure);
    let range = Range::from(&pair);
    let inner = pair
        .into_inner()
        .next()
        .missing("let_destructure body", range)?;
    match inner.as_rule() {
        Rule::let_constructor => build_let_constructor(ctx, inner),
        Rule::let_binding_tuple => {
            // let_binding_tuple = { "(" ~ let_binding_elem ~ ("," ~ let_binding_elem)+ ~
            // ")" }
            let elems = inner
                .into_inner()
                .map(|elem| {
                    // let_binding_elem = { identifier | empty_identifier }
                    let r = Range::from(&elem);
                    let child = elem
                        .into_inner()
                        .next()
                        .missing("let_binding_elem body", r)?;
                    match child.as_rule() {
                        Rule::identifier => {
                            let var =
                                HirVar::Decl(ctx.new_original_variable(&child, Rule::declaration)?);
                            Ok(HirPattern::Binding { var, range: r })
                        }
                        Rule::empty_identifier => Ok(HirPattern::Wildcard),
                        other => Err(AstError::UnexpectedRule {
                            expected: "identifier | empty_identifier",
                            got: other,
                            range: r,
                        }),
                    }
                })
                .collect::<Result<Vec<_>, AstError>>()?;
            Ok(HirPattern::Tuple(elems))
        }
        Rule::let_binding_elem => {
            // let_binding_elem = { identifier | empty_identifier }
            let child = inner
                .into_inner()
                .next()
                .missing("let_binding_elem body", range)?;
            match child.as_rule() {
                Rule::identifier => {
                    let var = HirVar::Decl(ctx.new_original_variable(&child, Rule::declaration)?);
                    Ok(HirPattern::Binding { var, range })
                }
                Rule::empty_identifier => Ok(HirPattern::Wildcard),
                other => Err(AstError::UnexpectedRule {
                    expected: "identifier | empty_identifier",
                    got: other,
                    range,
                }),
            }
        }
        other => Err(AstError::UnexpectedRule {
            expected: "let_constructor | let_binding_tuple | let_binding_elem",
            got: other,
            range,
        }),
    }
}

/// Build a constructor/tag payload from its (zero or more) argument
/// expressions. Multiple arguments desugar to a single tuple payload: `Ok(a,
/// b)` ≡ `Ok((a, b))`.
pub(crate) fn build_payload_expr<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    parts: impl Iterator<Item = Pair<'i, Rule>>,
    src: &str,
    range: Range,
) -> Result<Option<Box<Expr<'run>>>, AstError> {
    let mut exprs = parts
        .map(|p| build_expr(ctx, p, src))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match exprs.len() {
        0 => None,
        1 => Some(Box::new(exprs.pop().unwrap())),
        _ => Some(Box::new(Expr {
            expr: Expression::Tuple(exprs),
            range,
        })),
    })
}

/// Build a constructor/tag payload sub-pattern from its (zero or more) argument
/// patterns. Multiple arguments desugar to a single tuple sub-pattern:
/// `Cons(x, rest)` ≡ `Cons((x, rest))`.
pub(crate) fn build_payload_pattern<'i, 'run>(
    ctx: &mut CompileCtx<'run>,
    parts: impl Iterator<Item = Pair<'i, Rule>>,
) -> Result<Option<Box<HirPattern<'run>>>, AstError> {
    let mut pats = parts
        .map(|p| build_pattern(ctx, p))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match pats.len() {
        0 => None,
        1 => Some(Box::new(pats.pop().unwrap())),
        _ => Some(Box::new(HirPattern::Tuple(pats))),
    })
}

pub(crate) fn build_pattern<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<HirPattern<'run>, AstError> {
    assert_eq!(pair.as_rule(), Rule::pattern);
    let range = Range::from(&pair);
    let inner = pair.into_inner().next().missing("pattern body", range)?;
    match inner.as_rule() {
        Rule::constructor_pattern => {
            // constructor_pattern = { identifier ~ "#" ~ identifier ~ ("(" ~ pattern ~
            // ")")? }
            let mut parts = inner.into_inner();
            let type_name = parts
                .next()
                .missing("constructor type name", range)?
                .as_str()
                .to_string();
            let variant = parts
                .next()
                .missing("constructor variant name", range)?
                .as_str()
                .to_string();
            let payload = build_payload_pattern(ctx, parts)?;
            Ok(HirPattern::Constructor {
                type_name,
                variant,
                payload,
            })
        }
        Rule::tag_pattern => {
            // tag_pattern = { "#" ~ identifier ~ ("(" ~ pattern ~ ")")? }
            let mut parts = inner.into_inner();
            let variant = parts
                .next()
                .missing("tag pattern variant", range)?
                .as_str()
                .to_string();
            let payload = build_payload_pattern(ctx, parts)?;
            Ok(HirPattern::Tag { variant, payload })
        }
        Rule::tuple_pattern => {
            // tuple_pattern = { "(" ~ pattern ~ ("," ~ pattern)+ ~ ")" }
            let elems = inner
                .into_inner()
                .map(|p| build_pattern(ctx, p))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(HirPattern::Tuple(elems))
        }
        Rule::binding_pattern => {
            // binding_pattern = { identifier }
            let binding_range = Range::from(&inner);
            let name_pair = inner
                .into_inner()
                .next()
                .unwrap_or_else(|| unreachable!("binding_pattern always wraps an identifier"));
            let var = HirVar::Decl(ctx.new_original_variable(&name_pair, Rule::binding_pattern)?);
            Ok(HirPattern::Binding {
                var,
                range: binding_range,
            })
        }
        Rule::wildcard_pattern => Ok(HirPattern::Wildcard),
        Rule::int_literal_pattern => {
            let s = inner.as_str();
            let v = s.parse::<i64>().map_err(|e| AstError::InvalidInteger {
                got: s.to_string(),
                range,
                source: e,
            })?;
            Ok(HirPattern::IntLit(v))
        }
        Rule::bool_literal_pattern => {
            let b = match inner.as_str() {
                "true" => true,
                "false" => false,
                _ => unreachable!("bool_literal_pattern is 'true' | 'false'"),
            };
            Ok(HirPattern::BoolLit(b))
        }
        other => Err(AstError::UnexpectedRule {
            expected: "constructor_pattern | tag_pattern | tuple_pattern | wildcard_pattern | bool_literal_pattern | int_literal_pattern | binding_pattern",
            got: other,
            range,
        }),
    }
}

pub(crate) fn build_call<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr<'run>, AstError> {
    let rule = pair.as_rule();
    assert!(matches!(
        rule,
        Rule::function_call | Rule::external_function_call
    ));
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let ext_call = if rule == Rule::external_function_call {
        Some(inner.next().missing("function call module", range)?)
    } else {
        None
    };
    let name_pair = inner.next().missing("function call name", range)?;
    let name = name_pair.as_str().to_string();

    // optional turbofish `::<T, …>` (function_call only)
    let mut type_args = Vec::new();
    if inner.peek().map(|p| p.as_rule()) == Some(Rule::turbofish) {
        let tf = inner.next().missing("turbofish", range)?;
        for ty_pair in tf.into_inner() {
            type_args.push(build_type(ctx, ty_pair)?);
        }
    }

    let mut args = Vec::new();
    for expr_pair in inner {
        args.push(build_expr(ctx, expr_pair, src)?);
    }

    let fn_name = if let Some(mod_name) = ext_call {
        HirFnCall::External {
            module: mod_name.as_str().to_string(),
            name,
        }
    } else {
        HirFnCall::Local(name)
    };

    Ok(Expr {
        expr: Expression::Call {
            fn_name,
            args,
            type_args,
        },
        range,
    })
}
