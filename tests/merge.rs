// SPDX-License-Identifier: Apache-2.0
//! Merging small clusters. Rules from upstream `mergeChildrenBelowThresholds` and its helpers.
use vyges_mpl::cluster::{Cluster, ClusterType, Metrics};
use vyges_mpl::merge::{
    all_connections_weight, find_neighbors, find_single_well_formed_connected_cluster, is_dust,
    merge_honors_max_thresholds, merge_into, same_connection_signature, strong_connection,
};
use vyges_mpl::netlist::Connections;

fn cl(id: i32, name: &str, std: i32, mac: i32) -> Cluster {
    let mut c = Cluster::new(id, name);
    c.metrics = Metrics { num_std_cell: std, num_macro: mac };
    c
}

fn never_io(_: i32) -> bool {
    false
}

// ------------------------------------------------------------------ strong_connection

#[test]
fn the_shared_connection_is_subtracted_from_the_denominator_once() {
    // ⚠️ The connection appears in BOTH clusters' totals, so `a_total + b_total` double-counts it.
    // Upstream removes one copy before dividing.
    //
    // Finding a case that SEPARATES the two formulas takes care, because most numbers agree.
    // With weight(1,2) = 1 and each side carrying 5.6 more:
    //     with the subtraction: 1 / (6.6 + 6.6 - 1) = 0.0820  >= 0.08  -> strong
    //     without it:           1 / (6.6 + 6.6)     = 0.0758  <  0.08  -> not strong
    // A test built on numbers where both agree would pass either way and prove nothing.
    let mut c = Connections::new();
    c.connect(1, 2, 1.0);
    c.connect(1, 3, 5.6);
    c.connect(2, 4, 5.6);
    assert!(strong_connection(&c, 1, 2), "strong only if the shared weight is subtracted");
}

#[test]
fn a_pair_with_no_connection_is_not_strongly_connected() {
    let c = Connections::new();
    assert!(!strong_connection(&c, 1, 2));
}

#[test]
fn a_weak_connection_is_below_the_ratio() {
    let mut c = Connections::new();
    c.connect(1, 2, 1.0);
    c.connect(1, 3, 100.0);
    assert!(!strong_connection(&c, 1, 2), "1 against ~100 is far below 8%");
}

#[test]
fn a_sole_connection_is_always_strong() {
    // total = w + w - w = w, so the ratio is exactly 1.
    let mut c = Connections::new();
    c.connect(1, 2, 0.5);
    assert!(strong_connection(&c, 1, 2));
}

// ------------------------------------------------------------------ find_neighbors

#[test]
fn neighbors_use_the_targets_own_total_not_the_pairs() {
    // 🔑 The asymmetry. strong_connection divides by (a + b - shared); find_neighbors divides by
    // the target's total alone, with no subtraction. Unifying them changes which clusters merge.
    let mut c = Connections::new();
    c.connect(1, 2, 1.0);
    c.connect(1, 3, 9.0);
    // target 1 total = 10; 1->2 ratio = 0.10 >= 0.08 -> a neighbour.
    assert_eq!(find_neighbors(&c, 1, 99), vec![2, 3]);
    // But as a PAIR: 1/(10 + 1 - 1) = 0.10 -- also strong here; the denominators differ in
    // general, which the next assertion shows.
    let mut d = Connections::new();
    d.connect(1, 2, 1.0);
    d.connect(1, 3, 9.0);
    d.connect(2, 4, 40.0);
    assert_eq!(find_neighbors(&d, 1, 99), vec![2, 3], "target 1's view is unchanged by 2's load");
    assert!(!strong_connection(&d, 1, 2), "but as a pair it is now weak: 1/(10+41-1)");
}

#[test]
fn the_ignored_cluster_is_excluded() {
    let mut c = Connections::new();
    c.connect(1, 2, 5.0);
    c.connect(1, 3, 5.0);
    assert_eq!(find_neighbors(&c, 1, 2), vec![3]);
}

#[test]
fn a_cluster_with_no_connections_has_no_neighbors() {
    assert!(find_neighbors(&Connections::new(), 1, 99).is_empty());
}

// ------------------------------------------------------------------ signature

#[test]
fn two_clusters_with_the_same_neighbours_share_a_signature() {
    let mut c = Connections::new();
    c.connect(1, 10, 5.0);
    c.connect(1, 11, 5.0);
    c.connect(2, 10, 5.0);
    c.connect(2, 11, 5.0);
    assert!(same_connection_signature(&c, 1, 2));
}

#[test]
fn each_others_connection_is_ignored_when_comparing() {
    // They may be connected to each other; that link is excluded from both signatures.
    let mut c = Connections::new();
    c.connect(1, 2, 5.0);
    c.connect(1, 10, 5.0);
    c.connect(2, 10, 5.0);
    assert!(same_connection_signature(&c, 1, 2));
}

#[test]
fn different_neighbours_are_not_the_same_signature() {
    let mut c = Connections::new();
    c.connect(1, 10, 5.0);
    c.connect(2, 11, 5.0);
    assert!(!same_connection_signature(&c, 1, 2));
}

#[test]
fn two_isolated_clusters_do_NOT_share_a_signature() {
    // ⚠️ Both have the empty neighbour set, which upstream refuses deliberately -- merging on
    // "both connected to nothing" would pull unrelated logic together.
    let c = Connections::new();
    assert!(!same_connection_signature(&c, 1, 2));
}

