use std::cmp::Ordering;
use std::fmt;
use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::EnumDef;

/// The kind of a type — how its values may be used.
///
/// This is the `{Owned, Never}` fragment of the kind lattice (Calculus §1):
/// `Owned` is the top (a normal, fully-capable value) and `Never` is the
/// bottom (the uninhabited kind of a diverging expression). The borrow modes
/// (`Borrowed`, `BorrowedMut`, `InteriorMut`) are introduced in later steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A normal owned value.
    Owned,
    /// A shared (immutable) borrow in region `'r` (Calculus §1.1). A borrowed
    /// value may be used multiple times and is not consumed.
    Borrowed(Region),
    /// The uninhabited kind: a diverging expression (e.g. an infinite loop)
    /// never produces a value, so it is usable where any kind is expected.
    Never,
}

impl Kind {
    /// Subkinding `self <: other` (Calculus §1.2): "`self` is usable where
    /// `other` is expected". `Never` is the bottom; `Owned` coerces to any
    /// `Borrowed` (`SK-OwnedBorrowed`); otherwise kinds are subkinds only of
    /// themselves (distinct borrow regions are incomparable).
    pub fn is_subkind(self, other: Kind) -> bool {
        match (self, other) {
            (Kind::Never, _) => true,
            (a, b) if a == b => true,
            (Kind::Owned, Kind::Borrowed(_)) => true,
            _ => false,
        }
    }

    /// Least upper bound of two kinds (Calculus §1.4), used to merge the kinds
    /// of the branches of an `if`/`match`. `Never` is the identity; any two
    /// distinct non-`Never` kinds join to `Owned` (the top).
    pub fn join(self, other: Kind) -> Kind {
        match (self, other) {
            (Kind::Never, k) | (k, Kind::Never) => k,
            (a, b) if a == b => a,
            _ => Kind::Owned,
        }
    }
}

/// A globally unique identifier for a type parameter (the `T` in
/// `def f<T>(...)` or `type Option<T> = ...`). Assigned once per declared
/// parameter; two parameters named `T` in different declarations get distinct
/// ids, so `TyKind::Param` comparison is unambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeParamId(pub usize);

/// How a type constructor's behaviour relates to a parameter's subtyping
/// (Calculus §2.1). The system currently has no subtyping between concrete
/// types, so variance is validated at the declaration site but has no effect
/// on use-site checking yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Variance {
    /// `+` — covariant: the parameter appears only in producer positions.
    Covariant,
    /// `-` — contravariant: the parameter appears only in consumer positions.
    Contravariant,
    /// `∅` — invariant: the parameter appears in both (always sound).
    Invariant,
}

/// A region (lifetime) variable, interned per declaration scope (Calculus
/// §1.1). Distinct names in the same scope get distinct ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionVar(pub usize);

/// A region: either a variable `'r` or the permanent `'static` region that
/// outlives everything (Calculus §1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Region {
    Var(RegionVar),
    Static,
}

/// An outlives constraint `longer ≥ shorter`: region `longer` outlives region
/// `shorter` (Calculus §1.1). Collected during checking and discharged by the
/// region solver (a trivially-satisfied stub until Step 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionConstraint {
    pub longer: Region,
    pub shorter: Region,
}

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
    /// A type ascribed to a region, `T @ 'r` (Calculus §2.3). Carries the same
    /// kind as its inner type. Regions have no runtime representation, so
    /// monomorphisation erases this back to the inner `T`.
    Region(Ty<'tcx>, Region),
    /// A shared reference `&'r T` (Calculus §2.3), of kind `Borrowed 'r`.
    /// Immutable shared borrows have no distinct runtime representation in this
    /// phase, so monomorphisation erases `&'r T` to `T`.
    Ref(Region, Ty<'tcx>),
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
        match self.kind() {
            TyKind::Int | TyKind::Bool | TyKind::Unit => true,
            TyKind::Region(t, _) => t.is_copy(),
            // shared references are freely copyable (immutable, no ownership).
            TyKind::Ref(..) => true,
            _ => false,
        }
    }

    /// `true` if this type mentions any type parameter (directly or nested in a
    /// tuple/instantiation). Used to decide whether a value's type still needs
    /// substitution before it is fully concrete.
    pub fn has_param(self) -> bool {
        match self.kind() {
            TyKind::Param(_) => true,
            TyKind::Tuple(elems) => elems.iter().any(|t| t.has_param()),
            TyKind::App(_, args) => args.iter().any(|t| t.has_param()),
            TyKind::Region(t, _) => t.has_param(),
            TyKind::Ref(_, t) => t.has_param(),
            _ => false,
        }
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
            (TyKind::Region(a, r1), TyKind::Region(b, r2)) if r1 == r2 => a.compatible(*b),
            (TyKind::Ref(r1, a), TyKind::Ref(r2, b)) if r1 == r2 => a.compatible(*b),
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
            TyKind::Region(t, r) => write!(f, "{t} @ {r:?}"),
            TyKind::Ref(r, t) => write!(f, "&{r:?} {t}"),
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
