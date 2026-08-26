// SPDX-License-Identifier: Apache-2.0
//! Whether the boundary push runs at all, and which macro clusters it considers.

use vyges_mpl::placement::{
    fetch_macro_clusters, has_single_centralized_macro_array, push_decision, AreaKind, NoPush,
};

/// A small tree: id -> (kind, children).
fn tree() -> Vec<(AreaKind, Vec<usize>)> {
    vec![
        // 0: root, mixed
        (AreaKind::MixedCluster, vec![1, 2, 3]),
        // 1: a macro cluster
        (AreaKind::HardMacroCluster, vec![]),
        // 2: a mixed cluster holding another macro cluster
        (AreaKind::MixedCluster, vec![4]),
        // 3: a std cell cluster hiding a macro cluster
        (AreaKind::StdCellCluster, vec![5]),
        (AreaKind::HardMacroCluster, vec![]),
        (AreaKind::HardMacroCluster, vec![]),
    ]
}

/// 🔑 **Depth-first in child order**, descending through mixed clusters.
#[test]
fn macro_clusters_are_fetched_depth_first_through_mixed_clusters() {
    let t = tree();
    let kind_of = |i: usize| t[i].0;
    let children_of = |i: usize| t[i].1.clone();
    assert_eq!(fetch_macro_clusters(0, &kind_of, &children_of), vec![1, 4]);
}

/// ⚠️ **A standard-cell cluster is NOT descended into**, so a macro cluster underneath one is
/// never fetched. Nothing builds that shape today; the restriction is the reference's.
#[test]
fn a_macro_cluster_under_a_std_cell_cluster_is_never_fetched() {
    let t = tree();
    let kind_of = |i: usize| t[i].0;
    let children_of = |i: usize| t[i].1.clone();
    let got = fetch_macro_clusters(0, &kind_of, &children_of);
    assert!(!got.contains(&5), "cluster 5 is under a std cell cluster: {got:?}");
}

// ---------------------------------------------------------------- the centralized array test

/// 🔑 **The arrangement `singleArraySingleStdCellCluster` produces**: one macro array, and a
/// standard-cell cluster shrunk to nothing. The push then declines — the array is already placed.
#[test]
fn one_array_beside_a_shrunk_cell_cluster_is_centralized() {
    let children = [(AreaKind::HardMacroCluster, 40_000i64), (AreaKind::StdCellCluster, 0)];
    assert!(has_single_centralized_macro_array(&children));
}

/// ⛔ **A standard-cell cluster is judged by its SOFT MACRO's area.** A cluster that was NOT shrunk
/// has a non-zero one and fails the test — which is what makes this the mirror of the shrinking
/// step rather than a general "is there one array" question.
#[test]
fn a_cell_cluster_that_was_not_shrunk_fails_the_test() {
    let children = [(AreaKind::HardMacroCluster, 40_000i64), (AreaKind::StdCellCluster, 1)];
    assert!(!has_single_centralized_macro_array(&children), "one square unit is enough to fail it");
}

/// ⚠️ **Any MIXED cluster fails it immediately.**
#[test]
fn a_mixed_cluster_fails_it_at_once() {
    let children = [(AreaKind::MixedCluster, 0i64), (AreaKind::HardMacroCluster, 1)];
    assert!(!has_single_centralized_macro_array(&children));
}

/// ⚠️ Two macro clusters is not a single array.
#[test]
fn two_macro_clusters_fail_it() {
    let children = [(AreaKind::HardMacroCluster, 1i64), (AreaKind::HardMacroCluster, 1)];
    assert!(!has_single_centralized_macro_array(&children));
}

/// ⚠️ **The count test is inside the loop**, so the second macro cluster fails it at once — before
/// a later child that would also have failed it is even seen.
#[test]
fn the_second_macro_cluster_fails_it_before_later_children_are_seen() {
    let children = [
        (AreaKind::HardMacroCluster, 1i64),
        (AreaKind::HardMacroCluster, 1),
        (AreaKind::StdCellCluster, 999),
    ];
    assert!(!has_single_centralized_macro_array(&children));
}

/// ⚠️ **IO clusters and fixed macros fall through the reference's `switch` without a case** and are
/// simply ignored.
#[test]
fn io_clusters_and_fixed_macros_are_ignored() {
    let children = [
        (AreaKind::IoCluster, 0i64),
        (AreaKind::FixedMacro, 999),
        (AreaKind::HardMacroCluster, 1),
        (AreaKind::StdCellCluster, 0),
    ];
    assert!(has_single_centralized_macro_array(&children), "neither one counts against it");
}

