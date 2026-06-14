//! Intrinsics are compiler-known functions that map to non-language machine
//! code, implementing interactions with the OS.

use std::fmt::Display;
use std::sync::LazyLock;

use crate::compiler::structure::FnName;
use crate::compiler::structure::Map;
use crate::lang::types::CommonTypes;
use crate::lang::types::Ty;

pub static INTRINSICS: LazyLock<Map<Intrinsic, (FnName, IntrinsicSig)>> = LazyLock::new(intrinsics);

pub const RESERVED_FUNCTION_NAMES: [&str; 6] =
    ["print", "println", "printf", "scanf", "read", "readline"];

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Intrinsic {
    Print,
    Println,
    Abs,
    Min,
    Max,
    ReadInt,
    Exit,
    /// `__ptr_read(p: Ptr<T>): T` — load through a raw pointer (Memory Step A).
    /// Generic in the element type, so it has no entry in [`INTRINSICS`]; it is
    /// type-checked and lowered with bespoke rules using the element type
    /// recovered from its argument / result.
    PtrRead,
    /// `__ptr_write(p: Ptr<T>, v: T): Unit` — store through a raw pointer.
    PtrWrite,
    /// `__ptr_cast(p: Ptr<A>): Ptr<B>` — reinterpret a raw pointer (runtime
    /// no-op); `B` comes from the expected-type context.
    PtrCast,
    /// `__drop_in_place(x): Unit` — compiler-generated structural destructor
    /// glue (Memory Step A). A no-op for every type in Steps A/B (no type has a
    /// non-trivial destructor yet); it gains structural field recursion when
    /// `Heaped` types acquire `release` in Step C. Wired into drop insertion in
    /// Step B. Accepts any type (`Top` arg), returns `Unit`.
    DropInPlace,
    /// `size_of::<T>(): Int` — the byte size of `T` (Memory Step C). Generic
    /// via a turbofish *type* argument (no value args), so it is
    /// bespoke-typed; the concrete size is target-dependent, computed by
    /// codegen.
    SizeOf,
}

impl Intrinsic {
    /// Whether this intrinsic is a generic raw-pointer op (Memory Step A): it
    /// has no fixed [`IntrinsicSig`] and is handled by bespoke generic rules.
    pub fn is_ptr_op(self) -> bool {
        matches!(
            self,
            Intrinsic::PtrRead | Intrinsic::PtrWrite | Intrinsic::PtrCast
        )
    }

    /// Whether this intrinsic is generic in an explicit *type* argument
    /// (turbofish) rather than inferring from value args — only `size_of` so
    /// far. Such intrinsics are bespoke-typed and not in [`INTRINSICS`].
    pub fn is_type_arg_intrinsic(self) -> bool {
        matches!(self, Intrinsic::SizeOf)
    }
}

/// Lifetime-free type tag used in the [`INTRINSICS`] static. Resolve to a
/// concrete [`Ty<'tcx>`] via [`TyTag::to_ty`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TyTag {
    Int,
    Bool,
    Unit,
    Top,
}

impl TyTag {
    /// Resolve this tag to the concrete interned type from the given context.
    pub fn to_ty<'tcx>(self, types: &CommonTypes<'tcx>) -> Ty<'tcx> {
        match self {
            TyTag::Int => types.int,
            TyTag::Bool => types.bool,
            TyTag::Unit => types.unit,
            TyTag::Top => types.top,
        }
    }
}

/// The signature of an intrinsic function, stored in the [`INTRINSICS`] static.
/// Argument and return types are represented as [`TyTag`]s rather than
/// `Ty<'tcx>` so the static can be `'static`.
///
/// Use [`IntrinsicSig::resolve`] to obtain concrete `Ty<'tcx>` handles.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntrinsicSig {
    pub args: Vec<TyTag>,
    pub ret_ty: TyTag,
}

impl IntrinsicSig {
    /// Resolve the tag-based signature to concrete type handles.
    pub fn resolve<'tcx>(&self, types: &CommonTypes<'tcx>) -> (Vec<Ty<'tcx>>, Ty<'tcx>) {
        let args = self.args.iter().map(|t| t.to_ty(types)).collect();
        let ret_ty = self.ret_ty.to_ty(types);
        (args, ret_ty)
    }
}

