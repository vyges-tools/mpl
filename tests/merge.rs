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
    c.metrics = Metrics { num_std_cell: std, num_macro: mac , ..Default::default() };
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
    assert_eq!(r.metrics, Metrics { num_std_cell: 7, num_macro: 1 , ..Default::default() });
    assert_eq!(r.leaf_std_cells, vec![1, 2, 3]);
    assert_eq!(r.leaf_macros, vec![9]);
    assert_eq!(r.db_modules, vec![7]);
}

#[test]
fn a_receiver_with_children_takes_the_incomers_leaves_AND_adopts_it() {
    // ⛔ Upstream `Cluster::attemptMerge` appends the incomer's leaves and modules to the
    // receiver unconditionally, and only then, when the receiver has children, adopts the
    // incomer as one of them too. Both — so the receiver's association claims the incomer's
    // instances while the tree keeps it as a child.
    let mut r = cl(1, "r", 3, 0);
    r.children.push(cl(5, "kid", 1, 0));
    let mut i = cl(2, "i", 4, 0);
    i.leaf_std_cells = vec![3];
    i.db_modules = vec![7];
    let dissolved = merge_into(&mut r, i);
    assert!(!dissolved, "adopted, not dissolved");
    assert_eq!(r.children.len(), 2);
    assert_eq!(r.children[1].leaf_std_cells, vec![3], "the adopted child keeps its leaves");
    assert_eq!(r.leaf_std_cells, vec![3], "and the receiver has them too");
    assert_eq!(r.db_modules, vec![7]);
    assert_eq!(r.metrics.num_std_cell, 7);
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

// ------------------------------------------------------------------ the loop

use vyges_mpl::merge::{merge_children_below_thresholds, ImpossibleMerge, MergeCtx};

/// A parent holding the given children.
fn parent_of(children: Vec<Cluster>) -> Cluster {
    let mut p = Cluster::new(0, "parent");
    p.children = children;
    p
}

/// Connections that every round rebuilds the same, for a loop that should converge on structure
/// alone; no IO clusters; the engine's post-merge bookkeeping is a no-op.
struct Fixed {
    c: Connections,
    rebuilds: usize,
}

impl Fixed {
    fn new(c: Connections) -> Self {
        Self { c, rebuilds: 0 }
    }
}

impl MergeCtx for Fixed {
    fn rebuild_connections(&mut self) -> Connections {
        self.rebuilds += 1;
        self.c.clone()
    }
    fn is_io_cluster(&self, _: i32) -> bool {
        false
    }
    fn merged(&mut self, _: &mut Cluster, _: i32, _: bool) {}
}

#[test]
fn no_small_children_means_no_rounds() {
    let mut p = parent_of(vec![cl(1, "a", 100, 0)]);
    let r = merge_children_below_thresholds(
        &mut p, vec![], &mut Fixed::new(Connections::new()), 5, 5, 100, 100,
    );
    assert_eq!(r.rounds, 0);
    assert!(r.merged.is_empty());
}

#[test]
fn a_round_that_merges_nothing_ends_the_loop() {
    // ⚠️ Getting "nothing matches" takes care, and my first attempt did not: two isolated
    // ONE-CELL clusters are DUST, so type 3 merges them happily. To reach the exit test the
    // clusters must be small (below the minimum) yet NOT dust -- 20 cells against a minimum of
    // 50 and a dust limit of 10 -- and unconnected, so types 1 and 2 cannot fire either.
    // Without the exit test this loops forever.
    let mut p = parent_of(vec![cl(1, "a", 20, 0), cl(2, "b", 20, 0)]);
    let r = merge_children_below_thresholds(
        &mut p, vec![1, 2], &mut Fixed::new(Connections::new()), 50, 5, 100, 100,
    );
    assert_eq!(r.rounds, 1, "one round, then it gives up");
    assert!(r.merged.is_empty());
    assert_eq!(p.children.len(), 2, "both survive");
}

#[test]
fn a_small_cluster_merges_into_its_single_well_formed_neighbour() {
    // Type 1. Cluster 1 is small; 2 is well-formed (not in the small list) and strongly connected.
    let mut p = parent_of(vec![cl(1, "small", 1, 0), cl(2, "big", 50, 0)]);
    let mut conns = Connections::new();
    conns.connect(1, 2, 5.0);
    let r = merge_children_below_thresholds(
        &mut p, vec![1], &mut Fixed::new(conns), 5, 5, 100, 100,
    );
    assert_eq!(r.merged, vec![(2, 1)], "the WELL-FORMED cluster is the receiver");
    assert_eq!(p.children.len(), 1);
    assert_eq!(p.children[0].name, "big||small");
}

#[test]
fn type_1_is_skipped_when_the_merge_would_break_a_maximum() {
    let mut p = parent_of(vec![cl(1, "small", 4, 0), cl(2, "big", 50, 0)]);
    let mut conns = Connections::new();
    conns.connect(1, 2, 5.0);
    let r = merge_children_below_thresholds(
        &mut p, vec![1], &mut Fixed::new(conns), 5, 5, 50, 5,
    );
    assert!(r.merged.is_empty(), "4 + 50 exceeds the 50-cell maximum");
    assert_eq!(p.children.len(), 2);
}

#[test]
fn siblings_with_the_same_signature_merge_when_type_1_does_not_apply() {
    // Type 2. Both are small (so neither is well-formed for the other) and both connect to 10.
    let mut p = parent_of(vec![cl(1, "a", 1, 0), cl(2, "b", 1, 0), cl(10, "hub", 90, 0)]);
    let mut conns = Connections::new();
    conns.connect(1, 10, 5.0);
    conns.connect(2, 10, 5.0);
    let r = merge_children_below_thresholds(
        &mut p, vec![1, 2], &mut Fixed::new(conns), 5, 5, 100, 100,
    );
    // ⚠️ Type 1 fires first: each small cluster has exactly one well-formed neighbour (10).
    // That is the correct upstream behaviour -- type 2 only sees what type 1 left.
    assert!(!r.merged.is_empty());
    assert!(r.merged.iter().all(|&(recv, _)| recv == 10), "both went to the hub: {:?}", r.merged);
}

#[test]
fn type_1_takes_precedence_over_type_2() {
    // 🔑 The order IS the algorithm. A cluster absorbed by type 1 is no longer available to
    // type 2, so a test that only checks the end state cannot tell the two apart.
    let mut p = parent_of(vec![cl(1, "a", 1, 0), cl(2, "b", 1, 0), cl(10, "hub", 90, 0)]);
    let mut conns = Connections::new();
    conns.connect(1, 10, 5.0);
    conns.connect(2, 10, 5.0);
    let r = merge_children_below_thresholds(
        &mut p, vec![1, 2], &mut Fixed::new(conns), 5, 5, 100, 100,
    );
    assert!(
        !r.merged.iter().any(|&(recv, _)| recv == 1 || recv == 2),
        "no small cluster received another: {:?}",
        r.merged
    );
}

#[test]
fn dust_absorbs_dust_when_nothing_else_applies() {
    // Type 3. Two isolated dust clusters -- no connections at all, so types 1 and 2 cannot fire.
    let mut p = parent_of(vec![cl(1, "d1", 2, 0), cl(2, "d2", 3, 0)]);
    let r = merge_children_below_thresholds(
        &mut p, vec![1, 2], &mut Fixed::new(Connections::new()), 50, 5, 100, 100,
    );
    assert_eq!(r.merged, vec![(1, 2)], "the earlier one receives");
    assert_eq!(p.children.len(), 1);
    assert_eq!(p.children[0].name, "d1||d2");
}

#[test]
fn a_non_dust_receiver_does_not_absorb_dust() {
    // ⚠️ The hole mutation testing found. The existing test only exercised a non-dust INCOMER;
    // the check on the RECEIVER was untested, so removing it changed nothing observable.
    // Here cluster 1 is small (20 < 50) but NOT dust (20 > 10), and 2 is dust. Upstream only
    // lets DUST absorb dust, so nothing should happen.
    let mut p = parent_of(vec![cl(1, "notdust", 20, 0), cl(2, "dust", 2, 0)]);
    let r = merge_children_below_thresholds(
        &mut p, vec![1, 2], &mut Fixed::new(Connections::new()), 50, 5, 100, 100,
    );
    assert!(r.merged.is_empty(), "a non-dust cluster does not absorb dust");
    assert_eq!(p.children.len(), 2);
}

#[test]
fn a_cluster_with_a_macro_is_not_dust_and_is_left_alone() {
    let mut p = parent_of(vec![cl(1, "d", 2, 0), cl(2, "hasmacro", 2, 1)]);
    let r = merge_children_below_thresholds(
        &mut p, vec![1, 2], &mut Fixed::new(Connections::new()), 50, 5, 100, 100,
    );
    assert!(r.merged.is_empty(), "a macro is never negligible");
    assert_eq!(p.children.len(), 2);
}

#[test]
fn merging_preserves_sibling_order() {
    // ⚠️ Order is observable downstream, so removal must not swap. With `swap_remove` the
    // surviving children would come back in a different order.
    let mut p = parent_of(vec![
        cl(1, "d1", 2, 0),
        cl(2, "d2", 2, 0),
        cl(3, "keep", 90, 0),
        cl(4, "last", 91, 0),
    ]);
    merge_children_below_thresholds(
        &mut p, vec![1, 2], &mut Fixed::new(Connections::new()), 50, 5, 100, 100,
    );
    let names: Vec<&str> = p.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["d1||d2", "keep", "last"], "the tail kept its order");
}

