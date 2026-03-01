#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    Int,
    Bool,
    Unit,
    Top,    // the any type. used for error reporting when type inference fails
    Bottom, // the never type, for when an expression can never produce a value
}
