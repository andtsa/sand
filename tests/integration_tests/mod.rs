//! Integration tests: End-to-end and cross-system tests
//!
//! Tests in this module verify complete compiler workflows and system-level behavior:
//! - Example programs that should compile and run successfully
//! - Multi-file compilation and module resolution
//! - Cross-file function calls and module linking
//! - Error recovery and diagnostics in realistic scenarios

pub mod examples;
pub mod multi_file;
