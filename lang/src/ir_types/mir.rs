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

    /// Drop the value held in `place` at scope exit (Calculus: Ownership and
    /// Drop). First-class so MIR passes can reorder / elide it and so the
    /// `Drop` typeclass can attach here. Lowers to `__drop_in_place`.
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
/// `[Deref]` place is a load through the pointer; writing one is a store
/// through it. A `[Field(i)]` step addresses the `i`-th field of the aggregate
/// reached so far (enum payload is index 1, tuple element `i` is index `i`),
/// without loading it, so `RValue::Ref(Place{ local, [Deref, Field(1)] })`
/// yields `&(*local).payload`. Field projections are produced only by a
/// borrowing `match` (destructuring through a shared reference).
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

    /// This place with one more projection step appended.
    pub fn project(&self, elem: ProjElem) -> Self {
        let mut projection = self.projection.clone();
        projection.push(elem);
        Place {
            local: self.local,
            projection,
        }
    }
}

/// A single step in a [`Place`] projection path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjElem {
    /// Dereference the reference held by the place so far (`*r`).
    Deref,
    /// Address the `i`-th field of the aggregate reached so far without loading
    /// it (enum payload is index 1, tuple element `i` is index `i`). The
    /// inverse (by value) of [`RValue::Field`]. Used to take an interior
    /// borrow of a field when destructuring through a shared reference.
    Field(usize),
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

    /// The byte size of a type (`size_of::<T>()`). Carries the
    /// (monomorphised, concrete) type; codegen emits the target-dependent LLVM
    /// size, the interpreters a layout-free approximation. The type must be
    /// carried here because, unlike the pointer ops, `size_of` has no value
    /// argument to recover it from.
    SizeOf(Ty<'tcx>),

    /// Address-of: a pointer to `place`'s storage (`&place` / `&mut place`).
    /// The inverse of a `[Deref]` projection. References are pointers, so
    /// this yields the address, not a copy.
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

    /// A closure value: a fat pointer `{ fn_ptr, env_ptr }` to the
    /// lifted top-level function `fn_name`. `env` holds the captured operands
    /// (empty / null env in the non-capturing milestone).
    Closure {
        fn_name: FunRef<'tcx>,
        env: Vec<Operand>,
    },

    /// Indirect call of a function value: call the closure `callee`
    /// (a `Fn`-typed operand) with `args`.
    CallIndirect {
        callee: Operand,
        args: Vec<Operand>,
    },
}
