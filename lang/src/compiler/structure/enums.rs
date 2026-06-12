//! Enum-type management.

use std::cell::Cell;

use crate::compiler::structure::ModuleRef;
use crate::compiler::structure::Range;
use crate::compiler::structure::RegionParam;
use crate::compiler::structure::TypeParam;
use crate::lang::types::Ty;

/// A single variant of an enum: its name and the type of value it carries
/// (if any). `payload: None` is a nullary tag (`Light#Red`); `payload:
/// Some(ty)` carries exactly one value, which may itself be a
/// `TyKind::Tuple` for multi-field constructors (`Pair#Both((Int, Bool))`).
///
/// The payload is held in a [`Cell`] because enum registration is two-phase:
/// every `EnumDef` is allocated (immutably, into the arena) with all payloads
/// `None` so that forward/recursive references resolve, then the payload types
/// are filled in afterwards via [`Cell::set`]. The `Cell` is only mutated
/// during the single-threaded compilation phase (see the `unsafe impl Sync`
/// below).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant<'tcx> {
    pub name: String,
    pub payload: Cell<Option<Ty<'tcx>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef<'tcx> {
    pub name: String,
    /// Variants in declaration order; a variant's index is its position.
    pub variants: Vec<EnumVariant<'tcx>>,
    pub range: Range,
    pub src_module: ModuleRef<'tcx>,
    pub(crate) id: usize,
    /// Type parameters declared on the enum (`type Option<T> = ...`); empty for
    /// non-generic and anonymous enums.
    pub type_params: Vec<TypeParam>,
    /// Region parameters declared on the enum (`type Ref<'r, a> = ...`); empty
    /// for enums with no lifetime parameters and anonymous enums.
    pub region_params: Vec<RegionParam>,
    /// `true` for ad-hoc tag-union types (`#ok | #err`); `false` for named
    /// enums declared with `type T = A | B | C`.
    /// Used to decide whether to print variants with a `#` prefix.
    pub is_anonymous: bool,
}

/// Safety: the only interior mutability is `EnumVariant::payload`, a `Cell`
/// written exactly once per variant during the single-threaded compilation
/// phase (`set_variant_payload`). After compilation the `EnumDef` is only ever
/// read, so sharing `&EnumDef` across threads (as the LSP does) is sound. This
/// mirrors the `unsafe impl Sync for Arenas` justification.
unsafe impl Sync for EnumDef<'_> {}
