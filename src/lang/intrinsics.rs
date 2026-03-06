//! intrinsics are functions that the compiler substitutes with non-language
//! machine code, in order to implement interactions with the OS

use std::fmt::Display;
use std::sync::LazyLock;

use crate::ir_types::hhir::Parameter;
use crate::lang::structure::FnName;
use crate::lang::structure::FnSig;
use crate::lang::structure::Map;
use crate::lang::types::Ty;

pub static INTRINSICS: LazyLock<Map<Intrinsic, (FnName, FnSig)>> = LazyLock::new(intrinsics);

pub const RESERVED_FUNCTION_NAMES: [&str; 6] =
    ["print", "println", "printf", "scanf", "read", "readline"];

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Intrinsic {
    Print,
    Println,
}

fn intrinsics() -> Map<Intrinsic, (FnName, FnSig)> {
    [
        (
            Intrinsic::Print,
            FnSig::with(
                &[Parameter {
                    name: "something".to_string(),
                    ty: Ty::Top,
                    range: Default::default(),
                }],
                Ty::Unit,
            ),
        ),
        (
            Intrinsic::Println,
            FnSig::with(
                &[Parameter {
                    name: "something".to_string(),
                    ty: Ty::Top,
                    range: Default::default(),
                }],
                Ty::Unit,
            ),
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
        }
    }
}

impl TryFrom<&str> for Intrinsic {
    type Error = ();
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "print" | "printf" => Ok(Intrinsic::Print),
            "println" => Ok(Intrinsic::Println),
            _ => Err(()),
        }
    }
}

pub fn fn_name_allowed(name: &str) -> bool {
    !RESERVED_FUNCTION_NAMES.contains(&name) && Intrinsic::try_from(name).is_err()
}
