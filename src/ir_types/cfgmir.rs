//! a CFG MIR

use crate::compiler::structure::FunRef;
use crate::compiler::structure::Map;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;
use crate::lang::types::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(pub usize);

#[derive(Debug, Clone)]
pub struct MirProgram {
    pub functions: Map<FunRef, MirFunction>,
}

#[derive(Debug, Clone)]
pub struct MirFunction {
    pub name: FunRef,
    pub range: Range,
    pub params: Vec<MirParam>,
    pub ret_type: Ty,

    pub locals: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
}

#[derive(Debug, Clone)]
pub struct MirParam {
    pub local: LocalId,
    pub name: UniqVar,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub enum LocalName {
    /// traceable back to source via CompileCtx
    User(UniqVar),
    /// index for uniqueness, hint for readability
    Temp(usize, &'static str),
}

#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub id: LocalId,
    pub name: LocalName,
    pub ty: Ty,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Assign {
        dst: Place,
        value: RValue,
        range: Range,
    },

    /// expression statements with side effects
    Eval { value: RValue, range: Range },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Goto {
        target: BlockId,
    },

    Branch {
        cond: Operand,
        then_bb: BlockId,
        else_bb: BlockId,
    },

    Return {
        value: Option<Operand>,
    },

    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    pub local: LocalId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Operand {
    Copy(Place),
    Const(Constant),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Constant {
    Int(i64),
    Bool(bool),
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RValue {
    Use(Operand),

    BinaryOp {
        op: Bop,
        left: Operand,
        right: Operand,
    },

    UnaryOp {
        op: Uop,
        right: Operand,
    },

    Call {
        fn_name: FunRef,
        args: Vec<Operand>,
    },

    IntrinsicCall {
        fn_name: Intrinsic,
        args: Vec<Operand>,
    },
}
