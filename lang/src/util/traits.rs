use std::fmt;

use crate::ir_types::hhir::*;
use crate::lang::ops::*;
use crate::lang::types::*;

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.expr)
    }
}

impl fmt::Display for Expression {
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
            Expression::Constructor { type_name, variant } => {
                write!(f, "{type_name}#{variant}")
            }
            Expression::ExternalConstructor {
                mod_name,
                type_name,
                variant,
            } => {
                write!(f, "{mod_name}::{type_name}#{variant}")
            }
            Expression::Tag { variant } => {
                write!(f, "#{variant}")
            }
        }
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement::Declaration { name, ty, val, .. } => match ty {
                Some(ty) => write!(f, "let {:?}: {} = {}", name, ty, val.expr),
                None => write!(f, "let {:?} = {}", name, val.expr),
            },
            Statement::Assignment { name, val, .. } => {
                write!(f, "{:?} = {}", name, val.expr)
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
            Bop::And => write!(f, "&"),
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

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "Int"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Unit => write!(f, "Unit"),
            Ty::Top => write!(f, "Top"),
            Ty::Bottom => write!(f, "Bottom"),
            Ty::Enum(er) => write!(f, "Enum({:?})", er),
        }
    }
}
