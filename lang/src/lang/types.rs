/// Opaque index into the compiler's enum-definition registry.
/// Freely `Copy`, comparable, hashable — same pattern as `FunRef`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumRef(pub usize);

/// the actual (recursive, non-`Copy`) signature of a type.
///
/// interned into a registry on [`crate::compiler::context::CompileCtx`];
/// referred to elsewhere via the cheap, `Copy` [`Ty`] index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TyKind {
    Int,
    Bool,
    Unit,
    Top,    // the any type. used for error reporting when type inference fails
    Bottom, // the never type, for when an expression can never produce a value
    Enum(EnumRef),
}

/// opaque, interned index into the compiler's [`TyKind`] registry.
/// freely `Copy`, comparable, hashable — same pattern as [`EnumRef`].
///
/// the five primitive kinds are interned at fixed indices (see the
/// associated constants below) by
/// [`crate::compiler::context::CompileCtx::initial`], so they can be
/// constructed and compared without access to the context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ty(pub usize);

impl Ty {
    pub const INT: Ty = Ty(0);
    pub const BOOL: Ty = Ty(1);
    pub const UNIT: Ty = Ty(2);
    pub const TOP: Ty = Ty(3);
    pub const BOTTOM: Ty = Ty(4);

    /// true for types that are implicitly copied on use (Int, Bool, Unit).
    ///
    /// Enum types are *not* Copy, and are subject to move semantics.
    pub fn is_copy(&self) -> bool {
        matches!(*self, Ty::INT | Ty::BOOL | Ty::UNIT)
    }

    /// sound because interning is structural: equal `TyKind`s always
    /// produce the same `Ty` index, so equality of the underlying kinds
    /// (other than the Top/Bottom subtyping rules below) reduces to index
    /// equality.
    pub fn type_eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Ty::BOTTOM, Ty::BOTTOM) => true,
            (Ty::TOP, Ty::BOTTOM) | (Ty::BOTTOM, Ty::TOP) => false,
            (Ty::TOP, _) | (_, Ty::TOP) => true,
            (a, b) => a == b,
        }
    }

    pub fn type_neq(&self, other: &Self) -> bool {
        !self.type_eq(other)
    }
}

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Ty::INT => write!(f, "Int"),
            Ty::BOOL => write!(f, "Bool"),
            Ty::UNIT => write!(f, "Unit"),
            Ty::TOP => write!(f, "Top"),
            Ty::BOTTOM => write!(f, "Bottom"),
            Ty(n) => write!(f, "Ty({n})"),
        }
    }
}
