//! Internal declarative macros shared across the crate.

/// Generate the standard trait impls for a `Copy` arena handle newtype of the
/// shape `Ref<'tcx>(&'tcx Inner)` whose pointee carries a monotonic `id`:
///
/// - `PartialEq`/`Eq`/`Hash` by **pointer identity** (each distinct pointee is
///   allocated once, so identical pointee ⇔ identical pointer), and
/// - `PartialOrd`/`Ord` by the pointee's `id` (deterministic registration
///   order), plus
/// - `Debug` printing `Label(id, name)`.
///
/// `$this => $name` supplies the value shown for `name` in `Debug` (e.g.
/// `this => this.0.name`); `$label` is the literal shown before `(`.
macro_rules! impl_arena_ref_traits {
    ($ref_ty:ty, $label:literal, $this:ident => $name:expr) => {
        impl PartialEq for $ref_ty {
            fn eq(&self, other: &Self) -> bool {
                std::ptr::eq(self.0, other.0)
            }
        }
        impl Eq for $ref_ty {}
        impl std::hash::Hash for $ref_ty {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                std::ptr::hash(self.0, state);
            }
        }
        impl PartialOrd for $ref_ty {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for $ref_ty {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.0.id.cmp(&other.0.id)
            }
        }
        impl std::fmt::Debug for $ref_ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let $this = self;
                write!(f, concat!($label, "({}, {})"), $this.0.id, $name)
            }
        }
    };
}

pub(crate) use impl_arena_ref_traits;
