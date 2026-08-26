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
