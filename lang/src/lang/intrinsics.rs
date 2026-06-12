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
            _ => Err(()),
        }
    }
}

pub fn fn_name_allowed(name: &str) -> bool {
    !RESERVED_FUNCTION_NAMES.contains(&name) && Intrinsic::try_from(name).is_err()
}
