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

/// the outstanding-borrow state of a place (variable), used to enforce the
/// mutable-borrow exclusivity invariant (Calculus §1.2, Step 9b). A place may
/// have any number of shared borrows *or* a single exclusive borrow, never
/// both. `Mut` dominates `Shared` when merging branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowState {
    /// one or more live shared (`&x`) borrows.
    Shared,
    /// a live exclusive (`&mut x`) borrow.
    Mut,
}

/// the ownership environment is a map from every in-scope variable to its
/// current ownership state
///
/// cloning this struct produces an independent snapshot,
/// which we use for exploring each branch of an if/match independently
#[derive(Debug, Clone, Default)]
pub struct OwnershipEnv<'tcx> {
    states: Map<UniqVar<'tcx>, OwnershipState>,
    /// outstanding borrows of each place, for the exclusivity invariant.
    /// Borrows are lexically scoped: snapshotted on block entry and
    /// restored on exit, so borrows created inside a block are released
    /// when it closes.
    borrows: Map<UniqVar<'tcx>, BorrowState>,
}

impl<'tcx> OwnershipEnv<'tcx> {
    pub fn new() -> Self {
        Self::default()
    }

    /// declare `var` as `Owned`
    ///
    /// should be used for new declarations and for re-assignments that restore
    /// ownership
    pub fn declare(&mut self, var: UniqVar<'tcx>) {
        self.states.insert(var, OwnershipState::Owned);
    }

    /// look up the ownership state of `var`
    ///
    /// returns `None` if the variable is not in scope
    pub fn get(&self, var: &UniqVar<'tcx>) -> Option<&OwnershipState> {
        self.states.get(var)
    }

    /// mark a variable as moved
    pub fn mark_moved(&mut self, var: UniqVar<'tcx>, at: Range) {
        self.states.insert(var, OwnershipState::Moved { at });
    }

    /// the current borrow state of `var`, if any.
    pub fn borrow_state(&self, var: &UniqVar<'tcx>) -> Option<BorrowState> {
        self.borrows.get(var).copied()
    }

    /// record a borrow of `var`. A second shared borrow leaves the state
    /// `Shared`; exclusivity conflicts are checked by the caller *before*
    /// calling this.
    pub fn add_borrow(&mut self, var: UniqVar<'tcx>, mutable: bool) {
        let state = if mutable {
            BorrowState::Mut
        } else {
            BorrowState::Shared
        };
        self.borrows.insert(var, state);
    }

    /// snapshot the outstanding borrows (taken on block entry).
    pub fn borrows_snapshot(&self) -> Map<UniqVar<'tcx>, BorrowState> {
        self.borrows.clone()
    }

    /// restore the borrows to a snapshot (on block exit), releasing every
    /// borrow created within the block.
    pub fn restore_borrows(&mut self, snapshot: Map<UniqVar<'tcx>, BorrowState>) {
        self.borrows = snapshot;
    }

    /// conservative join of two post-branch environments.
    ///
    /// a variable is `Owned` in the result only if it is `Owned` in *both*
    /// branches; a borrow live in *either* branch is live in the result (with
    /// `Mut` dominating `Shared`).
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
        for (var, state) in &right.borrows {
            merged
                .borrows
                .entry(*var)
                .and_modify(|s| {
                    if *state == BorrowState::Mut {
                        *s = BorrowState::Mut;
                    }
                })
                .or_insert(*state);
        }
        merged
    }

    /// snapshot the set of variables currently in scope
    pub fn var_keys(&self) -> Set<UniqVar<'tcx>> {
        self.states.keys().cloned().collect()
    }

    /// remove all variables whose keys are *not* in `vars`
    ///
    /// use on block exit to drop block-local variables from the environment
    pub fn restrict_to(&mut self, vars: &Set<UniqVar<'tcx>>) {
        self.states.retain(|v, _| vars.contains(v));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&UniqVar<'tcx>, &OwnershipState)> {
        self.states.iter()
    }
}
