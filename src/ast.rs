//! build an AST from the Pair tree
#![allow(clippy::result_large_err)]

use pest::Parser;
use pest::error::Error;
use pest::iterators::Pair;
use thiserror::Error;

use crate::lang::*;
use crate::parse::LangParser;
use crate::parse::Rule;
use crate::reserved::RESERVED_FUNCTION_NAMES;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error)]
pub enum AstError {
    #[error("parse error: {0}")]
    Pest(#[from] Error<Rule>),

    #[error("unexpected rule: expected {expected:?}, got {got:?}")]
    UnexpectedRule { expected: &'static str, got: Rule },

    #[error("missing {expected:?}")]
    Missing { expected: &'static str },

    #[error("invalid integer literal: {0}")]
    InvalidInteger(String),

    #[error("invalid function name: {0}")]
    InvalidFunctionName(String),
}

trait AstExt<T> {
    fn missing(self, expecting: &'static str) -> Result<T, AstError>
    where
        Self: Sized;
}

impl<T> AstExt<T> for Option<T> {
    fn missing(self, expecting: &'static str) -> Result<T, AstError>
    where
        Self: Sized,
    {
        match self {
            Some(v) => Ok(v),
            None => Err(AstError::Missing {
                expected: expecting,
            }),
        }
    }
}

impl Program {
    pub fn parse(src: &str) -> Result<Self, AstError> {
        let mut pairs = LangParser::parse(Rule::program, src)?;
        let program_pair = pairs.next().missing("root node")?;

        build_program(program_pair, src)
    }
}

// ============== top level ==============

fn build_program(pair: Pair<Rule>, src: &str) -> Result<Program, AstError> {
    assert_eq!(pair.as_rule(), Rule::program);

    let mut funcs = Vec::new();
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::function => {
                funcs.push(build_function(child, src)?);
            }
            // ignore start/end markers
            Rule::EOI => continue,
            other => {
                // debug
                let span = child.as_span();
                let (line, col) = span.start_pos().line_col();
                eprintln!(
                    "parse error: unexpected top-level rule at {}:{} - got {:?}",
                    line, col, other
                );
                return Err(AstError::UnexpectedRule {
                    expected: "function",
                    got: other,
                });
            }
        }
    }

    Ok(Program(funcs))
}

fn build_function(pair: Pair<Rule>, src: &str) -> Result<Function, AstError> {
    if pair.as_rule() != Rule::function {
        return Err(AstError::UnexpectedRule {
            expected: "function",
            got: pair.as_rule(),
        });
    }

    let mut inner = pair.into_inner();

    // order in grammar: identifier, (parameter | parameters)? , type_, expression
    // first child must be identifier
    let name_pair = inner.next().missing("function name")?;
    if name_pair.as_rule() != Rule::identifier {
        return Err(AstError::UnexpectedRule {
            expected: "identifier",
            got: name_pair.as_rule(),
        });
    }
    let name = name_pair.as_str().to_string();

    // make sure we aren't redefining internal functions
    if RESERVED_FUNCTION_NAMES.contains(&name.as_str()) {
        return Err(AstError::InvalidFunctionName(name));
    }

    // collect optional parameters (parameter or parameters)
    let mut parameters = Vec::new();
    loop {
        let peek = inner.peek().map(|p| p.as_rule());
        match peek {
            Some(Rule::parameter) => {
                let p = inner.next().missing("parameter")?;
                parameters.push(build_parameter(p)?);
            }
            Some(Rule::parameters) => {
                let p = inner.next().missing("parameter")?;
                for pp in p.into_inner() {
                    parameters.push(build_parameter(pp)?);
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
                });
            }
            p
        }
        None => {
            return Err(AstError::UnexpectedRule {
                expected: "type_",
                got: Rule::program,
            });
        }
    };
    let ret_type = build_type(ty_pair)?;

    // final child is the function body expression
    let body_pair = inner.next().missing("function body expression")?;
    let body = build_expr(body_pair, src)?;

    Ok(Function {
        name,
        parameters,
        ret_type,
        body,
    })
}

fn build_parameter(pair: Pair<Rule>) -> Result<Parameter, AstError> {
    assert_eq!(pair.as_rule(), Rule::parameter);
    let mut inner = pair.into_inner();
    let name = inner.next().missing("parameter name")?.as_str().to_string(); // identifier
    let ty = build_type(inner.next().missing("parameter type")?)?;
    Ok(Parameter { name, ty })
}

fn build_type(pair: Pair<Rule>) -> Result<Ty, AstError> {
    assert_eq!(pair.as_rule(), Rule::type_);
    match pair.as_str() {
        "Int" => Ok(Ty::Int),
        "Bool" => Ok(Ty::Bool),
        "Unit" => Ok(Ty::Unit),
        _ => Err(AstError::UnexpectedRule {
            expected: "type literal",
            got: pair.as_rule(),
        }),
    }
}

// === statements ===

fn build_statement(pair: Pair<Rule>, src: &str) -> Result<Statement, AstError> {
    assert_eq!(pair.as_rule(), Rule::statement);
    // statement = ((declaration | assignment | expression) ~ ";")
    let mut inner = pair.into_inner();
    let first = inner.next().missing("statement beginning")?;

    match first.as_rule() {
        Rule::declaration => {
            let mut decl_inner = first.into_inner();
            let name = decl_inner
                .next()
                .missing("declaration name")?
                .as_str()
                .to_string(); // identifier
            let ty = build_type(decl_inner.next().missing("declaration type")?)?;
            let expr = build_expr(decl_inner.next().missing("declaration expression")?, src)?;
            Ok(Statement::Declaration {
                name,
                ty,
                val: expr,
            })
        }
        Rule::assignment => {
            let mut a_inner = first.into_inner();
            let name = a_inner
                .next()
                .missing("assignment name")?
                .as_str()
                .to_string(); // identifier
            let expr = build_expr(a_inner.next().missing("assignment type")?, src)?;
            Ok(Statement::Assignment { name, val: expr })
        }
        Rule::expression => {
            let expr = build_expr(first, src)?;
            Ok(Statement::Expr(expr))
        }
        other => Err(AstError::UnexpectedRule {
            expected: "declaration | assignment | expression",
            got: other,
        }),
    }
}

// === expressions ===
// rule hierarchy: expression -> logic_or -> logic_xor -> logic_and -> equality
// -> comparison -> add_sub -> mul_div -> power -> unary -> primary

fn build_expr(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    match pair.as_rule() {
        Rule::expression => {
            // expression wraps logic_or
            let inner = pair.into_inner().next().missing("expression body")?;
            build_expr(inner, src)
        }
        Rule::logic_or => build_logic_or(pair, src),
        Rule::logic_xor => build_logic_xor(pair, src),
        Rule::logic_and => build_logic_and(pair, src),
        Rule::equality => build_equality(pair, src),
        Rule::comparison => build_comparison(pair, src),
        Rule::add_sub => build_add_sub(pair, src),
        Rule::mul_div => build_mul_div(pair, src),
        Rule::power => build_power(pair, src),
        Rule::unary => build_unary(pair, src),
        Rule::primary => build_primary(pair, src),
        other => Err(AstError::UnexpectedRule {
            expected: "expression-like rule",
            got: other,
        }),
    }
}

// generic left-assoc binary fold helper
fn binop_fold<F>(
    mut inner: pest::iterators::Pairs<'_, Rule>,
    mut next_level: F,
    src: &str,
) -> Result<Expr, AstError>
where
    F: FnMut(Pair<Rule>, &str) -> Result<Expr, AstError>,
{
    // first element is left operand
    let first_pair = inner.next().missing("left")?;
    let mut expr = next_level(first_pair, src)?;
    while let Some(op_pair) = inner.next() {
        // op_pair is an operator terminal like Rule::or / Rule::add etc.
        let rhs_pair = inner.next().missing("right")?;
        let rhs = next_level(rhs_pair, src)?;
        let op = bop_from_rule(op_pair.as_rule());
        let start = expr.start;
        let end = rhs.end;
        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            start,
            end,
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
        _ => panic!("unexpected bop_from_rule: {rule:?}"),
    }
}

// logic_or = { logic_xor ~ (or ~ logic_xor)* }
fn build_logic_or(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    let inner = pair.into_inner();
    binop_fold(inner, build_logic_xor, src)
}

// logic_xor = { logic_and ~ (xor ~ logic_and)* }
fn build_logic_xor(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    let inner = pair.into_inner();
    binop_fold(inner, build_logic_and, src)
}

// logic_and = { equality ~ (and ~ equality)* }
fn build_logic_and(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    let inner = pair.into_inner();
    binop_fold(inner, build_equality, src)
}

// equality = { comparison ~ ( (eq | ne) ~ comparison )* }
fn build_equality(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    let mut inner = pair.into_inner();
    let mut expr = build_comparison(inner.next().missing("eq expression")?, src)?;
    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("eq right")?;
        let rhs = build_comparison(rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::eq => Bop::Comp(CompOp::Eq),
            Rule::ne => Bop::Comp(CompOp::Ne),
            other => panic!("unexpected equality operator: {other:?}"),
        };
        let start = expr.start;
        let end = rhs.end;
        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            start,
            end,
        };
    }
    Ok(expr)
}

