//! # Ownership environment
//! the persistent data structure threaded through the checker

use im::HashMap as Map;
use im::HashSet as Set;

use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;

/// the ownership state of a single variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipState {
    /// the variable currently owns its value; it may be used.
    Owned,
    /// the variable's value has been moved out; a subsequent use is an error.
    Moved { at: Range },
}

/// the ownership environment is a map from every in-scope variable to its
/// current ownership state
///
/// cloning this struct produces an independent snapshot,
/// which we use for exploring each branch of an if/match independently
#[derive(Debug, Clone, Default)]
pub struct OwnershipEnv {
    states: Map<UniqVar, OwnershipState>,
}

impl OwnershipEnv {
    pub fn new() -> Self {
        Self::default()
    }

    /// declare `var` as `Owned`
    ///
    /// should be used for new declarations and for re-assignments that restore
    /// ownership
    pub fn declare(&mut self, var: UniqVar) {
        self.states.insert(var, OwnershipState::Owned);
    }

    /// look up the ownership state of `var`
    ///
    /// returns `None` if the variable is not in scope
    pub fn get(&self, var: &UniqVar) -> Option<&OwnershipState> {
        self.states.get(var)
    }

    /// mark a variable as moved
    pub fn mark_moved(&mut self, var: UniqVar, at: Range) {
        self.states.insert(var, OwnershipState::Moved { at });
    }

    /// conservative join of two post-branch environments.
    ///
    /// a variable is `Owned` in the result only if it is `Owned` in *both*
    /// branches
    pub fn merge(left: &Self, right: &Self) -> Self {
        // start from the left env, adjust any variable that right moved
        let mut merged = left.clone();
        for (var, state) in &right.states {
            match state {
                OwnershipState::Moved { at } => {
                    merged
                        .states
                        .insert(*var, OwnershipState::Moved { at: *at });
                }
                OwnershipState::Owned => {
                    merged.states.entry(*var).or_insert(OwnershipState::Owned);
                }
            }
        }
        merged
    }

    /// snapshot the set of variables currently in scope
    pub fn var_keys(&self) -> Set<UniqVar> {
        self.states.keys().cloned().collect()
    }

    /// remove all variables whose keys are *not* in `vars`
    ///
    /// use on block exit to drop block-local variables from the environment
    pub fn restrict_to(&mut self, vars: &Set<UniqVar>) {
        self.states.retain(|v, _| vars.contains(v));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&UniqVar, &OwnershipState)> {
        self.states.iter()
    }
}
