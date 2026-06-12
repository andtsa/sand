use std::cmp::Ordering;
use std::fmt;
use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::EnumDef;

/// A globally unique identifier for a type parameter (the `T` in
/// `def f<T>(...)` or `type Option<T> = ...`). Assigned once per declared
/// parameter; two parameters named `T` in different declarations get distinct
/// ids, so `TyKind::Param` comparison is unambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeParamId(pub usize);

/// A `Copy` handle to an arena-allocated [`EnumDef`].
///
/// Equality and hashing are by pointer identity: each distinct enum (named
/// enums deduplicated by name, anonymous tag-unions by tag set) is allocated
/// exactly once, so identical enum ↔ identical pointer. Ordering is by the
/// monotonic registration `id` for deterministic iteration.
#[derive(Copy, Clone)]
pub struct EnumRef<'tcx>(pub(crate) &'tcx EnumDef<'tcx>);

impl<'tcx> EnumRef<'tcx> {
    /// Access the underlying enum definition.
    #[inline]
    pub fn def(self) -> &'tcx EnumDef<'tcx> {
        self.0
    }
}

impl PartialEq for EnumRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}
impl Eq for EnumRef<'_> {}
impl Hash for EnumRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as *const EnumDef<'_>).hash(state);
    }
}
impl PartialOrd for EnumRef<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for EnumRef<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.id.cmp(&other.0.id)
    }
}
impl fmt::Debug for EnumRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EnumRef({}, {})", self.0.id, self.0.name)
    }
}

/// The structural signature of a type.
///
/// `'tcx` is the lifetime of the arena backing all type allocations. All
/// variants are `Copy`: unit-like discriminants, `Copy` scalars, or
/// arena-backed fat-pointer references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TyKind<'tcx> {
    Int,
    Bool,
    Unit,
    /// Placeholder "any" type for polymorphic intrinsics (e.g. `println`).
    /// Will be retired in Step 10 once a `Display` typeclass is available.
    Top,
    Enum(EnumRef<'tcx>),
    /// Product type, arity >= 2 (arity-0 is `Unit`, arity-1 is plain grouping).
    /// The element slice is arena-allocated so `TyKind` stays `Copy`.
    Tuple(&'tcx [Ty<'tcx>]),
    /// A type parameter use site (the `T` in a generic signature/body). Opaque
    /// until monomorphisation (Step 3) substitutes a concrete type for it.
    Param(TypeParamId),
    /// A generic enum applied to concrete (or still-parametric) type arguments,
    /// e.g. `Option<Int>`. The `EnumRef` is the generic base enum; the slice is
    /// its type arguments, one per declared parameter. Distinct argument lists
    /// intern to distinct types. Monomorphisation (Step 3) replaces these with
    /// specialised concrete enums.
    App(EnumRef<'tcx>, &'tcx [Ty<'tcx>]),
}

/// A shallow, `Copy` handle to an interned [`TyKind`].
///
/// Equality and hashing are by pointer identity — sound because interning
/// guarantees that structurally equal types share the same arena allocation,
/// so identical structure ↔ identical pointer.
#[derive(Copy, Clone)]
pub struct Ty<'tcx>(pub(crate) &'tcx TyKind<'tcx>);

impl<'tcx> Ty<'tcx> {
    /// Access the structural signature of this type.
    #[inline]
    pub fn kind(self) -> &'tcx TyKind<'tcx> {
        self.0
    }

    /// `true` for types that are implicitly copied on use (Int, Bool, Unit).
    /// Enum types are *not* Copy and are subject to move semantics.
    pub fn is_copy(self) -> bool {
        matches!(self.kind(), TyKind::Int | TyKind::Bool | TyKind::Unit)
    }

    /// Equality that treats `Top` as compatible with any type.
    pub fn type_eq(self, other: Ty<'tcx>) -> bool {
        if std::ptr::eq(self.0, other.0) {
            return true;
        }
        matches!(
            (self.kind(), other.kind()),
            (TyKind::Top, _) | (_, TyKind::Top)
        )
    }

    pub fn type_neq(self, other: Ty<'tcx>) -> bool {
        !self.type_eq(other)
    }

    /// Like [`type_eq`](Ty::type_eq), but also looks through `Tuple` handles
    /// so that `Top` error-recovery types are recognised when nested in a
    /// composite type (e.g. `(Int, Top)` vs `(Int, Bool)` during recovery).
    pub fn compatible(self, other: Ty<'tcx>) -> bool {
        if self.type_eq(other) {
            return true;
        }
        match (self.kind(), other.kind()) {
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) if xs.len() == ys.len() => {
                xs.iter().zip(*ys).all(|(x, y)| x.compatible(*y))
            }
            (TyKind::App(e1, xs), TyKind::App(e2, ys)) if e1 == e2 && xs.len() == ys.len() => {
                xs.iter().zip(*ys).all(|(x, y)| x.compatible(*y))
            }
            _ => false,
        }
    }
}

impl PartialEq for Ty<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for Ty<'_> {}

impl PartialOrd for Ty<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Ordered by arena pointer address — consistent within a single compilation
/// context, where all pointers are stable after allocation.
impl Ord for Ty<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.0 as *const TyKind<'_>).cmp(&(other.0 as *const TyKind<'_>))
    }
}

impl Hash for Ty<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as *const TyKind<'_>).hash(state);
    }
}

impl fmt::Debug for Ty<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

/// Displays primitive types by name and composites structurally.
/// For enum types, use [`CompileCtx::display_ty`] to resolve the type name.
impl fmt::Display for Ty<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            TyKind::Int => write!(f, "Int"),
            TyKind::Bool => write!(f, "Bool"),
            TyKind::Unit => write!(f, "Unit"),
            TyKind::Top => write!(f, "Top"),
            TyKind::Enum(er) => write!(f, "Enum({:?})", er),
            TyKind::Param(id) => write!(f, "Param({})", id.0),
            TyKind::Tuple(ts) => {
                write!(f, "(")?;
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{t}")?;
                }
                write!(f, ")")
            }
            TyKind::App(er, args) => {
                write!(f, "App({er:?}<")?;
                for (i, t) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{t}")?;
                }
                write!(f, ">)")
            }
        }
    }
}

/// Pre-interned handles for the four primitive types, available on every
/// [`CompileCtx`](crate::compiler::context::CompileCtx) as `ctx.types`.
#[derive(Copy, Clone)]
pub struct CommonTypes<'tcx> {
    pub int: Ty<'tcx>,
    pub bool: Ty<'tcx>,
    pub unit: Ty<'tcx>,
    pub top: Ty<'tcx>,
}
