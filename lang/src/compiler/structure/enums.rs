//! enum-type management

use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::lang::types::EnumRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: String,
    /// Variant names in declaration order; the variant's index is its position.
    pub variants: Vec<String>,
    pub range: Range,
    pub src_module: ModuleRef,
    pub index: EnumRef,
    /// `true` for ad-hoc tag-union types (`#ok | #err`); `false` for named
    /// enums declared with `type T = A | B | C`.
    /// Used to decide whether to print variants with a `#` prefix.
    pub is_anonymous: bool,
}
