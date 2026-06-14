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
pub struct MirProgram<'tcx> {
    pub functions: Map<FunRef<'tcx>, MirFunction<'tcx>>,
}

#[derive(Debug, Clone)]
pub struct MirFunction<'tcx> {
    pub name: FunRef<'tcx>,
    pub range: Range,
    pub params: Vec<MirParam<'tcx>>,
    pub ret_type: Ty<'tcx>,

    pub locals: Vec<LocalDecl<'tcx>>,
    pub blocks: Vec<BasicBlock<'tcx>>,
    pub entry: BlockId,
}

#[derive(Debug, Clone)]
pub struct MirParam<'tcx> {
    pub local: LocalId,
    pub name: UniqVar<'tcx>,
    pub ty: Ty<'tcx>,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub enum LocalName<'tcx> {
    /// traceable back to source via CompileCtx
    User(UniqVar<'tcx>),
    /// index for uniqueness, hint for readability
    Temp(usize, &'static str),
}

#[derive(Debug, Clone)]
pub struct LocalDecl<'tcx> {
    pub id: LocalId,
    pub name: LocalName<'tcx>,
    pub ty: Ty<'tcx>,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub struct BasicBlock<'tcx> {
    pub id: BlockId,
    pub statements: Vec<Statement<'tcx>>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub enum Statement<'tcx> {
    Assign {
        dst: Place,
        value: RValue<'tcx>,
        range: Range,
    },

    /// expression statements with side effects
    Eval { value: RValue<'tcx>, range: Range },

    /// Drop the value held in `place` at scope exit (Memory Step B,
    /// Calculus §6.11). First-class so MIR passes can reorder / elide it and so
    /// the `Drop` typeclass can attach here. Lowers to `__drop_in_place` (a
    /// no-op until Step C gives types `release`).
    Drop { place: Place, range: Range },
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

/// A place: a local plus a (possibly empty) projection path. `projection` is
/// empty for a plain local (`x`); `[Deref]` denotes going *through* the
/// reference held in `local` (`*r`), the inverse of [`RValue::Ref`]. Reading a
/// `[Deref]` place is a load through the pointer; writing one (R3) is a store
/// through it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    pub local: LocalId,
    pub projection: Vec<ProjElem>,
}

impl Place {
    /// A bare local place (no projection).
    pub fn local(local: LocalId) -> Self {
        Place {
            local,
            projection: Vec::new(),
        }
    }

    /// A `*local` place: load/store through the reference held in `local`.
    pub fn deref(local: LocalId) -> Self {
        Place {
            local,
            projection: vec![ProjElem::Deref],
        }
    }
}

/// A single step in a [`Place`] projection path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjElem {
    /// Dereference the reference held by the place so far — `*r`.
    Deref,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Operand {
    Copy(Place),
    Const(Constant),
}

/// A compile-time constant.
///
/// Enum discriminants are just `Int`, there is no separate `EnumVariant`
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
pub enum RValue<'tcx> {
    Use(Operand),

    /// Address-of: a pointer to `place`'s storage — `&place` / `&mut place`
    /// (Calculus §3.2). The inverse of a `[Deref]` projection. With R2,
    /// references are real pointers, so this yields the address, not a copy.
    Ref(Place),

    /// Build an aggregate (enum variant or tuple) from a flat list of field
    /// operands. The destination local's type (available on [`LocalDecl::ty`])
    /// determines what kind of aggregate is produced:
    ///
    /// - **enum variant** (`TyKind::Enum`): `fields[0]` is always
    ///   `Const(Int(variant_idx))`. For a payload-carrying variant, `fields[1]`
    ///   is the payload operand. Nullary variants have only `fields[0]`.
    /// - **tuple** (`TyKind::Tuple`): `fields[i]` is the `i`-th element. No
    ///   discriminant.
    ///
    /// This encoding is the reason `Constant::EnumVariant` no longer exists:
    /// every aggregate, including nullary enum variants, is now built via
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
    /// `ProjectionKind` enum. the type system already encodes whether an
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
        fn_name: FunRef<'tcx>,
        args: Vec<Operand>,
    },

    IntrinsicCall {
        fn_name: Intrinsic,
        args: Vec<Operand>,
    },
}