// ------------------------------------------------------------------ thresholds

#[test]
fn a_merge_landing_exactly_on_a_maximum_is_allowed() {
    // ⚠️ `<=`, not `<`.
    let a = cl(1, "a", 5, 1);
    let b = cl(2, "b", 5, 1);
    assert!(merge_honors_max_thresholds(&a, &b, 10, 2));
    assert!(!merge_honors_max_thresholds(&a, &b, 9, 2), "one over the cell maximum");
    assert!(!merge_honors_max_thresholds(&a, &b, 10, 1), "one over the macro maximum");
}

#[test]
fn threshold_checks_use_the_type_masked_counts() {
    // A HardMacro cluster reports zero standard cells, so a merge involving one is judged on
    // its macros alone.
    let mut a = cl(1, "a", 500, 1);
    a.cluster_type = ClusterType::HardMacro;
    let b = cl(2, "b", 5, 0);
    assert!(merge_honors_max_thresholds(&a, &b, 10, 2), "a's 500 cells are masked away");
}

// ------------------------------------------------------------------ the single candidate

#[test]
fn exactly_one_well_formed_candidate_is_required() {
    // 🔑 Two candidates is not "pick the strongest" -- upstream declines, because a cluster
    // pulled equally by two neighbours has no obviously right home.
    let mut c = Connections::new();
    c.connect(1, 10, 5.0);
    assert_eq!(find_single_well_formed_connected_cluster(&c, 1, &[], &never_io), Some(10));
    c.connect(1, 11, 5.0);
    assert_eq!(
        find_single_well_formed_connected_cluster(&c, 1, &[], &never_io),
        None,
        "two candidates means no answer"
    );
}

#[test]
fn a_small_candidate_is_not_well_formed() {
    // ⚠️ Merging into another small cluster would only move the problem.
    let mut c = Connections::new();
    c.connect(1, 10, 5.0);
    assert_eq!(find_single_well_formed_connected_cluster(&c, 1, &[10], &never_io), None);
}

#[test]
fn an_io_cluster_is_never_the_candidate() {
    let mut c = Connections::new();
    c.connect(1, 10, 5.0);
    let io = |id: i32| id == 10;
    assert_eq!(find_single_well_formed_connected_cluster(&c, 1, &[], &io), None);
}

#[test]
fn a_weakly_connected_candidate_does_not_count() {
    let mut c = Connections::new();
    c.connect(1, 10, 1.0);
    c.connect(1, 11, 200.0);
    // 11 is the only strong one; 10 is far below the ratio.
    assert_eq!(find_single_well_formed_connected_cluster(&c, 1, &[], &never_io), Some(11));
}

// ------------------------------------------------------------------ the merge itself

#[test]
fn merging_joins_names_with_a_double_pipe() {
    // 🔑 Observable in upstream's own hierarchy dump, so reproduced rather than tidied.
    let mut r = cl(1, "recv", 3, 0);
    let dissolved = merge_into(&mut r, cl(2, "in", 4, 1));
    assert_eq!(r.name, "recv||in");
    assert!(dissolved);
}

#[test]
fn merging_sums_the_metrics_and_concatenates_the_leaves() {
    let mut r = cl(1, "r", 3, 0);
    r.leaf_std_cells = vec![1, 2];
    let mut i = cl(2, "i", 4, 1);
    i.leaf_std_cells = vec![3];
    i.leaf_macros = vec![9];
    i.db_modules = vec![7];
    merge_into(&mut r, i);
    assert_eq!(r.metrics, Metrics { num_std_cell: 7, num_macro: 1 });
    assert_eq!(r.leaf_std_cells, vec![1, 2, 3]);
    assert_eq!(r.leaf_macros, vec![9]);
    assert_eq!(r.db_modules, vec![7]);
}

#[test]
fn a_receiver_with_children_ADOPTS_the_incomer_instead_of_dissolving_it() {
    // ⚠️ A cluster with children cannot absorb another's leaves without losing the structure its
    // own children describe, so the incomer becomes another child.
    let mut r = cl(1, "r", 3, 0);
    r.children.push(cl(5, "kid", 1, 0));
    let mut i = cl(2, "i", 4, 0);
    i.leaf_std_cells = vec![3];
    let dissolved = merge_into(&mut r, i);
    assert!(!dissolved, "adopted, not dissolved");
    assert_eq!(r.children.len(), 2);
    assert!(r.leaf_std_cells.is_empty(), "its leaves did NOT move to the receiver");
    assert_eq!(r.metrics.num_std_cell, 7, "but the metrics still accumulate");
}

// ------------------------------------------------------------------ dust

#[test]
fn dust_is_a_few_cells_and_no_macros() {
    assert!(is_dust(&cl(1, "d", 10, 0)), "ten cells is still dust");
    assert!(!is_dust(&cl(1, "d", 11, 0)), "eleven is not");
    // ⚠️ A single macro disqualifies it however few cells it has -- a macro is never negligible.
    assert!(!is_dust(&cl(1, "d", 1, 1)));
}

#[test]
fn all_connections_weight_sums_every_link() {
    let mut c = Connections::new();
    c.connect(1, 2, 1.5);
    c.connect(1, 3, 2.5);
    assert_eq!(all_connections_weight(&c, 1), 4.0);
    assert_eq!(all_connections_weight(&c, 99), 0.0);
}
