use std::fmt;

use crate::ir_types::hhir::*;
use crate::lang::ops::*;

impl fmt::Display for Expr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.expr)
    }
}

impl fmt::Display for Expression<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Int(n) => {
                write!(f, "{}", n)
            }
            Expression::Bool(b) => {
                write!(f, "{}", b)
            }
            Expression::Unit => {
                write!(f, "()")
            }
            Expression::Borrow(inner, mutable) => {
                write!(f, "&{}{}", if *mutable { "mut " } else { "" }, inner.expr)
            }
            Expression::Deref(inner) => {
                write!(f, "*{}", inner.expr)
            }
            Expression::Var(name) => {
                write!(f, "{:?}", name)
            }
            Expression::BinOp { left, op, right } => {
                write!(f, "({} {} {})", left.expr, op, right.expr)
            }
            Expression::UnOp { op, right } => {
                write!(f, "({}{})", op, right.expr)
            }
            Expression::If {
                cond,
                t,
                f: else_branch,
            } => match else_branch {
                Some(e) => write!(f, "(if {} then {} else {})", cond.expr, t.expr, e.expr),
                None => write!(f, "(if {} then {})", cond.expr, t.expr),
            },
            Expression::While { cond, body } => {
                write!(f, "(while {} do {})", cond.expr, body.expr)
            }
            Expression::Call { fn_name, args } => {
                write!(f, "{:?}(", fn_name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expression::Block { statements, expr } => {
                write!(f, "{{ ")?;
                for stmt in statements {
                    write!(f, "{};", stmt)?;
                }
                if let Some(e) = expr {
                    write!(f, " {}", e.expr)?;
                }
                write!(f, " }}")
            }
            Expression::Constructor {
                type_name,
                variant,
                payload,
            } => {
                write!(f, "{type_name}#{variant}")?;
                if let Some(p) = payload {
                    write!(f, "({})", p.expr)?;
                }
                Ok(())
            }
            Expression::ExternalConstructor {
                mod_name,
                type_name,
                variant,
                payload,
            } => {
                write!(f, "{mod_name}::{type_name}#{variant}")?;
                if let Some(p) = payload {
                    write!(f, "({})", p.expr)?;
                }
                Ok(())
            }
            Expression::Tag { variant, payload } => {
                write!(f, "#{variant}")?;
                if let Some(p) = payload {
                    write!(f, "({})", p.expr)?;
                }
                Ok(())
            }
            Expression::Match { scrutinee, arms } => {
                fn fmt_hir_pattern(
                    f: &mut std::fmt::Formatter<'_>,
                    pattern: &crate::ir_types::hhir::HirPattern,
                ) -> std::fmt::Result {
                    use crate::ir_types::hhir::HirPattern;
                    match pattern {
                        HirPattern::Constructor {
                            type_name,
                            variant,
                            payload,
                        } => {
                            write!(f, "{type_name}::{variant}")?;
                            if let Some(p) = payload {
                                write!(f, "(")?;
                                fmt_hir_pattern(f, p)?;
                                write!(f, ")")?;
                            }
                            Ok(())
                        }
                        HirPattern::Tag { variant, payload } => {
                            write!(f, "#{variant}")?;
                            if let Some(p) = payload {
                                write!(f, "(")?;
                                fmt_hir_pattern(f, p)?;
                                write!(f, ")")?;
                            }
                            Ok(())
                        }
                        HirPattern::Tuple(elems) => {
                            write!(f, "(")?;
                            for (i, e) in elems.iter().enumerate() {
                                if i > 0 {
                                    write!(f, ", ")?;
                                }
                                fmt_hir_pattern(f, e)?;
                            }
                            write!(f, ")")
                        }
                        HirPattern::IntLit(n) => write!(f, "{n}"),
                        HirPattern::BoolLit(b) => write!(f, "{b}"),
                        HirPattern::Binding { var, .. } => write!(f, "{var:?}"),
                        HirPattern::Wildcard => write!(f, "_"),
                    }
                }

                write!(f, "match {} {{ ", scrutinee.expr)?;
                for arm in arms {
                    fmt_hir_pattern(f, &arm.pattern)?;
                    write!(f, " => {}, ", arm.body.expr)?;
                }
                write!(f, "}}")
            }
            Expression::Tuple(elems) => {
                write!(f, "(")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", e.expr)?;
                }
                write!(f, ")")
            }
        }
    }
}

impl fmt::Display for Statement<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement::Declaration { name, ty, val, .. } => match ty {
                Some(ty) => write!(f, "let {:?}: {} = {}", name, ty, val.expr),
                None => write!(f, "let {:?} = {}", name, val.expr),
            },
            Statement::Assignment { name, val, .. } => {
                write!(f, "{:?} = {}", name, val.expr)
            }
            Statement::DerefAssign {
                reference, value, ..
            } => {
                write!(f, "*{} = {}", reference.expr, value.expr)
            }
            Statement::LetTuple { elems, ty, val, .. } => {
                let names: Vec<String> = elems
                    .iter()
                    .map(|(name, is_mutable, _)| {
                        if *is_mutable {
                            format!("mut {:?}", name)
                        } else {
                            format!("{:?}", name)
                        }
                    })
                    .collect();
                match ty {
                    Some(ty) => write!(f, "let ({}): {} = {}", names.join(", "), ty, val.expr),
                    None => write!(f, "let ({}) = {}", names.join(", "), val.expr),
                }
            }
            Statement::LetPattern {
                pattern,
                val,
                else_branch,
                ..
            } => {
                write!(
                    f,
                    "let {:?} = {} else {}",
                    pattern, val.expr, else_branch.expr
                )
            }
            Statement::Expr(expr) => {
                write!(f, "{}", expr.expr)
            }
        }
    }
}

impl fmt::Display for Bop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bop::Plus => write!(f, "+"),
            Bop::Minus => write!(f, "-"),
            Bop::Mult => write!(f, "*"),
            Bop::Div => write!(f, "/"),
            Bop::Pow => write!(f, "^"),
            Bop::BitAnd => write!(f, "&&"),
            Bop::And => write!(f, "and"),
            Bop::Or => write!(f, "|"),
            Bop::Xor => write!(f, "#"),
            Bop::Comp(op) => write!(f, "{}", op),
        }
    }
}

impl fmt::Display for CompOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompOp::Eq => write!(f, "=="),
            CompOp::Ne => write!(f, "!="),
            CompOp::Gt => write!(f, ">"),
            CompOp::Lt => write!(f, "<"),
            CompOp::Ge => write!(f, ">="),
            CompOp::Le => write!(f, "<="),
        }
    }
}

impl fmt::Display for Uop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Uop::Neg => write!(f, "-"),
            Uop::Not => write!(f, "!"),
        }
    }
}
