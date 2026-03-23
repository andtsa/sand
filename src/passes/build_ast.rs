//! build an AST from the Pair tree
#![allow(clippy::result_large_err)]

use pest::Parser;
use pest::error::Error;
use pest::iterators::Pair;
use thiserror::Error;

use crate::compiler::context::CompileCtx;
use crate::compiler::context::ContextError;
use crate::compiler::structure::FileRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::UriError;
use crate::internal_bug;
use crate::ir_types::hhir::*;
use crate::lang::intrinsics;
use crate::lang::ops::*;
use crate::lang::types::*;
use crate::passes::parse::LangParser;
use crate::passes::parse::Rule;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error)]
pub enum AstError {
    #[error("parse error: {0}")]
    Pest(#[from] Box<Error<Rule>>),

    #[error("unexpected rule: expected {expected:?}, got {got:?} at {range}")]
    UnexpectedRule {
        expected: &'static str,
        got: Rule,
        range: Range,
    },

    #[error("missing {expected:?} at {range}")]
    Missing {
        expected: &'static str,
        range: Range,
    },

    #[error("invalid integer literal: {got} at {range}")]
    InvalidInteger { got: String, range: Range },

    #[error("invalid name: {got} at {range}")]
    InvalidName { got: String, range: Range },

    #[error("context error: {0}")]
    ContextError(#[from] ContextError),

    #[error(transparent)]
    UriError(#[from] UriError),
}

trait AstExt<T> {
    /// If the Option is None, produce a `Missing` error located at
    /// `start..end`.
    fn missing(self, expecting: &'static str, range: Range) -> Result<T, AstError>
    where
        Self: Sized;
}

impl<T> AstExt<T> for Option<T> {
    fn missing(self, expecting: &'static str, range: Range) -> Result<T, AstError>
    where
        Self: Sized,
    {
        match self {
            Some(v) => Ok(v),
            None => Err(AstError::Missing {
                expected: expecting,
                range,
            }),
        }
    }
}

impl ProgramModule {
    pub fn parse_source_file<'run>(
        ctx: &mut CompileCtx<'run>,
        src: &str,
        file: FileRef,
    ) -> Result<Vec<Self>, AstError> {
        let mut pairs = LangParser::parse(Rule::program, src).map_err(Box::new)?;

        let program_pair = match pairs.next() {
            Some(p) => p,
            None => {
                return Err(AstError::Missing {
                    expected: "root node",
                    range: Range::new(1, 1, 1, 1),
                });
            }
        };

        let dm = ctx.default_module(file);

        let map = build_program(ctx, program_pair, src, dm, file)?;
        Ok(map
            .into_iter()
            .map(|(module_name, functions)| ProgramModule {
                functions,
                module_name,
            })
            .collect::<Vec<_>>())
    }

    pub fn parse_stub<'run>(ctx: &mut CompileCtx<'run>, src: &str) -> Result<Self, AstError> {
        let fr = ctx.dummy_file();
        let modules = Self::parse_source_file(ctx, src, fr)?;
        if modules.len() == 1 {
            Ok(modules.into_iter().next().unwrap())
        } else {
            Err(AstError::UnexpectedRule {
                expected: "exactly one module",
                got: Rule::program,
                range: Range::new(1, 1, 1, 1),
            })
        }
    }
}

// ============== top level ==============

pub fn build_program<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
    default_module: ModuleRef,
    file: FileRef,
) -> Result<Map<ModuleRef, Vec<Function>>, AstError> {
    assert_eq!(pair.as_rule(), Rule::program);

    // todo: discard unused modules from the ctx
    let mut mods = Map::new();
    let mut funcs = Vec::new();
    let mut current_module = default_module;
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::module => {
                // parse module name
                let child_span = child.as_span();
                let modname_pair = child
                    .into_inner()
                    .next()
                    .missing("module name", Range::from(child_span))?;
                let mod_span = modname_pair.as_span();
                if modname_pair.as_rule() != Rule::identifier {
                    return Err(AstError::UnexpectedRule {
                        expected: "identifier",
                        got: modname_pair.as_rule(),
                        range: Range::from(&modname_pair),
                    });
                }
                // if we have an existing module, save it before starting the new one
                if !funcs.is_empty() {
                    mods.insert(current_module, funcs);
                    funcs = Vec::new();
                }
                current_module = ctx.register_module(mod_span.as_str(), file);
            }
            Rule::function => {
                funcs.push(build_function(ctx, child, src, &current_module)?);
            }
            // ignore start/end markers
            Rule::EOI => continue,
            other => {
                // debug
                let range = Range::from(child);
                eprintln!("parse error: unexpected top-level rule at {range} - got {other:?}");
                return Err(AstError::UnexpectedRule {
                    expected: "function or module declaration",
                    got: other,
                    range,
                });
            }
        }
    }

    if !funcs.is_empty() {
        mods.insert(current_module, funcs);
    }

    Ok(mods)
}