/// ℹ️ A root with no children passes vacuously — zero arrays counts as one.
#[test]
fn an_empty_root_passes_vacuously() {
    assert!(has_single_centralized_macro_array(&[]));
}

// ---------------------------------------------------------------- the two guards

/// ⚠️ A design that is nothing but macros is never pushed.
#[test]
fn an_all_macro_design_is_not_pushed() {
    assert_eq!(
        push_decision(AreaKind::HardMacroCluster, &[]),
        Err(NoPush::DesignIsAllMacros)
    );
}

/// ⚠️ The all-macros test comes FIRST, so it wins even when the array test would also have fired.
#[test]
fn the_all_macro_guard_is_checked_first() {
    let children = [(AreaKind::HardMacroCluster, 1i64), (AreaKind::StdCellCluster, 0)];
    assert_eq!(
        push_decision(AreaKind::HardMacroCluster, &children),
        Err(NoPush::DesignIsAllMacros),
        "not SingleCentralizedMacroArray"
    );
}

/// 🔑 An ordinary design with two macro clusters is pushed.
#[test]
fn an_ordinary_design_is_pushed() {
    let children = [
        (AreaKind::HardMacroCluster, 1i64),
        (AreaKind::HardMacroCluster, 1),
        (AreaKind::StdCellCluster, 999),
    ];
    assert_eq!(push_decision(AreaKind::MixedCluster, &children), Ok(()));
}

// ---------------------------------------------------------------- the push itself

use vyges_mpl::halo::Boundary;
use vyges_mpl::placement::{
    distance_to_close_boundaries, move_towards_boundary, push_macro_cluster, PushAttempt,
};

const CORE: (i32, i32, i32, i32) = (0, 0, 1000, 1000);

/// 🔑 **At most one horizontal and one vertical boundary**, each the nearer of its pair.
#[test]
fn a_cluster_is_never_pushed_both_ways_on_one_axis() {
    // Near the bottom-left corner; macros 200 x 200.
    let got = distance_to_close_boundaries((50, 30, 250, 230), CORE, 200, 200);
    assert_eq!(got, vec![(Boundary::B, 30), (Boundary::L, 50)]);
}

/// ⚠️ **The threshold is the MACRO's own dimension**, not the cluster's — a cluster further away
/// than one macro is wide is left alone, which is what stops the push dragging it across the die.
#[test]
fn a_cluster_further_than_one_macro_is_left_alone() {
    // 300 from the left, macros only 200 wide.
    let got = distance_to_close_boundaries((300, 300, 500, 500), CORE, 200, 200);
    assert!(got.is_empty(), "{got:?}");

    // Widen the macro past the distance and it qualifies.
    let got = distance_to_close_boundaries((300, 300, 500, 500), CORE, 301, 301);
    assert_eq!(got.len(), 2);
}

/// ⛔ **The horizontal test uses the macro's WIDTH and the vertical its HEIGHT** — unlike the notch
/// thresholds, which are crossed. A SQUARE macro cannot tell the two readings apart, and a mutation
/// proved that every fixture here was square.
#[test]
fn each_axis_is_measured_against_its_own_macro_dimension() {
    // Macros 200 wide and 400 tall. The cluster is 300 from the left — further than one macro
    // WIDTH, so no horizontal push; but nearer than one macro HEIGHT, so the crossed reading
    // would push it.
    // Vertically it is centred at 450 from either edge, further than 400, so nothing there either.
    let got = distance_to_close_boundaries((300, 450, 500, 550), CORE, 200, 400);
    assert!(got.is_empty(), "the crossed reading would have pushed it left: {got:?}");
}

/// ⛔ **A tie goes to RIGHT, and to TOP.** A centred cluster is pushed towards the far edges.
#[test]
fn a_tie_goes_right_and_top() {
    // Exactly centred: 400 from every edge.
    let got = distance_to_close_boundaries((400, 400, 600, 600), CORE, 500, 500);
    assert_eq!(got, vec![(Boundary::T, 400), (Boundary::R, 400)]);
}

/// ⚠️ **Both distances are `abs`**, so a cluster already OUTSIDE the core reads as close to the
/// boundary it has passed — and is pushed further out rather than back in.
#[test]
fn a_cluster_outside_the_core_is_pushed_further_out() {
    // Its left edge is 50 past the core's left edge.
    let got = distance_to_close_boundaries((-50, 400, 150, 600), CORE, 200, 200);
    assert_eq!(got, vec![(Boundary::L, 50)], "50, not -50");
}

