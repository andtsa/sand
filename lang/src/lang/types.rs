/// Opaque index into the compiler's enum-definition registry.
/// Freely `Copy`, comparable, hashable — same pattern as `FunRef`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumRef(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    Int,
    Bool,
    Unit,
    Top,    // the any type. used for error reporting when type inference fails
    Bottom, // the never type, for when an expression can never produce a value
    Enum(EnumRef),
}

impl Ty {
    pub fn type_eq(&self, other: &Self) -> bool {
        use Ty::*;
        match (self, other) {
            (Bottom, Bottom) => true,
            (Top, Bottom) => false,
            (Bottom, Top) => false,
            (Top, _) => true,
            (_, Top) => true,
            (Int, Int) => true,
            (Bool, Bool) => true,
            (Unit, Unit) => true,
            (Enum(a), Enum(b)) => a == b,
            _ => false,
        }
    }

    pub fn type_neq(&self, other: &Self) -> bool {
        !self.type_eq(other)
    }
}
