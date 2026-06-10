//! enum-type management

use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::lang::types::EnumRef;
use crate::lang::types::Ty;

/// a single variant of an enum: its name, and the type of value it carries
/// (if any). `payload: None` is a nullary tag (`Light#Red`); `payload:
/// Some(ty)` carries exactly one value of type `ty` — which may itself be a
/// `TyKind::Tuple` for multi-field constructors (`Pair#Both((Int, Bool))`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: String,
    pub payload: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: String,
    /// Variants in declaration order; a variant's index is its position.
    pub variants: Vec<EnumVariant>,
    pub range: Range,
    pub src_module: ModuleRef,
    pub index: EnumRef,
    /// `true` for ad-hoc tag-union types (`#ok | #err`); `false` for named
    /// enums declared with `type T = A | B | C`.
    /// Used to decide whether to print variants with a `#` prefix.
    pub is_anonymous: bool,
}
