//! display implementations for inspecting the different IRs,
//! and formatting parameters
//!
//! todo: move fmt params somewhere configurable (probably by the user)

pub mod mir;
pub mod prog;
pub mod typed_expr;

/// by default, use 4 spaces for indentation
pub const INDENT: &str = "    ";

/// maximum line length before wrapping
pub const MAX_LINE_LENGTH: usize = 80;