#[test]
fn the_loop_runs_again_after_a_successful_merge() {
    // Three dust clusters: the first round absorbs both into the first, then the next round
    // finds nothing left to do and stops.
    let mut p = parent_of(vec![cl(1, "a", 1, 0), cl(2, "b", 1, 0), cl(3, "c", 1, 0)]);
    let r = merge_children_below_thresholds(
        &mut p, vec![1, 2, 3], &mut Fixed::new(Connections::new()), 50, 5, 100, 100,
    );
    assert_eq!(p.children.len(), 1, "all three ended up together");
    assert!(r.rounds >= 1);
}

#[test]
fn connections_are_rebuilt_every_round() {
    // ⚠️ Cluster ids change as clusters merge, so a map built once and reused would connect
    // clusters that no longer exist.
    let mut p = parent_of(vec![cl(1, "a", 1, 0), cl(2, "b", 1, 0)]);
    let mut ctx = Fixed::new(Connections::new());
    let r = merge_children_below_thresholds(&mut p, vec![1, 2], &mut ctx, 50, 5, 100, 100);
    assert_eq!(ctx.rebuilds, r.rounds, "rebuilt once per round");
}

#[test]
fn a_well_formed_neighbour_that_is_not_a_sibling_is_silently_skipped() {
    // ⚠️ Upstream's attemptMerge returns false on differing parents and type 1 does NOT treat
    // that as an error -- unlike types 2 and 3, whose failures are critical.
    let mut p = parent_of(vec![cl(1, "small", 1, 0)]);
    let mut conns = Connections::new();
    conns.connect(1, 99, 5.0); // 99 is not a child of this parent
    let r = merge_children_below_thresholds(
        &mut p, vec![1], &mut Fixed::new(conns), 5, 5, 100, 100,
    );
    assert!(r.merged.is_empty());
    assert!(r.impossible.is_empty(), "not an error, just no merge");
    assert_eq!(p.children.len(), 1);
}

