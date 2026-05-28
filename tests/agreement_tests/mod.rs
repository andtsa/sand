//! Agreement tests: Cross-layer verification
//!
//! Tests in this module verify that multiple compiler passes agree on their
//! results. For example, ensuring that the HIR interpreter and MIR interpreter
//! produce the same results for identical programs.
//!
//! These are distinct from layer tests, which verify correctness within a single pass.

pub mod hir_mir_agreement;
