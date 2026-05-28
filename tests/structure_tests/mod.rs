//! Structure tests: Infrastructure and context testing
//!
//! Tests in this module verify the infrastructure that underpins the compiler:
//! - File and module reference creation and tracking
//! - Compilation and project contexts
//! - Error and diagnostic handling
//!
//! These tests require access to compiler internal structures and may test behavior
//! that isn't directly observable at the language level.

pub mod compile_ctx_tests;
pub mod project_ctx_tests;
pub mod diagnostic_tests;
