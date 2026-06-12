//! operators

use crate::lang::types::CommonTypes;
use crate::lang::types::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bop {
    Plus,
    Minus,
    Mult,
    Div,
    Pow,
    /// bitwise AND on `Int`, written `&&`.
    BitAnd,
    /// logical AND on `Bool`, written `and`.
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

impl Bop {
    /// Returns the result type if the given operand types are accepted by this
    /// operator, or `Err(expected_ty)` otherwise.
    pub fn accepts_types<'tcx>(
        &self,
        types: &CommonTypes<'tcx>,
        left: Ty<'tcx>,
        right: Ty<'tcx>,
    ) -> Result<Ty<'tcx>, Ty<'tcx>> {
        use Bop::*;
        match self {
            Plus | Minus | Mult | Div | Pow | BitAnd => {
                if left == types.int && right == types.int {
                    Ok(types.int)
                } else {
                    Err(types.int)
                }
            }
            And => {
                if left == types.bool && right == types.bool {
                    Ok(types.bool)
                } else {
                    Err(types.bool)
                }
            }
            Or | Xor => {
                if left == right {
                    Ok(left)
                } else {
                    Err(left)
                }
            }
            Comp(op) => match op {
                CompOp::Ge | CompOp::Le | CompOp::Gt | CompOp::Lt => {
                    if left == types.int && right == types.int {
                        Ok(types.bool)
                    } else {
                        Err(types.int)
                    }
                }
                CompOp::Eq | CompOp::Ne => {
                    if left == right {
                        Ok(types.bool)
                    } else {
                        Err(left)
                    }
                }
            },
        }
    }
}

impl Uop {
    /// Returns `Ok(result_ty)` if the given operand type is accepted by this
    /// operator, or `Err(expected_ty)` otherwise.
    pub fn accepts_type<'tcx>(
        &self,
        types: &CommonTypes<'tcx>,
        right: Ty<'tcx>,
    ) -> Result<Ty<'tcx>, Ty<'tcx>> {
        use Uop::*;
        match self {
            Neg => {
                if right == types.int {
                    Ok(types.int)
                } else {
                    Err(types.int)
                }
            }
            Not => {
                if right == types.bool {
                    Ok(types.bool)
                } else if right == types.int {
                    Ok(types.int)
                } else {
                    Err(types.bool)
                }
            }
        }
    }
}
