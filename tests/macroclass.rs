// SPDX-License-Identifier: Apache-2.0
//! Macro classification. Rules from upstream's three `classifyMacrosBy*` passes and the grouping.
use vyges_mpl::macroclass::{
    classify_by_conn_signature, classify_by_interconn, classify_by_size,
    group_single_macro_clusters, single_macro_grouping, GroupMerge, MacroSize,
};
use vyges_mpl::netlist::Connections;

fn sz(w: i64, h: i64) -> MacroSize {
    MacroSize { width: w, height: h }
}

// ------------------------------------------------------------------ size

#[test]
fn macros_of_identical_dimensions_share_a_size_class() {
    let sizes = [sz(10, 20), sz(30, 40), sz(10, 20)];
    assert_eq!(classify_by_size(&sizes), vec![0, 1, 0], "index 2 follows index 0");
}

#[test]
fn size_compares_width_AND_height_not_area() {
    // ⚠️ Upstream's HardMacro::operator== is width and height. Two macros of equal AREA but
    // different shape are NOT the same size, and grouping them would build an array that does
    // not tile.
    let sizes = [sz(10, 20), sz(20, 10)];
    assert_eq!(classify_by_size(&sizes), vec![0, 1], "same area, different shape, different class");
}

#[test]
fn an_unmatched_macro_represents_itself() {
    assert_eq!(classify_by_size(&[sz(1, 1), sz(2, 2), sz(3, 3)]), vec![0, 1, 2]);
}

#[test]
fn the_lowest_index_leads_a_size_group() {
    let sizes = [sz(5, 5), sz(5, 5), sz(5, 5)];
    assert_eq!(classify_by_size(&sizes), vec![0, 0, 0]);
}

// ------------------------------------------------------------------ signature

#[test]
fn macros_wired_to_the_same_neighbours_share_a_signature_class() {
    let mut c = Connections::new();
    c.connect(1, 99, 5.0);
    c.connect(2, 99, 5.0);
    assert_eq!(classify_by_conn_signature(&c, &[1, 2]), vec![0, 0]);
}

#[test]
fn macros_wired_differently_do_not() {
    let mut c = Connections::new();
    c.connect(1, 98, 5.0);
    c.connect(2, 99, 5.0);
    assert_eq!(classify_by_conn_signature(&c, &[1, 2]), vec![0, 1]);
}

#[test]
fn unconnected_macros_do_NOT_share_a_signature() {
    // ⚠️ An empty neighbour set is deliberately not a match -- otherwise every isolated macro in
    // the design would group into one array.
    let c = Connections::new();
    assert_eq!(classify_by_conn_signature(&c, &[1, 2, 3]), vec![0, 1, 2]);
}

// ------------------------------------------------------------------ interconnection

#[test]
fn strongly_wired_macros_share_an_interconnection_class() {
    let mut c = Connections::new();
    c.connect(1, 2, 5.0);
    assert_eq!(classify_by_interconn(&c, &[1, 2]), vec![0, 0]);
}

#[test]
fn unwired_macros_each_lead_their_own_class() {
    let c = Connections::new();
    assert_eq!(classify_by_interconn(&c, &[1, 2, 3]), vec![0, 1, 2]);
}

#[test]
fn a_macro_can_ADOPT_a_class_led_by_a_higher_index() {
    // 🔑 The interconnection pass scans ALL macros, not just later ones, and BREAKS on the first
    // neighbour that already has a class -- adopting it. Neither other classifier can produce a
    // group led by a higher index, so this is the pass's distinguishing behaviour.
    let mut c = Connections::new();
    // 0 and 1 are unconnected to each other; 2 is wired to both.
    c.connect(10, 12, 5.0);
    c.connect(11, 12, 5.0);
    let class = classify_by_interconn(&c, &[10, 11, 12]);
    assert_eq!(class[0], 0, "10 leads");
    assert_eq!(class[2], 0, "12 was claimed by 10");
    assert_eq!(class[1], 0, "11 adopted 12's class rather than leading its own");
}

#[test]
fn the_interconnection_pass_scans_EARLIER_macros_too() {
    // ⚠️ The test above does not actually prove the backward scan: its adoption happens from a
    // LATER index, which a forward-only loop reaches just as well. Proving it needs a macro whose
    // only strong neighbour sits BEHIND it.
    //
    // Chain 10-11-12 with no 10-12 link. Macro 10 leads and claims 11. Macro 12 is then unclassed
    // and its only neighbour is 11, at a LOWER index -- reachable only by scanning backwards.
    //   full scan:    [0, 0, 0]   -- 12 adopts 11's class
    //   forward only: [0, 0, 2]   -- 12 never sees 11 and leads its own
    let mut c = Connections::new();
    c.connect(10, 11, 5.0);
    c.connect(11, 12, 5.0);
    assert_eq!(classify_by_interconn(&c, &[10, 11, 12]), vec![0, 0, 0]);
}

