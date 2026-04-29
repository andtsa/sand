//! In the sand language, a castle is a collection of files that are compiled
//! together as a single unit, also known as a project. (im still not decided on
//! the name)
//!
//! this module provides utilities for working with multiple files, handling
//! configuration files, external dependencies, file IO, and other
//! non-language-specific tasks.

pub mod config;
pub mod discovery;
pub mod fileutils;
pub mod project;
