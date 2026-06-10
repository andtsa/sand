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

/// A compile-time constant.
///
/// Enum discriminants are just `Int` — there is no separate `EnumVariant`
/// constant. In MIR, all enum values (including nullary variants) are
/// constructed via [`RValue::Aggregate`] with a discriminant integer as their
/// first field; match dispatch extracts that field with [`RValue::Field`] and
/// compares it as a plain integer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Constant {
    Int(i64),
    Bool(bool),
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RValue {
    Use(Operand),

    /// Build an aggregate (enum variant or tuple) from a flat list of field
    /// operands. The destination local's type (available on [`LocalDecl::ty`])
    /// determines what kind of aggregate is produced:
    ///
    /// - **enum variant** (`TyKind::Enum`): `fields[0]` is always
    ///   `Const(Int(variant_idx))` — the discriminant. For a payload-carrying
    ///   variant, `fields[1]` is the payload operand. Nullary variants have
    ///   only `fields[0]`.
    /// - **tuple** (`TyKind::Tuple`): `fields[i]` is the `i`-th element. No
    ///   discriminant.
    ///
    /// This encoding is the reason `Constant::EnumVariant` no longer exists:
    /// every aggregate — including nullary enum variants — is now built via
    /// `Aggregate` rather than represented as an `Operand::Const`. The
    /// uniformity lets every downstream consumer (LLVM codegen, interpreters,
    /// display) use a single code path for all aggregate types.
    Aggregate(Vec<Operand>),

    /// Extract field `index` from an aggregate value (read-only projection,
    /// the inverse of the corresponding `Aggregate` field).
    ///
    /// Encoding:
    /// - **enum**: index `0` → discriminant (`Int`); index `1` → payload.
    /// - **tuple**: index `i` → element `i`.
    ///
    /// This is deliberately a plain integer index rather than a typed
    /// `ProjectionKind` enum — the type system already encodes whether an
    /// aggregate is a tuple or an enum, so the index alone is sufficient and
    /// no separate kind tag is needed.
    Field {
        base: Operand,
        index: usize,
    },

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