fn intrinsics() -> Map<Intrinsic, (FnName, IntrinsicSig)> {
    [
        (
            Intrinsic::Print,
            IntrinsicSig {
                args: vec![TyTag::Top],
                ret_ty: TyTag::Unit,
            },
        ),
        (
            Intrinsic::Println,
            IntrinsicSig {
                args: vec![TyTag::Top],
                ret_ty: TyTag::Unit,
            },
        ),
        (
            Intrinsic::Abs,
            IntrinsicSig {
                args: vec![TyTag::Int],
                ret_ty: TyTag::Int,
            },
        ),
        (
            Intrinsic::Min,
            IntrinsicSig {
                args: vec![TyTag::Int, TyTag::Int],
                ret_ty: TyTag::Int,
            },
        ),
        (
            Intrinsic::Max,
            IntrinsicSig {
                args: vec![TyTag::Int, TyTag::Int],
                ret_ty: TyTag::Int,
            },
        ),
        (
            Intrinsic::ReadInt,
            IntrinsicSig {
                args: vec![],
                ret_ty: TyTag::Int,
            },
        ),
        (
            Intrinsic::Exit,
            IntrinsicSig {
                args: vec![TyTag::Int],
                ret_ty: TyTag::Unit,
            },
        ),
        (
            // `Top` arg = accepts a value of any type; no-op until Step C.
            Intrinsic::DropInPlace,
            IntrinsicSig {
                args: vec![TyTag::Top],
                ret_ty: TyTag::Unit,
            },
        ),
    ]
    .into_iter()
    .map(|(n, s)| (n, (FnName::from(n), s)))
    .collect()
}

impl Display for Intrinsic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Intrinsic::Print => write!(f, "print"),
            Intrinsic::Println => write!(f, "println"),
            Intrinsic::Abs => write!(f, "__abs"),
            Intrinsic::Min => write!(f, "__min"),
            Intrinsic::Max => write!(f, "__max"),
            Intrinsic::ReadInt => write!(f, "__read_int"),
            Intrinsic::Exit => write!(f, "__exit"),
            Intrinsic::PtrRead => write!(f, "__ptr_read"),
            Intrinsic::PtrWrite => write!(f, "__ptr_write"),
            Intrinsic::PtrCast => write!(f, "__ptr_cast"),
            Intrinsic::DropInPlace => write!(f, "__drop_in_place"),
            Intrinsic::SizeOf => write!(f, "size_of"),
        }
    }
}

impl TryFrom<&str> for Intrinsic {
    type Error = ();
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "print" | "printf" => Ok(Intrinsic::Print),
            "println" => Ok(Intrinsic::Println),
            "__abs" => Ok(Intrinsic::Abs),
            "__min" => Ok(Intrinsic::Min),
            "__max" => Ok(Intrinsic::Max),
            "__read_int" => Ok(Intrinsic::ReadInt),
            "__exit" => Ok(Intrinsic::Exit),
            "__ptr_read" => Ok(Intrinsic::PtrRead),
            "__ptr_write" => Ok(Intrinsic::PtrWrite),
            "__ptr_cast" => Ok(Intrinsic::PtrCast),
            "__drop_in_place" => Ok(Intrinsic::DropInPlace),
            "size_of" => Ok(Intrinsic::SizeOf),
            _ => Err(()),
        }
    }
}

pub fn fn_name_allowed(name: &str) -> bool {
    !RESERVED_FUNCTION_NAMES.contains(&name) && Intrinsic::try_from(name).is_err()
}

/// A layout-free approximation of a type's byte size, used by *both*
/// interpreters so they agree on `size_of` (Memory Step C). The interpreters
/// model the heap as a cell graph, not raw bytes, so the precise size is
/// irrelevant there (it is consumed by `malloc`, which mints a cell regardless)
/// — the real, target-dependent size is computed by codegen. Kept simple and
/// shared so HIR and MIR results match.
pub fn interp_size_of(ty: Ty<'_>) -> i64 {
    use crate::lang::types::TyKind;
    match ty.kind() {
        TyKind::Unit => 0,
        TyKind::Bool => 1,
        TyKind::Tuple(elems) => elems.iter().map(|e| interp_size_of(*e)).sum(),
        // Int, pointers/refs, and (boxed/handle) aggregates are word-sized here.
        _ => 8,
    }
}
