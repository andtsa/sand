//! # Ownership environment
//! the persistent data structure threaded through the checker

// Ordered by `UniqVar` (its uniquification `idx`, assigned in a source-order
// pre-order walk) so that iterating block-local bindings yields them in
// declaration order, which `drop` insertion reverses for scope-exit drop order.
// The map is persistent, so per-branch snapshots stay cheap clones.
use im::HashSet as Set;
use im::OrdMap as Map;

use super::liveness::Liveness;
use crate::compiler::structure::Range;
use crate::compiler::structure::UniqVar;
use crate::lang::types::Ty;

/// the ownership state of a single variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipState {
    /// the variable currently owns its value; it may be used.
    Owned,
    /// the variable's value has been moved out; a subsequent use is an error.
    Moved { at: Range },
}

/// A single outstanding loan of a place, for the mutable-borrow exclusivity
/// invariant (Calculus: Ownership and Drop). A place may have any number of
/// shared loans *or* a single exclusive loan, never both.
///
/// `holder` is the variable the resulting reference is bound to (`let h = &x`),
/// or `None` for a temporary borrow (`f(&x)`). The holder drives non-lexical
/// release: the loan is dead once the holder's last use has passed (see
/// [`OwnershipEnv::prune_dead_loans`]). Temporaries stay lexically scoped.
#[derive(Debug, Clone, Copy)]
pub struct Loan<'tcx> {
    pub mutable: bool,
    pub holder: Option<UniqVar<'tcx>>,
}

/// the ownership environment is a map from every in-scope variable to its
/// current ownership state
///
/// cloning this struct produces an independent snapshot,
/// which we use for exploring each branch of an if/match independently
#[derive(Debug, Clone, Default)]
pub struct OwnershipEnv<'tcx> {
    states: Map<UniqVar<'tcx>, OwnershipState>,
    /// outstanding loans of each place, for the exclusivity invariant. Loans
    /// are released non-lexically: pruned once their holder's last use has
    /// passed ([`prune_dead_loans`](Self::prune_dead_loans)), with the
    /// lexical block-entry snapshot / block-exit restore kept as a backstop
    /// for temporaries and any untracked loans.
    borrows: Map<UniqVar<'tcx>, Vec<Loan<'tcx>>>,
    /// the declared type of each in-scope variable, so scope-exit drop
    /// insertion can exempt `Copy` bindings. A variable's type is
    /// fixed at declaration and never changes.
    types: Map<UniqVar<'tcx>, Ty<'tcx>>,
}

impl<'tcx> OwnershipEnv<'tcx> {
    pub fn new() -> Self {
        Self::default()
    }

    /// declare `var` as `Owned`
    ///
    /// should be used for new declarations and for re-assignments that restore
    /// ownership
    pub fn declare(&mut self, var: UniqVar<'tcx>, ty: Ty<'tcx>) {
        self.states.insert(var, OwnershipState::Owned);
        self.types.insert(var, ty);
    }