// ------------------------------------------------------------------ grouping

#[test]
fn a_merge_ALWAYS_requires_the_same_size() {
    // 🔑 The connection tests decide WHICH KIND of group it is, never whether to group at all.
    // Two differently-sized macros never merge however strongly they are wired.
    let g = group_single_macro_clusters(&[0, 1], &[0, 0], &[0, 0]);
    assert!(g.merges.is_empty(), "different size classes, no merge");
    assert_eq!(g.macro_class, vec![0, 1]);
}

#[test]
fn same_size_and_same_interconnection_makes_an_interconnected_array() {
    let g = group_single_macro_clusters(&[0, 0], &[0, 1], &[0, 0]);
    assert_eq!(g.merges, vec![GroupMerge::Interconnected { receiver: 0, incomer: 1 }]);
    assert_eq!(g.macro_class, vec![0, 0]);
}

#[test]
fn same_size_and_same_signature_merges_when_the_interconnection_differs() {
    let g = group_single_macro_clusters(&[0, 0], &[0, 0], &[0, 1]);
    assert_eq!(g.merges, vec![GroupMerge::SameSignature { receiver: 0, incomer: 1 }]);
}

#[test]
fn same_size_but_neither_test_matching_does_not_merge() {
    let g = group_single_macro_clusters(&[0, 0], &[0, 1], &[0, 1]);
    assert!(g.merges.is_empty());
    assert_eq!(g.macro_class, vec![0, 1]);
}

#[test]
fn meeting_a_different_interconnection_CLEARS_the_leaders_own_class() {
    // ⚠️ Upstream mutates interconn_class[i] mid-loop, and it affects every LATER j in the same
    // inner loop -- so the order of comparisons changes the outcome. This is how a real
    // interconnected array is distinguished from macros merely sharing a signature.
    let g = group_single_macro_clusters(&[0, 0], &[0, 0], &[0, 1]);
    assert_eq!(g.interconn_class[0], -1, "the leader's class was cleared");
}

#[test]
fn a_cleared_leader_no_longer_matches_a_later_macro_by_interconnection() {
    // Three same-size macros. Macro 1 differs in interconnection, clearing 0's class to -1.
    // Macro 2 then compares against -1 rather than against 0's original class.
    let g = group_single_macro_clusters(&[0, 0, 0], &[0, 9, 0], &[0, 1, 0]);
    assert_eq!(g.interconn_class[0], -1);
    // 2 shares 0's signature, so it merges by signature rather than by interconnection.
    assert!(
        g.merges.contains(&GroupMerge::SameSignature { receiver: 0, incomer: 2 }),
        "merged by signature, not as an array: {:?}",
        g.merges
    );
}

#[test]
fn a_single_movable_macro_is_never_an_array_of_one() {
    // ⚠️ Upstream special-cases this before any classification runs.
    let g = single_macro_grouping();
    assert_eq!(g.macro_class, vec![0]);
    assert_eq!(g.interconn_class, vec![-1], "not an interconnected array");
}

#[test]
fn a_macro_already_merged_is_not_considered_again() {
    let g = group_single_macro_clusters(&[0, 0, 0], &[0, 0, 0], &[0, 0, 0]);
    assert_eq!(g.macro_class, vec![0, 0, 0], "both followers went to 0");
    assert_eq!(g.merges.len(), 2, "and 1 was not then re-merged into 2");
}

// ------------------------------------------------------------------ breakMixedLeaf

use vyges_mpl::macroclass::{break_mixed_leaf, MacroCluster};

fn mc(id: i32, name: &str, inst: usize, fixed: bool, w: i64, h: i64) -> MacroCluster {
    MacroCluster { id, name: name.into(), inst, is_fixed: fixed, size: sz(w, h) }
}

#[test]
fn every_macro_becomes_its_own_cluster_and_the_leaf_keeps_the_cells() {
    let macros = [mc(10, "M1", 0, false, 5, 5), mc(11, "M2", 1, false, 9, 9)];
    let plan = break_mixed_leaf(1, &macros, &Connections::new());
    assert_eq!(plan.std_cell_cluster, 1, "the mixed leaf survives as the standard-cell cluster");
    assert_eq!(plan.arrays.len(), 2, "different sizes, so neither merged");
}

/// Two macros wired to a common third cluster, so their SIGNATURES match. ⚠️ Needed for any
/// merge test: same size alone does NOT merge — see `same_size_alone_does_NOT_merge`.
fn shared_signature(ids: &[i32]) -> Connections {
    let mut c = Connections::new();
    for &id in ids {
        c.connect(id, 99, 5.0);
    }
    c
}

