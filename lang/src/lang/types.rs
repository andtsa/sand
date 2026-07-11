use std::cmp::Ordering;
use std::fmt;
use std::hash::Hash;
use std::hash::Hasher;

use crate::compiler::structure::AdtDef;
use crate::util::macros::impl_arena_ref_traits;

/// The kind of a type reflects how its values may be used.
///
/// This is the `{Owned, Borrowed, BorrowedMut, Never}` fragment of the kind
/// lattice (Calculus: Kinds): `Owned` is the top (a normal, fully-capable
/// value) and `Never` is the bottom (the uninhabited kind of a diverging
/// expression). `Borrowed` and `BorrowedMut` are mutually-incomparable borrow
/// modes; the remaining mode (`InteriorMut`) is out of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A normal owned value.
    Owned,
    /// A shared (immutable) borrow. A borrowed value may be used multiple times
    /// and is not consumed. The borrow's *region* lives on the **type**
    /// (`&'r T`), not the kind: kinds record only *capability*; regions belong
    /// to the type system and region escape is checked on the type.
    Borrowed,
    /// An exclusive (mutable) borrow. While it is live, no
    /// other borrow of the same place may exist (the exclusivity invariant,
    /// enforced by the ownership pass). Its region lives on the type, as
    /// for `Borrowed`.
    BorrowedMut,
    /// The uninhabited kind: a diverging expression (e.g. an infinite loop)
    /// never produces a value, so it is usable where any kind is expected.
    Never,
    /// A type-constructor kind `K₁ -> K₂` (higher-kinded type
    /// parameters): the kind of a thing that, applied to a type of kind `K₁`,
    /// yields a type of kind `K₂`, e.g. `Option : Owned -> Owned`. The arrow's
    /// domain/codomain are held in a context-side **kind interner** (canonical,
    /// so equal arrows share one [`KindId`] and derived `Eq`/`Hash`/`Ord` on
    /// the id are structural). `Kind` therefore stays `Copy` and
    /// lifetime-free while the arrow space is fully general (nesting /
    /// multi-argument via currying).
    Arrow(KindId),
}

/// How a call uses a function value's captured environment.
///
/// This is sand's single kind-annotated arrow (Calculus: Types, the function
/// arrow), standing in for Rust's three closure traits. `Reusable` is the
/// bare-`->` default (the common case, and what higher-order functions like
/// `fmap` need). Only `Reusable` is produced from surface syntax until closures
/// land; the others are reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FnMode {
    /// `-[Borrowed]>`, ≈ Rust `Fn`: reads its environment; callable repeatedly.
    Reusable,
    /// `-[BorrowedMut]>`, ≈ Rust `FnMut`: may mutate its environment.
    ReusableMut,
    /// `-[Owned]>`, ≈ Rust `FnOnce`: may consume its environment; callable
    /// once.
    Consuming,
}

impl FnMode {
    /// Subsumption `self <: other`: a function of mode `self` is usable where
    /// mode `other` is expected. A reusable (`Fn`) closure can stand in for any
    /// arrow, and a mutating (`FnMut`) one for a consuming (`FnOnce`) slot,
    /// mirroring Rust's `Fn ⊆ FnMut ⊆ FnOnce`.
    pub fn usable_as(self, other: FnMode) -> bool {
        use FnMode::*;
        matches!(
            (self, other),
            (Reusable, _) | (ReusableMut, ReusableMut | Consuming) | (Consuming, Consuming)
        )
    }
}

/// Canonical id of an interned arrow kind (`K₁ -> K₂`); see [`Kind::Arrow`].
/// Interned per [`crate::compiler::context::CompileCtx`]; ids are only
/// meaningful within one compilation (kinds never cross contexts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KindId(pub usize);

impl Kind {
    /// Subkinding `self <: other` (Calculus: Kinds, subkinding): "`self` is
    /// usable where `other` is expected". `Never` is the bottom; `Owned`
    /// coerces to any borrow mode; otherwise kinds are subkinds only of
    /// themselves (the two borrow modes are incomparable). Regions play no
    /// part, they live on the type.
    pub fn is_subkind(self, other: Kind) -> bool {
        match (self, other) {
            (Kind::Never, _) => true,
            (a, b) if a == b => true,
            (Kind::Owned, Kind::Borrowed | Kind::BorrowedMut) => true,
            _ => false,
        }
    }