fn build_function<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
    cur_module: &ModuleRef,
) -> Result<Function, AstError> {
    let range = Range::from(&pair);
    if pair.as_rule() != Rule::function {
        return Err(AstError::UnexpectedRule {
            expected: "function",
            got: pair.as_rule(),
            range,
        });
    }

    let mut inner = pair.into_inner();

    // order in grammar: identifier, (parameter | parameters)? , type_, expression
    // first child must be identifier
    let name_pair = inner.next().missing("function name", range)?;
    let name_range = Range::from(&name_pair);
    if name_pair.as_rule() != Rule::identifier {
        return Err(AstError::UnexpectedRule {
            expected: "identifier",
            got: name_pair.as_rule(),
            range: name_range,
        });
    }
    let name = name_pair.as_str().to_string();

    // make sure we aren't redefining internal functions
    if !intrinsics::fn_name_allowed(&name) {
        return Err(AstError::InvalidName {
            got: name,
            range: name_range,
        });
    }

    // collect optional parameters (parameter or parameters)
    let mut parameters = Vec::new();
    loop {
        let peek = inner.peek().map(|p| p.as_rule());
        match peek {
            Some(Rule::parameter) => {
                let p = inner.next().missing("parameter", range)?;
                for pp in p.into_inner() {
                    parameters.push(build_parameter(ctx, pp)?);
                }
            }
            Some(Rule::parameters) => {
                let p = inner.next().missing("parameter", range)?;
                for pp in p.into_inner() {
                    parameters.push(build_parameter(ctx, pp)?);
                }
            }
            _ => break,
        }
    }

    // next should be type_
    let ty_pair = match inner.next() {
        Some(p) => {
            if p.as_rule() != Rule::type_ {
                return Err(AstError::UnexpectedRule {
                    expected: "type_",
                    got: p.as_rule(),
                    range: Range::from(&p),
                });
            }
            p
        }
        None => {
            return Err(AstError::UnexpectedRule {
                expected: "type_",
                got: Rule::program,
                range,
            });
        }
    };
    let ret_type = build_type(ctx, ty_pair)?;

    // final child is the function body expression
    let body_pair = inner.next().missing("function body expression", range)?;
    let body = build_expr(ctx, body_pair, src)?;

    let ofref = ctx.register_function(&name_pair, cur_module)?;

    Ok(Function {
        name: ofref,
        range: Range::from(name_pair),
        parameters,
        ret_type,
        body,
    })
}

fn build_parameter<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
) -> Result<Parameter, AstError> {
    let rule = pair.as_rule();
    assert_eq!(rule, Rule::parameter);
    // capture span before into_inner
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();
    let name = inner.next().missing("parameter name", range)?; // identifier
    let ty_pair = inner.next().missing("parameter type", range)?;
    let ty = build_type(ctx, ty_pair)?;
    let var = HirVar::Decl(ctx.new_original_variable(&name, rule)?);
    Ok(Parameter {
        name: var,
        ty,
        range,
    })
}

fn build_type<'run>(_ctx: &mut CompileCtx<'run>, pair: Pair<Rule>) -> Result<Ty, AstError> {
    assert_eq!(pair.as_rule(), Rule::type_);
    match pair.as_str() {
        "Int" => Ok(Ty::Int),
        "Bool" => Ok(Ty::Bool),
        "Unit" => Ok(Ty::Unit),
        _ => Err(AstError::UnexpectedRule {
            expected: "type literal",
            got: pair.as_rule(),
            range: Range::from(pair),
        }),
    }
}

// === statements ===

fn build_statement<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Statement, AstError> {
    assert_eq!(pair.as_rule(), Rule::statement);
    // statement = ((declaration | assignment | expression) ~ ";")
    // capture pair span before moving
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let first = inner.next().missing("statement beginning", range)?;

    let inner_range = Range::from(&first);
    match first.as_rule() {
        d @ Rule::declaration => {
            let mut decl_inner = first.into_inner();
            let name_pair = decl_inner.next().missing("declaration name", inner_range)?;
            let var = HirVar::Decl(ctx.new_original_variable(&name_pair, d)?); // identifier
            let ty = build_type(
                ctx,
                decl_inner.next().missing("declaration type", inner_range)?,
            )?;
            let expr = build_expr(
                ctx,
                decl_inner
                    .next()
                    .missing("declaration expression", inner_range)?,
                src,
            )?;
            Ok(Statement::Declaration {
                name: var,
                range: inner_range,
                ty,
                val: expr,
            })
        }
        Rule::assignment => {
            let mut a_inner = first.into_inner();
            let name = a_inner
                .next()
                .missing("assignment name", inner_range)?
                .as_str()
                .to_string(); // identifier
            let expr = build_expr(
                ctx,
                a_inner.next().missing("assignment type", inner_range)?,
                src,
            )?;
            Ok(Statement::Assignment {
                name: HirVar::Unqualified(name),
                range: inner_range,
                val: expr,
            })
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

fn build_expr<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
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
        other => Err(AstError::UnexpectedRule {
            expected: "expression-like rule",
            got: other,
            range,
        }),
    }
}

// generic left-assoc binary fold helper
fn binop_fold<'run, F>(
    ctx: &mut CompileCtx<'run>,
    mut inner: pest::iterators::Pairs<'_, Rule>,
    mut next_level: F,
    src: &str,
    parent_range: Range,
) -> Result<Expr, AstError>
where
    F: FnMut(&mut CompileCtx<'run>, Pair<Rule>, &str) -> Result<Expr, AstError>,
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

// mapping only for token rules used in these folds (or/xor/and)
// other operators handled in their specific builders
fn bop_from_rule(rule: Rule) -> Bop {
    match rule {
        Rule::or => Bop::Or,
        Rule::xor => Bop::Xor,
        Rule::and => Bop::And,
        _ => internal_bug!("unexpected bop_from_rule: {rule:?}"),
    }
}

// logic_or = { logic_xor ~ (or ~ logic_xor)* }
fn build_logic_or<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_logic_xor, src, range)
}

