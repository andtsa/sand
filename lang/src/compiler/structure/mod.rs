//! types for structuring projects

mod debug;
mod enums;
mod functions;
mod projects;
mod type_params;
mod variables;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub use debug::*;
pub use enums::*;
pub use functions::*;
pub use projects::*;
pub use type_params::*;
pub use variables::*;

pub type Map<K, V> = BTreeMap<K, V>;
pub type Set<V> = BTreeSet<V>;
