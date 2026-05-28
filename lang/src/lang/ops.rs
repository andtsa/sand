//! operators

use crate::lang::types::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bop {
    Plus,
    Minus,
    Mult,
    Div,
    Pow,
    And,
    Or,
    Xor,
    Comp(CompOp),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompOp {
    Ge,
    Le,
    Eq,
    Ne,
    Gt,
    Lt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Uop {
    Neg,
    Not,
}

// specify what binary operations are allowed in this language.
impl Bop {
    /// returns the resulting type if the given types are accepted by this
    /// operator, and `Err(Ty)` with the expected type otherwise
    pub fn accepts_types(&self, left: Ty, right: Ty) -> Result<Ty, Ty> {
        use Bop::*;
        match self {
            Plus | Minus | Mult | Div | Pow => {
                if left == Ty::Int && right == Ty::Int {
                    Ok(Ty::Int)
                } else {
                    Err(Ty::Int)
                }
            }
            And | Or | Xor => {
                if left == right {
                    Ok(left) // both types are the same, so we can return either one
                } else {
                    Err(left) // could be either type, diagnostic is based on the first operand
                }
            }
            Comp(op) => {
                match op {
                    CompOp::Ge | CompOp::Le | CompOp::Gt | CompOp::Lt => {
                        if left == Ty::Int && right == Ty::Int {
                            Ok(Ty::Bool)
                        } else {
                            Err(Ty::Int)
                        }
                    }
                    CompOp::Eq | CompOp::Ne => {
                        if left == right {
                            Ok(Ty::Bool)
                        } else {
                            Err(left) // could be either type, diagnostic is based on the first operand
                        }
                    }
                }
            }
        }
    }
}

impl Uop {
    /// returns `Ok(Ty)` with the resulting type if the given type is accepted
    /// by this operator, and `Err(Ty)` with the expected type otherwise
    pub fn accepts_type(&self, right: Ty) -> Result<Ty, Ty> {
        use Uop::*;
        match self {
            Neg => {
                if right == Ty::Int {
                    Ok(Ty::Int)
                } else {
                    Err(Ty::Int)
                }
            }
            Not => {
                if right == Ty::Bool {
                    Ok(Ty::Bool)
                } else {
                    Err(Ty::Bool)
                }
            }
        }
    }
}