    /// Least upper bound of two kinds (Calculus: Kinds, join), used to merge
    /// the kinds of the branches of an `if`/`match`. `Never` is the
    /// identity; any two distinct non-`Never` kinds join to `Owned` (the
    /// top).
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
/// (Calculus: Types, variance). The system currently has no subtyping between
/// concrete types, so variance is validated at the declaration site but has no
/// effect on use-site checking yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Variance {
    /// `+` covariant: the parameter appears only in producer positions.
    Covariant,
    /// `-` contravariant: the parameter appears only in consumer positions.
    Contravariant,
    /// `∅` invariant: the parameter appears in both (always sound).
    Invariant,
}

/// A region (lifetime) variable, interned per declaration scope (Calculus:
/// Regions). Distinct names in the same scope get distinct ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionVar(pub usize);

/// A region: either a variable `'r` or the permanent `'static` region that
/// outlives everything (Calculus: Regions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Region {
    Var(RegionVar),
    Static,
}

/// An outlives constraint `longer ≥ shorter`: region `longer` outlives region
/// `shorter` (Calculus: Regions). Stored from `where` clauses and discharged by
/// the region solver
/// ([`outlives`](crate::compiler::context::CompileCtx::outlives)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionConstraint {
    pub longer: Region,
    pub shorter: Region,
}

/// Lifetime-elision rule scaffolding. These describe how an omitted region in a
/// function signature *would* be filled in
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElisionRule {
    /// With exactly one input reference, every elided output region is that
    /// input's region (the single-input rule).
    SingleInput,
    /// Each elided input reference gets its own fresh region.
    FreshPerInput,
}

/// A `Copy` handle to an arena-allocated [`AdtDef`].
///
/// Equality and hashing are by pointer identity: each distinct enum (named
/// enums deduplicated by name, anonymous tag-unions by tag set) is allocated
/// exactly once, so identical enum <=> identical pointer. Ordering is by the
/// monotonic registration `id` for deterministic iteration.
#[derive(Copy, Clone)]
pub struct AdtRef<'tcx>(pub(crate) &'tcx AdtDef<'tcx>);

impl<'tcx> AdtRef<'tcx> {
    /// Access the underlying enum definition.
    #[inline]
    pub fn def(self) -> &'tcx AdtDef<'tcx> {
        self.0
    }
}