// comparison = { add_sub ~ ( (gt | lt | ge | le) ~ add_sub )* }
fn build_comparison(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    let mut inner = pair.into_inner();
    let mut expr = build_add_sub(inner.next().missing("comp expression")?, src)?;
    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("comp right")?;
        let rhs = build_add_sub(rhs_pair, src)?;
        let comp_op = match op_pair.as_rule() {
            Rule::gt => CompOp::Gt,
            Rule::lt => CompOp::Lt,
            Rule::ge => CompOp::Ge,
            Rule::le => CompOp::Le,
            other => panic!("unexpected comp operator: {other:?}"),
        };
        let start = expr.start;
        let end = rhs.end;
        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op: Bop::Comp(comp_op),
                right: Box::new(rhs),
            },
            start,
            end,
        };
    }
    Ok(expr)
}

// add_sub = { mul_div ~ ( (add | subtract) ~ mul_div )* }
fn build_add_sub(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    let mut inner = pair.into_inner();
    let mut expr = build_mul_div(inner.next().missing("add_sub expresison")?, src)?;
    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("add_sub right")?;
        let rhs = build_mul_div(rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::add => Bop::Plus,
            Rule::subtract => Bop::Minus,
            other => panic!("unexpected add_sub op: {other:?}"),
        };
        let start = expr.start;
        let end = rhs.end;
        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            start,
            end,
        };
    }
    Ok(expr)
}