#[test]
fn a_FIXED_macro_is_never_folded_into_an_array() {
    // 🔑 It cannot move to join one. Only the movable macros are classified at all.
    let macros = [
        mc(10, "M1", 0, false, 5, 5),
        mc(11, "M2", 1, true, 5, 5),
        mc(12, "M3", 2, false, 5, 5),
    ];
    let plan = break_mixed_leaf(1, &macros, &shared_signature(&[10, 12]));
    assert_eq!(plan.fixed_clusters, vec![11], "the fixed one stands alone");
    assert_eq!(plan.arrays.len(), 1, "the two movable ones merged");
    assert_eq!(plan.arrays[0].members, vec![10, 12], "and the fixed one is not among them");
}

#[test]
fn same_size_alone_does_NOT_merge() {
    // ⚠️ The correction that cost three tests. "Same size is required" is true and is NOT
    // sufficient: the grouping needs size AND (same interconnection OR same signature). Two
    // identical macros wired to nothing share neither, because an empty neighbour set is
    // deliberately not a signature match -- so they stay apart.
    let macros = [mc(10, "M1", 0, false, 5, 5), mc(11, "M2", 1, false, 5, 5)];
    let plan = break_mixed_leaf(1, &macros, &Connections::new());
    assert_eq!(plan.arrays.len(), 2, "identical size, no connections, no merge");
}

#[test]
fn same_sized_movable_macros_merge_into_one_array() {
    let macros = [mc(10, "M1", 0, false, 5, 5), mc(11, "M2", 1, false, 5, 5)];
    let plan = break_mixed_leaf(1, &macros, &shared_signature(&[10, 11]));
    assert_eq!(plan.arrays.len(), 1);
    assert_eq!(plan.arrays[0].id, 10, "the leader keeps its id");
    assert_eq!(plan.arrays[0].members, vec![10, 11]);
}

#[test]
fn a_single_movable_macro_is_not_an_interconnected_array() {
    let macros = [mc(10, "M1", 0, false, 5, 5), mc(11, "M2", 1, true, 5, 5)];
    let plan = break_mixed_leaf(1, &macros, &Connections::new());
    assert_eq!(plan.arrays.len(), 1);
    assert!(!plan.arrays[0].is_interconnected_array, "one macro is not an array");
}

#[test]
fn a_wired_group_is_marked_as_an_interconnected_array() {
    let mut c = Connections::new();
    c.connect(10, 11, 5.0);
    let macros = [mc(10, "M1", 0, false, 5, 5), mc(11, "M2", 1, false, 5, 5)];
    let plan = break_mixed_leaf(1, &macros, &c);
    assert_eq!(plan.arrays.len(), 1);
    assert!(plan.arrays[0].is_interconnected_array);
}

// ------------------------------------------------------------------ virtual connections

#[test]
fn virtual_connections_join_every_pair() {
    // 🔑 The std-cell cluster, each surviving array and each fixed cluster are all joined, so the
    // annealer knows they came from one place. Three members means three pairs.
    let macros = [mc(10, "M1", 0, false, 5, 5), mc(11, "M2", 1, true, 9, 9)];
    let plan = break_mixed_leaf(1, &macros, &Connections::new());
    assert_eq!(
        plan.virtual_connections,
        vec![(1, 10), (1, 11), (10, 11)],
        "std-cell first, then the array, then the fixed cluster"
    );
}

#[test]
fn a_leaf_with_one_macro_still_joins_it_to_the_cells() {
    let macros = [mc(10, "M1", 0, false, 5, 5)];
    let plan = break_mixed_leaf(1, &macros, &Connections::new());
    assert_eq!(plan.virtual_connections, vec![(1, 10)]);
}

#[test]
fn merged_macros_contribute_ONE_virtual_connection_not_one_each() {
    // ⚠️ Only the surviving leaders take part. Counting every macro would over-connect the
    // annealer's view by exactly the number of merges.
    let macros = [
        mc(10, "M1", 0, false, 5, 5),
        mc(11, "M2", 1, false, 5, 5),
        mc(12, "M3", 2, false, 5, 5),
    ];
    let plan = break_mixed_leaf(1, &macros, &shared_signature(&[10, 11, 12]));
    assert_eq!(plan.arrays.len(), 1, "all three merged");
    assert_eq!(plan.virtual_connections, vec![(1, 10)], "one array, one connection");
}

#[test]
fn a_leaf_with_no_macros_has_no_connections_to_make() {
    let plan = break_mixed_leaf(1, &[], &Connections::new());
    assert!(plan.arrays.is_empty() && plan.fixed_clusters.is_empty());
    assert!(plan.virtual_connections.is_empty());
}
