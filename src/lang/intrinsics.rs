//! intrinsics are functions that the compiler substitutes with non-language machine code,
//! in order to implement interactions with the OS


#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Intrinsic {
    Print,
    Println,
}