// mul_div = { power ~ ( (multiply | divide) ~ power )* }
fn build_mul_div(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    let mut inner = pair.into_inner();
    let mut expr = build_power(inner.next().missing("mul_div expression")?, src)?;
    while let Some(op_pair) = inner.next() {
        let rhs_pair = inner.next().missing("mul_div right")?;
        let rhs = build_power(rhs_pair, src)?;
        let op = match op_pair.as_rule() {
            Rule::multiply => Bop::Mult,
            Rule::divide => Bop::Div,
            other => panic!("unexpected mul_div op: {other:?}"),
        };
        let start = expr.start;
        let end = rhs.end;
        expr = Expr {
            expr: Expression::BinOp {
                left: Box::new(expr),
                op,
                right: Box::new(rhs),
            },
            start,
            end,
        };
    }
    Ok(expr)
}

// power = { unary ~ (pow ~ power)? }  -> right-assoc
fn build_power(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    let mut inner = pair.into_inner();
    let left_pair = inner.next().missing("power expression")?;
    let left = build_unary(left_pair, src)?;
    if let Some(_op_pair) = inner.next() {
        // op_pair should be pow, next is power
        let rhs_pair = inner.next().missing("power right")?;
        let rhs = build_power(rhs_pair, src)?;
        let start = left.start;
        let end = rhs.end;
        Ok(Expr {
            expr: Expression::BinOp {
                left: Box::new(left),
                op: Bop::Pow,
                right: Box::new(rhs),
            },
            start,
            end,
        })
    } else {
        Ok(left)
    }
}