impl_arena_ref_traits!(AdtRef<'_>, "EnumRef", this => this.0.name);

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
    /// To be retired once a `Display` typeclass is available.
    Top,
    Enum(AdtRef<'tcx>),
    /// Product type, arity >= 2 (arity-0 is `Unit`, arity-1 is plain grouping).
    /// The element slice is arena-allocated so `TyKind` stays `Copy`.
    Tuple(&'tcx [Ty<'tcx>]),
    /// A type parameter use site (the `T` in a generic signature/body). Opaque
    /// until monomorphisation substitutes a concrete type for it.
    Param(TypeParamId),
    /// A **higher-kinded** type parameter applied to arguments, `F<A>`
    /// where `F` is a type *constructor* parameter (kind `Owned -> Owned`), not
    /// a concrete enum. Distinct from [`TyKind::App`], whose head is a known
    /// `EnumRef`. Opaque until monomorphisation substitutes a concrete
    /// constructor for `F` (its `Subst` entry is the bare `Enum(er)`), turning
    /// `F<A>` into `App(er, A)`. Like `Param`, it never survives mono.
    ParamApp(TypeParamId, &'tcx [Ty<'tcx>]),
    /// A **hole** in a partial application (Calculus §4.5): the `_` in an
    /// abstraction head such as `Result<_, E>` (`Ty::App(er, [Hole(0), E])`).
    /// The `u32` is the hole's positional index (`0..m-1`, in left-to-right
    /// order of the `_`s), matching the argument order of the higher-kinded
    /// parameter it abstracts. A hole is *only* well-formed inside a
    /// constructor-abstraction value — the binding of a higher-kinded parameter
    /// and the head of an `impl` — and is removed by β-reduction (§4.5) before
    /// a value's type is formed, so it never survives into
    /// monomorphisation.
    Hole(u32),
    /// A function type `A -> B`: a first-class function / closure value. Unary
    /// (multi-argument via tuple or currying). The [`FnMode`] records how a
    /// call uses the closure's captured environment: sand's single
    /// kind-annotated arrow (Calculus: Types, the function arrow) standing
    /// in for Rust's three closure traits (`Fn`/`FnMut`/`FnOnce`).
    /// Arena-backed, so `TyKind` stays `Copy`.
    ///
    /// The fourth component is the **environment type**: the (structural tuple)
    /// type of the values the closure captures, or `Unit` for a capture-free
    /// function. Carrying it on the type is what lets the closure's soundness
    /// properties be decided structurally — escape (`freeRegions(env)`), drop,
    /// `Copy`, and the `Send`/`Sync` markers all read it. The env does **not**
    /// yet participate in type equality/subsumption (it is informational until
    /// env-polymorphism lands), so `A -> B` still abstracts over captures.
    Fn(Ty<'tcx>, Ty<'tcx>, FnMode, Ty<'tcx>),
    /// A generic enum applied to concrete (or still-parametric) type arguments,
    /// e.g. `Option<Int>`. The `EnumRef` is the generic base enum; the slice is
    /// its type arguments, one per declared parameter. Distinct argument lists
    /// intern to distinct types. Monomorphisation replaces these with
    /// specialised concrete enums.
    /// `Option<Int>` / `Holder<'a, T>`. Second slice = type arguments (one per
    /// declared type parameter); third slice = region arguments (one per
    /// declared region parameter, lifetimes-first). The region args carry
    /// the lifetimes a value of this type may borrow from, so `freeRegions`
    /// exposes them to the escape check. Distinct type *or* region
    /// arguments intern distinct. Monomorphisation drops the region args
    /// (regions are compile-time).
    App(AdtRef<'tcx>, &'tcx [Ty<'tcx>], &'tcx [Region]),
    /// A type ascribed to a region, `T @ 'r` (Calculus: Types). Carries the
    /// same kind as its inner type. Regions have no runtime representation,
    /// so monomorphisation erases this back to the inner `T`.
    Region(Ty<'tcx>, Region),
    /// A shared reference `&'r T` (Calculus: Types), of kind `Borrowed 'r`.
    /// Immutable shared borrows have no distinct runtime representation in this
    /// phase, so monomorphisation erases `&'r T` to `T`.
    Ref(Region, Ty<'tcx>),
    /// An exclusive (mutable) reference `&'r mut T` (Calculus: Types), of kind
    /// `BorrowedMut 'r`. Like `Ref`, borrows have no distinct runtime
    /// representation yet, so monomorphisation erases `&'r mut T` to `T`.
    RefMut(Region, Ty<'tcx>),
    /// A raw pointer `Ptr<T>`: address-sized, `Copy`, *outside* the affine /
    /// region / borrow discipline. Unlike `Ref`, a `Ptr` carries
    /// no region and has a real runtime representation (`ptr`), so it survives
    /// monomorphisation (only the element type `T` is substituted). Deref is
    /// the `unsafe` operation; use is confined to the core library.
    Ptr(Ty<'tcx>),
}

/// A shallow, `Copy` handle to an interned [`TyKind`].
///
/// Equality and hashing are by pointer identity. sound because interning
/// guarantees that structurally equal types share the same arena allocation,
/// so identical structure <=> identical pointer.
#[derive(Copy, Clone)]
pub struct Ty<'tcx>(pub(crate) &'tcx TyKind<'tcx>);

impl<'tcx> Ty<'tcx> {
    /// Access the structural signature of this type.
    #[inline]
    pub fn kind(self) -> &'tcx TyKind<'tcx> {
        self.0
    }

    /// `true` if this type mentions any type parameter (directly or nested in a
    /// tuple/instantiation). Used to decide whether a value's type still needs
    /// substitution before it is fully concrete.
    pub fn has_param(self) -> bool {
        match self.kind() {
            TyKind::Param(_) => true,
            // `F<A>` has a parameter head, so it is always non-concrete.
            TyKind::ParamApp(_, _) => true,
            TyKind::Fn(a, r, _, env) => a.has_param() || r.has_param() || env.has_param(),
            TyKind::Tuple(elems) => elems.iter().any(|t| t.has_param()),
            TyKind::App(_, args, _) => args.iter().any(|t| t.has_param()),
            TyKind::Region(t, _) => t.has_param(),
            TyKind::Ref(_, t) | TyKind::RefMut(_, t) => t.has_param(),
            TyKind::Ptr(t) => t.has_param(),
            _ => false,
        }
    }

    /// `true` if this type contains a constructor **hole** (`TyKind::Hole`),
    /// i.e. it is (or embeds) a partial application (Calculus §4.5). Used to
    /// assert that no hole leaks past instance elaboration into a value type or
    /// monomorphisation.
    pub fn has_hole(self) -> bool {
        match self.kind() {
            TyKind::Hole(_) => true,
            TyKind::Fn(a, r, _, env) => a.has_hole() || r.has_hole() || env.has_hole(),
            TyKind::Tuple(elems) => elems.iter().any(|t| t.has_hole()),
            TyKind::App(_, args, _) | TyKind::ParamApp(_, args) => {
                args.iter().any(|t| t.has_hole())
            }
            TyKind::Region(t, _) | TyKind::Ref(_, t) | TyKind::RefMut(_, t) | TyKind::Ptr(t) => {
                t.has_hole()
            }
            _ => false,
        }
    }

    /// Collect every [`TypeParamId`] appearing in this type into `out`. Used at
    /// call sites to recover the *callee's own* type parameters from its
    /// declared signature (the enclosing function's rigid parameters never
    /// appear in a callee's stored signature), so the checker can verify
    /// they were all solved before substituting. a parameter left unbound
    /// would otherwise leak into the result type and crash
    /// monomorphisation.
    pub fn collect_params(self, out: &mut Vec<TypeParamId>) {
        match self.kind() {
            TyKind::Param(id) => out.push(*id),
            TyKind::ParamApp(id, args) => {
                out.push(*id);
                for a in args.iter() {
                    a.collect_params(out);
                }
            }
            TyKind::Fn(a, r, _, env) => {
                a.collect_params(out);
                r.collect_params(out);
                env.collect_params(out);
            }
            TyKind::Tuple(elems) => {
                for e in elems.iter() {
                    e.collect_params(out);
                }
            }
            TyKind::App(_, args, _) => {
                for a in args.iter() {
                    a.collect_params(out);
                }
            }
            TyKind::Region(t, _) | TyKind::Ref(_, t) | TyKind::RefMut(_, t) | TyKind::Ptr(t) => {
                t.collect_params(out)
            }
            _ => {}
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

    /// Structural equality that ignores reference / region-ascription *regions*
    /// (region-blind). Used at type-checking boundaries while regions live on
    /// the type but are validated separately by the escape check (on free
    /// regions), not by use-site comparison. Full region-aware subtyping
    /// (covariant `&`, invariant `&mut`) is what eventually replaces this.
    pub fn eq_modulo_regions(self, other: Ty<'tcx>) -> bool {
        if self.type_eq(other) {
            return true;
        }
        match (self.kind(), other.kind()) {
            (TyKind::Ref(_, a), TyKind::Ref(_, b))
            | (TyKind::RefMut(_, a), TyKind::RefMut(_, b))
            | (TyKind::Region(a, _), TyKind::Region(b, _)) => a.eq_modulo_regions(*b),
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) if xs.len() == ys.len() => {
                xs.iter().zip(*ys).all(|(x, y)| x.eq_modulo_regions(*y))
            }
            // region-blind: ignore the region args (regions are inferred at call
            // sites, not matched here), compare only the type args.
            (TyKind::App(e1, xs, _), TyKind::App(e2, ys, _))
                if e1 == e2 && xs.len() == ys.len() =>
            {
                xs.iter().zip(*ys).all(|(x, y)| x.eq_modulo_regions(*y))
            }
            (TyKind::Ptr(a), TyKind::Ptr(b)) => a.eq_modulo_regions(*b),
            (TyKind::ParamApp(p1, xs), TyKind::ParamApp(p2, ys))
                if p1 == p2 && xs.len() == ys.len() =>
            {
                xs.iter().zip(*ys).all(|(x, y)| x.eq_modulo_regions(*y))
            }
            // `self` is the actual type, `other` the expected; a function value
            // may stand in for a more-permissive arrow (arrow subsumption).
            // The env type is informational and does not participate in
            // equality/subsumption (so `A -> B` abstracts over captures).
            (TyKind::Fn(a1, r1, m1, _), TyKind::Fn(a2, r2, m2, _)) if m1.usable_as(*m2) => {
                a1.eq_modulo_regions(*a2) && r1.eq_modulo_regions(*r2)
            }
            _ => false,
        }
    }

    /// Collect the free regions appearing in this type into `out` (the
    /// `freeRegions` of the Calculus escape check). Used by the escape check: a
    /// value crossing a scope boundary must not name a region introduced at or
    /// inside that scope.
    pub fn free_regions(self, out: &mut Vec<crate::lang::types::Region>) {
        match self.kind() {
            TyKind::Ref(r, t) | TyKind::RefMut(r, t) => {
                out.push(*r);
                t.free_regions(out);
            }
            TyKind::Region(t, r) => {
                out.push(*r);
                t.free_regions(out);
            }
            TyKind::Tuple(elems) => {
                for e in elems.iter() {
                    e.free_regions(out);
                }
            }
            TyKind::App(_, args, regions) => {
                for a in args.iter() {
                    a.free_regions(out);
                }
                // the lifetimes this ADT instantiation borrows from: exposing
                // them lets the escape check catch an ADT holding a local borrow.
                for r in regions.iter() {
                    out.push(*r);
                }
            }
            TyKind::ParamApp(_, args) => {
                for a in args.iter() {
                    a.free_regions(out);
                }
            }
            // A closure's captured environment may borrow from regions; exposing
            // them lets the escape check reject a closure that captures a local
            // borrow and escapes (`freeRegions(env)`).
            TyKind::Fn(a, r, _, env) => {
                a.free_regions(out);
                r.free_regions(out);
                env.free_regions(out);
            }
            _ => {}
        }
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
            (TyKind::App(e1, xs, rs1), TyKind::App(e2, ys, rs2))
                if e1 == e2 && xs.len() == ys.len() && rs1 == rs2 =>
            {
                xs.iter().zip(*ys).all(|(x, y)| x.compatible(*y))
            }
            (TyKind::Region(a, r1), TyKind::Region(b, r2)) if r1 == r2 => a.compatible(*b),
            (TyKind::Ref(r1, a), TyKind::Ref(r2, b)) if r1 == r2 => a.compatible(*b),
            (TyKind::RefMut(r1, a), TyKind::RefMut(r2, b)) if r1 == r2 => a.compatible(*b),
            (TyKind::Ptr(a), TyKind::Ptr(b)) => a.compatible(*b),
            (TyKind::ParamApp(p1, xs), TyKind::ParamApp(p2, ys))
                if p1 == p2 && xs.len() == ys.len() =>
            {
                xs.iter().zip(*ys).all(|(x, y)| x.compatible(*y))
            }
            (TyKind::Fn(a1, r1, m1, _), TyKind::Fn(a2, r2, m2, _)) if m1 == m2 => {
                a1.compatible(*a2) && r1.compatible(*r2)
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

/// Ordered by arena pointer address.
/// consistent within a single compilation
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
            TyKind::Hole(i) => write!(f, "_{i}"),
            TyKind::ParamApp(id, args) => {
                write!(f, "Param({})<", id.0)?;
                for (i, t) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{t}")?;
                }
                write!(f, ">")
            }
            // The reusable arrow prints bare; the mutating/consuming arrows
            // surface their kind so two arrows that differ only in mode (e.g.
            // `Int -> Int` vs `Int -[Owned]> Int`) are distinguishable.
            // The env type is not surfaced (it does not affect the surface
            // arrow type the programmer wrote).
            TyKind::Fn(a, r, mode, _) => match mode {
                FnMode::Reusable => write!(f, "{a} -> {r}"),
                FnMode::ReusableMut => write!(f, "{a} -[BorrowedMut]> {r}"),
                FnMode::Consuming => write!(f, "{a} -[Owned]> {r}"),
            },
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
            TyKind::App(er, args, regions) => {
                write!(f, "App({er:?}<")?;
                let mut first = true;
                for r in regions.iter() {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "{r:?}")?;
                    first = false;
                }
                for t in args.iter() {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "{t}")?;
                    first = false;
                }
                write!(f, ">)")
            }
            TyKind::Region(t, r) => write!(f, "{t} @ {r:?}"),
            TyKind::Ref(r, t) => write!(f, "&{r:?} {t}"),
            TyKind::RefMut(r, t) => write!(f, "&{r:?} mut {t}"),
            TyKind::Ptr(t) => write!(f, "Ptr<{t}>"),
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