    /// the declared type of `var`, if in scope.
    pub fn var_ty(&self, var: &UniqVar<'tcx>) -> Option<Ty<'tcx>> {
        self.types.get(var).copied()
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

    /// Record a loan of `var` (`mutable` = exclusive). `holder` is the variable
    /// the reference is bound to, or `None` for a temporary. Exclusivity is
    /// checked by the caller (via [`borrow_conflict`](Self::borrow_conflict))
    /// *before* recording.
    pub fn add_borrow(&mut self, var: UniqVar<'tcx>, mutable: bool, holder: Option<UniqVar<'tcx>>) {
        self.borrows
            .entry(var)
            .or_default()
            .push(Loan { mutable, holder });
    }

    /// Attach `holder` to the most recently recorded loan of `var`, used when a
    /// `let h = &var` binding's holder becomes known after the borrow's been
    /// checked.
    pub fn attach_holder(&mut self, var: &UniqVar<'tcx>, holder: UniqVar<'tcx>) {
        if let Some(loans) = self.borrows.get_mut(var)
            && let Some(last) = loans.last_mut()
        {
            last.holder = Some(holder);
        }
    }

    /// Drop loans of `var` that are dead at `point`: a holder whose last use
    /// precedes `point` (or which is never used) can no longer conflict. This
    /// is the non-lexical release.
    ///
    /// A loan is kept (not pruned) when its holder is `None` (a temporary) or
    /// *escaping* (used in any non-dereference position, so the borrow may have
    /// propagated to a longer-lived place; see [`Liveness::holder_escapes`]).
    /// Such loans remain lexically scoped via the snapshot/restore backstop,
    /// which is sound: we only ever keep loans live too long, never too short.
    pub fn prune_dead_loans(&mut self, var: &UniqVar<'tcx>, live: &Liveness<'tcx>, point: usize) {
        let Some(loans) = self.borrows.get(var) else {
            return;
        };
        let kept: Vec<Loan<'tcx>> = loans
            .iter()
            .copied()
            .filter(|l| match l.holder {
                None => true,
                Some(h) => {
                    live.holder_escapes(&h) || live.last_use_of(&h).is_some_and(|lu| lu >= point)
                }
            })
            .collect();
        if kept.len() == loans.len() {
            return;
        }
        if kept.is_empty() {
            self.borrows.remove(var);
        } else {
            self.borrows.insert(*var, kept);
        }
    }

    /// Whether `var` has any live loan. Call *after* [`prune_dead_loans`].
    pub fn has_live_borrow(&self, var: &UniqVar<'tcx>) -> bool {
        self.borrows.get(var).is_some_and(|l| !l.is_empty())
    }

    /// If introducing a `mutable` borrow of `var` would conflict with an
    /// existing live loan, returns `Some(existing_is_mutable)`; else `None`.
    /// Any number of shared loans coexist; a mutable loan excludes all
    /// others. Call *after* [`prune_dead_loans`].
    pub fn borrow_conflict(&self, var: &UniqVar<'tcx>, mutable: bool) -> Option<bool> {
        let loans = self.borrows.get(var)?;
        if loans.is_empty() {
            return None;
        }
        let any_mut = loans.iter().any(|l| l.mutable);
        (mutable || any_mut).then_some(any_mut)
    }

    /// snapshot the outstanding loans (taken on block entry).
    pub fn borrows_snapshot(&self) -> Map<UniqVar<'tcx>, Vec<Loan<'tcx>>> {
        self.borrows.clone() // clones are cheap over immutable data structures
    }

    /// restore the loans to a snapshot (on block exit), releasing every loan
    /// created within the block (the lexical backstop).
    pub fn restore_borrows(&mut self, snapshot: Map<UniqVar<'tcx>, Vec<Loan<'tcx>>>) {
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
        // a loan live in *either* branch is live in the result: union the loan
        // lists per place (deduping identical loans).
        for (var, loans) in &right.borrows {
            merged
                .borrows
                .entry(*var)
                .and_modify(|existing| {
                    for l in loans {
                        if !existing
                            .iter()
                            .any(|e| e.mutable == l.mutable && e.holder == l.holder)
                        {
                            existing.push(*l);
                        }
                    }
                })
                .or_insert_with(|| loans.clone());
        }
        // types are identical for a var on both branches; union is enough.
        for (var, ty) in &right.types {
            merged.types.entry(*var).or_insert(*ty);
        }
        merged
    }

    /// Conservative join *and* the per-branch completing drops (Calculus:
    /// Ownership and Drop). The merged env is that of [`merge()`](Self::merge);
    /// in addition, a variable that one branch left `Owned` but the merge makes
    /// `Moved` (because the *other* branch moved it) must be dropped on the
    /// owning branch, so it is uniformly consumed at the join. Returns
    /// `(merged, drop_on_left, drop_on_right)`, each drop list in
    /// reverse-declaration order. Such candidates are always non-`Copy`: a
    /// `Copy` value is never marked `Moved`, so it never appears here.
    pub fn merge_with_drops(
        left: &Self,
        right: &Self,
    ) -> (Self, Vec<UniqVar<'tcx>>, Vec<UniqVar<'tcx>>) {
        let merged = Self::merge(left, right);
        let drops_for = |branch: &Self| -> Vec<UniqVar<'tcx>> {
            // `states` iterates in ascending `UniqVar` order (declaration
            // order); reverse for drop order.
            let mut vars: Vec<UniqVar<'tcx>> = merged
                .states
                .iter()
                .filter(|(var, st)| {
                    matches!(st, OwnershipState::Moved { .. })
                        && matches!(branch.states.get(var), Some(OwnershipState::Owned))
                })
                .map(|(var, _)| *var)
                .collect();
            vars.reverse();
            vars
        };
        let drop_on_left = drops_for(left);
        let drop_on_right = drops_for(right);
        (merged, drop_on_left, drop_on_right)
    }

    /// snapshot the set of variables currently in scope
    pub fn var_keys(&self) -> Set<UniqVar<'tcx>> {
        self.states.keys().cloned().collect()
    }

    /// remove all variables whose keys are *not* in `vars`
    ///
    /// use on block exit to drop block-local variables from the environment
    pub fn restrict_to(&mut self, vars: &Set<UniqVar<'tcx>>) {
        // im::OrdMap doesnt have `retain` :(
        // filter achieves the same result
        self.states = self
            .states
            .iter()
            .filter(|(v, _)| vars.contains(*v))
            .map(|(v, s)| (*v, s.clone()))
            .collect();
        self.types = self
            .types
            .iter()
            .filter(|(v, _)| vars.contains(*v))
            .map(|(v, t)| (*v, *t))
            .collect();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&UniqVar<'tcx>, &OwnershipState)> {
        self.states.iter()
    }
}
