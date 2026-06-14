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

/// A heap-allocation *strategy* (Memory Step C): which allocator/ownership
/// discipline backs a heaped type. Only `Unique` exists in Step C; `Shared`
/// (Rc-like) arrives in Step E. This is the *strategy* axis, distinct from the
/// *capability* refinement (`HeapedUnique`/`HeapedShared`) named in source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapedStrategy {
    /// Box-like unique ownership (alloc / borrow / release, plus borrow_mut /
    /// take / reuse). The default unique allocator.
    Unique,
}

/// A property requested via a `deriving` clause (Memory Step C). This is a
/// *general* mechanism — `deriving` is not tied to `Heaped`. Today only the
/// heap-allocation capability is derivable; future variants slot in here
/// (`Eq`, `Clone`, `Ord`, `Display`, and `Custom(String)` for user-defined
/// derivations). A type may derive several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Derivable {
    /// `deriving HeapedUnique` (and, later, `HeapedShared`) — heap-allocate
    /// this type via the given strategy. A (mutually) recursive type must
    /// derive this; a non-recursive type may, to opt a large value onto the
    /// heap.
    Heaped(HeapedStrategy),
    // future: Eq, Clone, Ord, Display, Custom(String), …
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
    /// Properties this type derives via a `deriving` clause (Memory Step C),
    /// e.g. `Derivable::Heaped(Unique)`. A (mutually) recursive type must
    /// derive a heap strategy; a non-recursive type may (to opt a large
    /// value onto the heap) but need not.
    pub derives: Vec<Derivable>,
}

impl<'tcx> EnumDef<'tcx> {
    /// The heap strategy this type derives, if any. `Some` ⇒ values are a heap
    /// handle (the type is `Heaped`).
    // `find_map` reads as "find the Heaped derive"; clippy flags it as trivial
    // only because `Derivable` currently has a single variant — it is the right
    // shape once `Eq`/`Clone`/… join the enum.
    #[allow(clippy::unnecessary_find_map)]
    pub fn heaped_strategy(&self) -> Option<HeapedStrategy> {
        self.derives.iter().find_map(|d| match d {
            Derivable::Heaped(s) => Some(*s),
        })
    }
}

/// Safety: the only interior mutability is `EnumVariant::payload`, a `Cell`
/// written exactly once per variant during the single-threaded compilation
/// phase (`set_variant_payload`). After compilation the `EnumDef` is only ever
/// read, so sharing `&EnumDef` across threads (as the LSP does) is sound. This
/// mirrors the `unsafe impl Sync for Arenas` justification.
unsafe impl Sync for EnumDef<'_> {}
