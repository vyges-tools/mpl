// SPDX-License-Identifier: Apache-2.0
//! Orientation correction: which macros may be flipped together, and when a flip is kept.

use vyges_mpl::placement::{
    keep_flip, orientation_groups, orientation_strategy, OrientationStrategy, FLIP_PASSES,
};

/// ⛔ **The branch reads backwards.** Pin-aware halos — `use_full_halo` FALSE — take the
/// RESTRICTED by-cluster path, because flipping a single macro inside a cluster could leave part of
/// it unreachable. A full halo has no such worry and flips each macro alone.
#[test]
fn pin_aware_halos_take_the_restricted_path() {
    assert_eq!(orientation_strategy(false), OrientationStrategy::ByCluster);
    assert_eq!(orientation_strategy(true), OrientationStrategy::Single);
}

/// ⚠️ **`>`, strictly — so a TIE KEEPS THE FLIP.** The flip happens first and is undone only when
/// it made things strictly worse.
#[test]
fn an_equal_wirelength_keeps_the_flip() {
    assert!(keep_flip(100.0, 100.0), "a tie keeps it");
    assert!(keep_flip(100.0, 99.0), "an improvement keeps it");
    assert!(!keep_flip(100.0, 100.001), "any worsening reverts it");
}

/// ⛔ **Two full passes, vertical then horizontal** — not two flips per macro. A macro's horizontal
/// trial is measured against a board on which every other macro's vertical decision has already
/// been made.
#[test]
fn the_passes_are_vertical_then_horizontal() {
    assert_eq!(FLIP_PASSES, [true, false]);
}

/// 🔑 **A macro belongs to BOTH a column and a row**, so it is flipped as part of one group in the
/// vertical pass and a different group in the horizontal one.
#[test]
fn a_macro_is_in_both_a_column_and_a_row() {
    // A 2 x 2 array: ids 0..3 at (0,0), (100,0), (0,100), (100,100).
    let macros = [(0usize, (0, 0)), (1, (100, 0)), (2, (0, 100)), (3, (100, 100))];
    let (cols, rows) = orientation_groups(&macros);
    assert_eq!(cols, vec![vec![0, 2], vec![1, 3]], "grouped by x");
    assert_eq!(rows, vec![vec![0, 1], vec![2, 3]], "grouped by y");
}

/// ⚠️ **The groups come out in ascending coordinate order**, because upstream's container is a
/// `std::map` — not in the order the macros were listed.
#[test]
fn groups_come_out_in_ascending_coordinate_order() {
    let macros = [(0usize, (500, 500)), (1, (100, 100)), (2, (300, 300))];
    let (cols, rows) = orientation_groups(&macros);
    assert_eq!(cols, vec![vec![1], vec![2], vec![0]], "x = 100, 300, 500");
    assert_eq!(rows, vec![vec![1], vec![2], vec![0]]);
}

/// ⚠️ A single macro is its own column and its own row.
#[test]
fn a_lone_macro_is_its_own_column_and_row() {
    let (cols, rows) = orientation_groups(&[(7usize, (10, 20))]);
    assert_eq!(cols, vec![vec![7]]);
    assert_eq!(rows, vec![vec![7]]);
}

/// ℹ️ A cluster with no macros produces no groups, and neither pass has anything to do.
#[test]
fn no_macros_gives_no_groups() {
    let (cols, rows) = orientation_groups(&[]);
    assert!(cols.is_empty());
    assert!(rows.is_empty());
}

/// ⚠️ **A whole row sharing one y is flipped together**, however wide — the grouping is by exact
/// coordinate, so a macro one unit off is in a group of its own.
#[test]
fn a_macro_one_unit_off_forms_its_own_group() {
    let macros = [(0usize, (0, 100)), (1, (200, 100)), (2, (400, 101))];
    let (_, rows) = orientation_groups(&macros);
    assert_eq!(rows, vec![vec![0, 1], vec![2]], "no tolerance at all");
}