// unary = { (unary_operand ~ unary) | primary }
fn build_unary(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::unary);
    // capture span before we call `into_inner()` (which moves `pair`)
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let first = inner.next().missing("unary expr")?;

    match first.as_rule() {
        // case where pest returns the operator wrapped in unary_operand
        Rule::unary_operand => {
            // unary_operand contains a single child: subtract | negate
            let op_pair = first.into_inner().next().missing("unary operator")?;
            match op_pair.as_rule() {
                Rule::subtract => {
                    let rhs = build_unary(inner.next().missing("subtract rhs")?, src)?;
                    let (start_line, start_col) = span.start_pos().line_col();
                    let (end_line, end_col) = rhs.end;
                    Ok(Expr {
                        expr: Expression::UnOp {
                            op: Uop::Neg,
                            right: Box::new(rhs),
                        },
                        start: (start_line, start_col),
                        end: (end_line, end_col),
                    })
                }
                Rule::negate => {
                    let rhs = build_unary(inner.next().missing("negate rhs")?, src)?;
                    let (start_line, start_col) = span.start_pos().line_col();
                    let (end_line, end_col) = rhs.end;
                    Ok(Expr {
                        expr: Expression::UnOp {
                            op: Uop::Not,
                            right: Box::new(rhs),
                        },
                        start: (start_line, start_col),
                        end: (end_line, end_col),
                    })
                }
                other => Err(AstError::UnexpectedRule {
                    expected: "subtract | negate",
                    got: other,
                }),
            }
        }
        Rule::subtract => {
            let rhs = build_unary(inner.next().missing("subtract rhs")?, src)?;
            let (start_line, start_col) = span.start_pos().line_col();
            let (end_line, end_col) = rhs.end;
            Ok(Expr {
                expr: Expression::UnOp {
                    op: Uop::Neg,
                    right: Box::new(rhs),
                },
                start: (start_line, start_col),
                end: (end_line, end_col),
            })
        }
        Rule::negate => {
            let rhs = build_unary(inner.next().missing("negate rhs")?, src)?;
            let (start_line, start_col) = span.start_pos().line_col();
            let (end_line, end_col) = rhs.end;
            Ok(Expr {
                expr: Expression::UnOp {
                    op: Uop::Not,
                    right: Box::new(rhs),
                },
                start: (start_line, start_col),
                end: (end_line, end_col),
            })
        }

        // otherwise it's a primary/parenthesized expression
        _ => build_primary(first, src),
    }
}