#[test]
fn no_impossible_merges_arise_in_ordinary_operation() {
    // ⛔ A non-empty `impossible` list means an invariant broke, not a design we cannot handle.
    let mut p = parent_of(vec![cl(1, "a", 1, 0), cl(2, "b", 1, 0), cl(10, "hub", 90, 0)]);
    let mut conns = Connections::new();
    conns.connect(1, 10, 5.0);
    conns.connect(2, 10, 5.0);
    let r = merge_children_below_thresholds(
        &mut p, vec![1, 2], &mut Fixed::new(conns), 5, 5, 100, 100,
    );
    assert_eq!(r.impossible, Vec::<ImpossibleMerge>::new());
}

// ------------------------------------------------------------------ connections edited mid-round

#[test]
fn a_dissolved_incomers_neighbours_re_point_to_the_receiver_one_way() {
    // 🔑 Upstream `ClusteringEngine::attemptMerge`: once the incomer dissolves, every cluster in
    // its (cached) map forgets it and gains that weight towards the receiver — ONE way. The
    // receiver forgets the incomer and gains nothing. Later merges in the round read that state.
    //
    // X is well-formed, A and D are small. A (20 to X, 1 to Q) merges into X first. D's only
    // link is 1 to X, and whether it is strong depends on X's total AFTER A left:
    //   upstream  -- X keeps D 1 + W 11 = 12  ->  1 / (1 + 12 - 1) = 0.083 >= 0.08: D merges.
    //   mutations -- X keeping A's 20, or linking to itself (+20), or gaining Q back (+1) all
    //                push X past 12, the ratio under 0.08, and D stays.
    let mut p = parent_of(vec![
        cl(1, "A", 3, 0),
        cl(2, "D", 20, 0),
        cl(10, "X", 90, 0),
        cl(11, "W", 90, 0),
        cl(12, "Q", 90, 0),
    ]);
    let mut c = Connections::new();
    c.connect(1, 10, 20.0);
    c.connect(1, 12, 1.0);
    c.connect(2, 10, 1.0);
    c.connect(10, 11, 11.0);
    let r = merge_children_below_thresholds(&mut p, vec![1, 2], &mut Fixed::new(c), 50, 5, 1000, 100);
    assert_eq!(r.merged[..2], [(10, 1), (10, 2)], "A, then D, into X in the same round: {:?}", r.merged);
}

#[test]
fn a_signature_is_read_from_the_connections_as_this_rounds_merges_left_them() {
    // A merges into X in type 1. Before that, B's neighbours are {A, Y, Z} and C's {X, Y, Z};
    // A dissolving re-points B to X, so by type 2 they share a signature and merge.
    // ⛔ Scoring the round against the map it started with — the earlier reading — merges nothing.
    let mut p = parent_of(vec![
        cl(1, "A", 3, 0),
        cl(2, "B", 20, 0),
        cl(3, "C", 20, 0),
        cl(10, "X", 90, 0),
        cl(11, "Y", 90, 0),
        cl(12, "Z", 90, 0),
    ]);
    let mut c = Connections::new();
    c.connect(1, 10, 1.0);
    c.connect(2, 1, 1.0);
    c.connect(2, 11, 1.0);
    c.connect(2, 12, 1.0);
    c.connect(3, 10, 1.0);
    c.connect(3, 11, 1.0);
    c.connect(3, 12, 1.0);
    let r = merge_children_below_thresholds(&mut p, vec![1, 2, 3], &mut Fixed::new(c), 50, 5, 1000, 100);
    assert!(r.merged.contains(&(10, 1)), "A into X: {:?}", r.merged);
    assert!(r.merged.contains(&(2, 3)), "then B and C by signature: {:?}", r.merged);
}
