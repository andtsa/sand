//! parse the pest tokens into an AST
//!

use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct LangParser;