/// ⚠️ **The result comes out in enum order — B, L, T, R** — not in the order the two were decided.
#[test]
fn the_boundaries_come_out_in_enum_order() {
    let got = distance_to_close_boundaries((50, 30, 250, 230), CORE, 200, 200);
    let order: Vec<Boundary> = got.iter().map(|(b, _)| *b).collect();
    assert_eq!(order, vec![Boundary::B, Boundary::L], "B before L");

    // Top-right corner: T before R.
    let got = distance_to_close_boundaries((750, 770, 950, 970), CORE, 200, 200);
    let order: Vec<Boundary> = got.iter().map(|(b, _)| *b).collect();
    assert_eq!(order, vec![Boundary::T, Boundary::R]);

    // ⛔ **Top-LEFT is the corner that proves it is a SORT and not a reversal.** In the other three
    // corners the two happen to agree — the vertical boundary sorts ahead of the horizontal one and
    // is decided second, so reversing the decision order gives the same answer. Here `L` (1) sorts
    // ahead of `T` (2) while still being decided first, and only a sort keeps it there. A mutation
    // that reversed instead of sorting went straight through the other three.
    let got = distance_to_close_boundaries((50, 750, 250, 950), CORE, 200, 200);
    assert_eq!(got, vec![(Boundary::L, 50), (Boundary::T, 50)], "L before T");
}

/// ⚠️ `L` and `B` move negative, `R` and `T` positive.
#[test]
fn the_direction_comes_from_the_boundary_not_the_sign() {
    let b = (100, 100, 200, 200);
    assert_eq!(move_towards_boundary(b, Boundary::L, 50), (50, 100, 150, 200));
    assert_eq!(move_towards_boundary(b, Boundary::R, 50), (150, 100, 250, 200));
    assert_eq!(move_towards_boundary(b, Boundary::B, 50), (100, 50, 200, 150));
    assert_eq!(move_towards_boundary(b, Boundary::T, 50), (100, 150, 200, 250));
}

/// 🔑 **The two pushes COMPOSE** — the second is applied to the box the first left behind.
#[test]
fn the_two_pushes_compose() {
    let (moved, attempts) = push_macro_cluster(
        (50, 30, 250, 230),
        &[(Boundary::B, 30), (Boundary::L, 50)],
        &|_| false,
    );
    assert_eq!(moved, (0, 0, 200, 200), "into the corner");
    assert!(attempts.iter().all(|a| a.committed));
}

/// ⚠️ **A reverted push leaves the box exactly as it was**, and does not stop the next one.
///
/// ⛔ **The revert has to come FIRST for this to prove anything.** With the revert second, a
/// version that gave up after the first failure would behave identically — a mutation proved that
/// the original fixture here did exactly that.
#[test]
fn a_reverted_push_does_not_block_the_next() {
    // Anything moved DOWN overlaps; moving left does not. The bottom push is tried first.
    let overlaps = |b: (i32, i32, i32, i32)| b.1 < 30;
    let (moved, attempts) = push_macro_cluster(
        (50, 30, 250, 230),
        &[(Boundary::B, 30), (Boundary::L, 50)],
        &overlaps,
    );
    assert_eq!(moved, (0, 30, 200, 230), "the LEFT push still happened after the bottom failed");
    assert_eq!(
        attempts,
        vec![
            PushAttempt { boundary: Boundary::B, distance: 30, committed: false },
            PushAttempt { boundary: Boundary::L, distance: 50, committed: true },
        ]
    );
}

/// ⚠️ **A distance of zero is skipped, not attempted** — the cluster is already on that boundary,
/// and no trace line is emitted for it.
#[test]
fn a_zero_distance_is_skipped_entirely() {
    let (moved, attempts) = push_macro_cluster(
        (0, 30, 200, 230),
        &[(Boundary::B, 30), (Boundary::L, 0)],
        &|_| false,
    );
    assert_eq!(moved, (0, 0, 200, 200));
    assert_eq!(attempts.len(), 1, "only the bottom push was attempted");
    assert_eq!(attempts[0].boundary, Boundary::B);
}

/// ⚠️ **The trace records ATTEMPTS, not commits** — upstream prints its line before the overlap
/// test, so its log says "Moved X" for pushes it then undoes. Anyone scoring against the
/// `boundary_push` channel is scoring attempts.
#[test]
fn a_reverted_push_is_still_an_attempt() {
    let (_, attempts) =
        push_macro_cluster((50, 50, 250, 250), &[(Boundary::L, 50)], &|_| true);
    assert_eq!(attempts.len(), 1, "recorded even though it was undone");
    assert!(!attempts[0].committed);
}
