//! Robustness tests: Edge cases and regression tests grouped by weakness

pub mod call_arity_and_type;
pub mod division_by_zero;
pub mod duplicate_parameters;
pub mod if_without_else;
pub mod integer_literals;
pub mod mir_block_structure;
pub mod mir_structure;
pub mod missing_keywords;
pub mod mutability;
pub mod overflow_underflow;
pub mod return_type_mismatch;
pub mod variable_scope;
pub mod while_return_type;
