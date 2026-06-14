//! Layer tests: Testing individual compiler passes in isolation
//!
//! Each module tests one specific compiler pass and layer:
//! - `parse_tests`: HHIR parsing (raw syntax tree construction)
//! - `qualify_tests`: QHIR qualification (name resolution & module linking)
//! - `typecheck_tests`: TypedHIR type checking (type analysis & inference)
//! - `interpreter_tests`: Both HIR and MIR interpretation. each test runs the
//!   program through both interpreters and asserts they agree.
//!
//! Tests in this category do NOT verify agreement between layers.
//! see `agreement_tests` for cross-layer verification.

pub mod borrow_tests;
pub mod deref_tests;
pub mod enum_tests;
pub mod generics_tests;
pub mod interpreter_tests;
pub mod kind_tests;
pub mod match_tests;
pub mod module_tests;
pub mod mut_borrow_tests;
pub mod operator_tests;
pub mod ownership_tests;
pub mod parse_tests;
pub mod qualify_tests;
pub mod region_adt_tests;
pub mod region_escape_tests;
pub mod region_inference_tests;
pub mod region_param_tests;
pub mod region_tests;
pub mod typecheck_tests;
