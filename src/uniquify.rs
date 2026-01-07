//! the uniquify pass of the compiler
//!
//! takes a program AST and ensures all variable and function names are unique
use crate::lang::Program;

impl Program {
    pub fn uniquify(&self) -> Self {
        // TODO
        self.clone()
    }
}