// logic_xor = { logic_and ~ (xor ~ logic_and)* }
fn build_logic_xor<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_logic_and, src, range)
}

// logic_and = { equality ~ (and ~ equality)* }
fn build_logic_and<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let inner = pair.into_inner();
    binop_fold(ctx, inner, build_equality, src, range)
}

// equality = { comparison ~ ( (eq | ne) ~ comparison )* }
fn build_equality<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_comparison(ctx, inner.next().missing("eq expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("eq right", range)?;
        let rhs = build_comparison(ctx, rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::eq => Bop::Comp(CompOp::Eq),
            Rule::ne => Bop::Comp(CompOp::Ne),
            other => internal_bug!("unexpected equality operator: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// comparison = { add_sub ~ ( (gt | lt | ge | le) ~ add_sub )* }
fn build_comparison<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_add_sub(ctx, inner.next().missing("comp expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("comp right", range)?;
        let rhs = build_add_sub(ctx, rhs_pair, src)?;
        let comp_op = match op_pair.as_rule() {
            Rule::gt => CompOp::Gt,
            Rule::lt => CompOp::Lt,
            Rule::ge => CompOp::Ge,
            Rule::le => CompOp::Le,
            other => internal_bug!("unexpected comp operator: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op: Bop::Comp(comp_op),
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// add_sub = { mul_div ~ ( (add | subtract) ~ mul_div )* }
fn build_add_sub<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_mul_div(ctx, inner.next().missing("add_sub expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("add_sub right", range)?;
        let rhs = build_mul_div(ctx, rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::add => Bop::Plus,
            Rule::subtract => Bop::Minus,
            other => internal_bug!("unexpected add_sub op: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// mul_div = { power ~ ( (multiply | divide) ~ power )* }
fn build_mul_div<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    let range = Range::from(&pair);
    let mut inner = pair.into_inner();

    let mut expr = build_power(ctx, inner.next().missing("mul_div expression", range)?, src)?;

    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("mul_div right", range)?;
        let rhs = build_power(ctx, rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::multiply => Bop::Mult,
            Rule::divide => Bop::Div,
            other => internal_bug!("unexpected mul_div op: {other:?}"),
        };

        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            range,
        };
    }

    Ok(expr)
}

// power = { unary ~ (pow ~ power)? }  -> right-assoc
fn build_power<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
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
fn build_unary<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
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

fn build_primary<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::primary);
    let range = Range::from(&pair);

    let s = pair.as_str();
    if s.starts_with('{') {
        let mut statements = Vec::new();
        let mut expr: Option<Box<Expr>> = None;

        for inner in pair.into_inner() {
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
        Rule::expression => build_expr(ctx, inner, src),
        Rule::ifstatement => build_if(ctx, inner, src),
        Rule::whileloop => build_while(ctx, inner, src),
        Rule::function_call | Rule::external_function_call => build_call(ctx, inner, src),
        Rule::number => {
            let s = inner.as_str().to_string();
            let v = s.parse::<i64>().map_err(|_| AstError::InvalidInteger {
                got: s.clone(),
                range: Range::from(&inner),
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

fn build_if<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::ifstatement);
    let range = Range::from(&pair);

    let mut inner = pair.into_inner();
    let cond_pair = inner.next().missing("if condition", range)?;
    let then_pair = inner.next().missing("then branch", range)?;
    let else_pair = inner.next();

    let cond = build_expr(ctx, cond_pair, src)?;
    let then_e = build_expr(ctx, then_pair, src)?;
    let else_e = match else_pair {
        Some(p) => build_expr(ctx, p, src)?,
        None => Expr {
            expr: Expression::Unit,
            range,
        },
    };

    Ok(Expr {
        expr: Expression::If {
            cond: Box::new(cond),
            t: Box::new(then_e),
            f: Box::new(else_e),
        },
        range,
    })
}

fn build_while<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
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

fn build_call<'run>(
    ctx: &mut CompileCtx<'run>,
    pair: Pair<Rule>,
    src: &str,
) -> Result<Expr, AstError> {
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
        expr: Expression::Call { fn_name, args },
        range,
    })
}
