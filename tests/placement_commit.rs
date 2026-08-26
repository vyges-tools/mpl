// SPDX-License-Identifier: Apache-2.0
//! Writing the placement back to the database.

use vyges_mpl::placement::{
    commit_macro, final_commit, needs_halo_blockage, FinalCommit, HaloKind, MacroCommit,
};

/// ⛔ **Orientation before location.** Setting the orientation mirrors the macro about an axis,
/// which moves its lower-left corner — so the location must be written afterwards to put it back.
/// The other order leaves every flipped macro misplaced by its own width or height.
#[test]
fn orientation_is_written_before_location() {
    let got = commit_macro(false, (1000, 2000)).expect("an unfixed macro is written");
    assert!(got.orientation_first);
    assert_eq!(got.location, (1000, 2000));
}

/// ⚠️ **`PLACED`, not `LOCKED`** — orientation improvement runs next and needs the macros movable.
#[test]
fn the_first_write_does_not_lock_the_macro() {
    let got = commit_macro(false, (0, 0)).unwrap();
    assert!(!got.locked, "the lock comes later, in the final commit");
}

/// ⚠️ A fixed instance is skipped entirely — not written, not locked.
#[test]
fn a_fixed_instance_is_not_written() {
    assert_eq!(commit_macro(true, (1000, 2000)), None);
}

/// ⚠️ Every field of the write is decided together, so the whole record is pinned.
#[test]
fn the_write_is_one_record() {
    assert_eq!(
        commit_macro(false, (7, 9)),
        Some(MacroCommit { orientation_first: true, location: (7, 9), locked: false })
    );
}

// ---------------------------------------------------------------- the final commit

/// ⛔ **A SOFT halo gets NO blockage** — other tools capable of placement are already aware of
/// them, so one would be redundant.
#[test]
fn a_soft_halo_casts_no_blockage() {
    assert!(!needs_halo_blockage(HaloKind::Soft));
    assert!(needs_halo_blockage(HaloKind::Hard));
    assert!(needs_halo_blockage(HaloKind::None), "no halo still gets a blockage");
}

/// ⛔ **The blockage is created for EVERY macro, FIXED OR NOT.** The `isFixed` test guards only the
/// snap and the lock; the blockage block sits outside it. So a fixed macro is never snapped and
/// never locked, and still casts a blockage.
#[test]
fn a_fixed_macro_is_not_snapped_but_still_casts_a_blockage() {
    assert_eq!(
        final_commit(true, HaloKind::Hard),
        FinalCommit { snapped: false, locked: false, blockage: true }
    );
}

/// ⚠️ An ordinary macro is snapped, locked, and casts its blockage.
#[test]
fn an_unfixed_macro_is_snapped_locked_and_blocked() {
    assert_eq!(
        final_commit(false, HaloKind::None),
        FinalCommit { snapped: true, locked: true, blockage: true }
    );
}

/// ⚠️ The two decisions are independent: a soft-haloed unfixed macro is snapped and locked but
/// casts nothing.
#[test]
fn snapping_and_blocking_are_independent_decisions() {
    assert_eq!(
        final_commit(false, HaloKind::Soft),
        FinalCommit { snapped: true, locked: true, blockage: false }
    );
    assert_eq!(
        final_commit(true, HaloKind::Soft),
        FinalCommit { snapped: false, locked: false, blockage: false }
    );
}