fn build_primary(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::primary);

    // we need to distinguish between:
    // - parenthesized expression: "(" expression ")"
    // - braced block: "{" statement* expression? "}"
    // The pair.as_str() will start with '(' or '{' accordingly, so use that.
    // Capture the span before moving the pair with `into_inner()`.
    let span = pair.as_span();
    let s = pair.as_str();
    if s.starts_with('{') {
        // it's a braced block: collect statements and optional trailing expression
        let mut statements = Vec::new();
        let mut expr: Option<Box<Expr>> = None;
        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::statement => {
                    let st = build_statement(inner, src)?;
                    statements.push(st);
                }
                Rule::expression => {
                    // trailing expression (the final expression inside the block)
                    expr = Some(Box::new(build_expr(inner, src)?));
                }
                other => {
                    return Err(AstError::UnexpectedRule {
                        expected: "statement | expression in block",
                        got: other,
                    });
                }
            }
        }
        let (start_line, start_col) = span.start_pos().line_col();
        let (end_line, end_col) = span.end_pos().line_col();
        return Ok(Expr {
            expr: Expression::Block { statements, expr },
            start: (start_line, start_col),
            end: (end_line, end_col),
        });
    }

    // not a braced block: primary has a single inner
    let inner = pair.into_inner().next().missing("inner expression")?;
    match inner.as_rule() {
        Rule::expression => build_expr(inner, src), // parenthesized expression
        Rule::ifstatement => build_if(inner, src),
        Rule::whileloop => build_while(inner, src),
        Rule::function_call => build_call(inner, src),
        Rule::number => {
            let s = inner.as_str().to_string();
            let v = s
                .parse::<i64>()
                .map_err(|_| AstError::InvalidInteger(s.clone()))?;
            let span = inner.as_span();
            let (start_line, start_col) = span.start_pos().line_col();
            let (end_line, end_col) = span.end_pos().line_col();
            Ok(Expr {
                expr: Expression::Int(v),
                start: (start_line, start_col),
                end: (end_line, end_col),
            })
        }
        Rule::boolean => {
            let b = match inner.as_str() {
                "true" => true,
                "false" => false,
                other => panic!("invalid boolean literal: {other}"),
            };
            let span = inner.as_span();
            let (start_line, start_col) = span.start_pos().line_col();
            let (end_line, end_col) = span.end_pos().line_col();
            Ok(Expr {
                expr: Expression::Bool(b),
                start: (start_line, start_col),
                end: (end_line, end_col),
            })
        }
        Rule::identifier => {
            let span = inner.as_span();
            let (start_line, start_col) = span.start_pos().line_col();
            let (end_line, end_col) = span.end_pos().line_col();
            Ok(Expr {
                expr: Expression::Var(inner.as_str().to_string()),
                start: (start_line, start_col),
                end: (end_line, end_col),
            })
        }
        other => Err(AstError::UnexpectedRule {
            expected: "primary inner",
            got: other,
        }),
    }
}

fn build_if(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::ifstatement);
    // grammar: if ~ expression ~ then ~ expression ~ (else ~ expression)?
    // capture the span before moving `pair` with `into_inner()`
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let cond_pair = inner.next().missing("if condition")?;
    let then_pair = inner.next().missing("then branch")?;
    let else_pair = inner.next();

    let cond = build_expr(cond_pair, src)?;
    let then_e = build_expr(then_pair, src)?;
    let else_e = match else_pair {
        Some(p) => build_expr(p, src)?,
        None => {
            // use Unit expression with span from the if pair end
            let (end_line, end_col) = span.end_pos().line_col();
            Expr {
                expr: Expression::Unit,
                start: (end_line, end_col),
                end: (end_line, end_col),
            }
        }
    };

    let (start_line, start_col) = span.start_pos().line_col();
    let (end_line, end_col) = span.end_pos().line_col();

    Ok(Expr {
        expr: Expression::If {
            cond: Box::new(cond),
            t: Box::new(then_e),
            f: Box::new(else_e),
        },
        start: (start_line, start_col),
        end: (end_line, end_col),
    })
}

fn build_while(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::whileloop);
    // grammar: while ~ expression ~ do ~ expression
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let cond_pair = inner.next().missing("while condition")?;
    let body_pair = inner.next().missing("while body")?;

    let cond = build_expr(cond_pair, src)?;
    let body = build_expr(body_pair, src)?;
    let (start_line, start_col) = span.start_pos().line_col();
    let (end_line, end_col) = span.end_pos().line_col();

    Ok(Expr {
        expr: Expression::While {
            cond: Box::new(cond),
            body: Box::new(body),
        },
        start: (start_line, start_col),
        end: (end_line, end_col),
    })
}

fn build_call(pair: Pair<Rule>, src: &str) -> Result<Expr, AstError> {
    assert_eq!(pair.as_rule(), Rule::function_call);
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let name_pair = inner.next().missing("function call name")?;
    let name = name_pair.as_str().to_string();

    let mut args = Vec::new();
    for expr_pair in inner {
        args.push(build_expr(expr_pair, src)?);
    }

    let (start_line, start_col) = span.start_pos().line_col();
    let (end_line, end_col) = span.end_pos().line_col();

    Ok(Expr {
        expr: Expression::Call {
            fn_name: name,
            args,
        },
        start: (start_line, start_col),
        end: (end_line, end_col),
    })
}
