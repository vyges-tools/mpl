// SPDX-License-Identifier: Apache-2.0
//! Applying the placement — as a **transaction**, not as a sequence of edits.
//!
//! 🔑 **The house rule: a failed run leaves the database exactly as it was found.** The engine
//! decides where macros go; the database records that decision only if the whole decision is
//! sound. A half-placed design is worse than an unplaced one, because it looks placed.
//!
//! That differs from OpenROAD's model, where the caller invokes an explicit rollback that walks
//! back over the values it changed. Here the scope IS the transaction: leaving it by any path —
//! success, refusal, or error — settles the database.
//!
//! ## ⛔ The journal does not cover everything
//!
//! OpenDB's ECO journal handles `dbInst`, `dbNet`, `dbBTerm`, `dbITerm`, `dbGuide` and friends.
//! It has **no `dbBlockage` case**, so a blockage created inside `eco_begin`/`eco_undo`
//! **survives the rollback** (verified at pin `945a9f4`). Macro placement writes blockages on its
//! main path — 34 of upstream's 36 DEF goldens carry a `BLOCKAGES` section — so the engine has to
//! undo those itself. [`Transaction`] does, and the type exists precisely so that no call site
//! has to remember to.

/// What the engine wrote, and what it must take back if the run does not stand.
///
/// ⚠️ **Record the baseline BEFORE any edit.** The blockage count is the only piece of state the
/// journal will not restore for us, so it is the only piece we have to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Baseline {
    /// `num_blockages()` as it stood before the engine touched anything.
    pub blockages: usize,
}

/// How a transaction ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settlement {
    /// The edits stand.
    Committed,
    /// The edits were taken back: journalled ones by `eco_undo`, blockages by hand.
    RolledBack,
}

/// The manual half of a rollback: how many blockages to destroy to get back to `baseline`.
///
/// Returns the indices to destroy, **highest first** — destroying one shifts the indices of
/// everything after it, so working forwards would delete the wrong objects and then run off the
/// end. Empty when nothing was added.
///
/// 🔑 Split out as a pure function so the ordering rule is testable without a database. It is the
/// kind of off-by-one that a database test would show only as a confusing count.
pub fn blockages_to_destroy(baseline: Baseline, now: usize) -> Vec<usize> {
    // ℹ️ No guard for `now < baseline` is needed: a Rust range whose start exceeds its end is
    // simply EMPTY, it does not panic or wrap. An earlier version of this function had such a
    // guard with a comment claiming otherwise; mutation testing removed the guard, nothing failed,
    // and the comment turned out to be the only thing that had been wrong.
    (baseline.blockages..now).rev().collect()
}

/// Whether a verdict should keep the edits.
///
/// ⚠️ **Only an outright success commits.** A refusal rolls back even though it is not an error —
/// the engine declining to place a design is not a licence to leave half of it placed.
pub fn settles_as(kept: bool) -> Settlement {
    if kept {
        Settlement::Committed
    } else {
        Settlement::RolledBack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_added_means_nothing_to_destroy() {
        assert!(blockages_to_destroy(Baseline { blockages: 3 }, 3).is_empty());
    }

    #[test]
    fn destruction_runs_highest_index_first() {
        // ⚠️ The whole point. Forwards would destroy index 3, shifting 4 and 5 down, and then
        // ask for an index that no longer exists.
        assert_eq!(blockages_to_destroy(Baseline { blockages: 3 }, 6), vec![5, 4, 3]);
    }

    #[test]
    fn a_baseline_of_zero_removes_everything() {
        assert_eq!(blockages_to_destroy(Baseline { blockages: 0 }, 2), vec![1, 0]);
    }

    #[test]
    fn a_shrunken_count_asks_for_nothing_rather_than_producing_garbage_indices() {
        // Should not happen, but a rollback path is the worst place to find out. The risk is not
        // a panic -- an inverted Rust range is empty -- it is asking to destroy indices that were
        // never ours, which is why the endpoints must not be swapped.
        assert!(blockages_to_destroy(Baseline { blockages: 5 }, 2).is_empty());
    }

    #[test]
    fn only_success_commits() {
        assert_eq!(settles_as(true), Settlement::Committed);
        assert_eq!(settles_as(false), Settlement::RolledBack);
    }
}
