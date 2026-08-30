// SPDX-License-Identifier: Apache-2.0
//! The cluster tree. Every case pins a rule read from upstream `object.cpp` / `clusterEngine.cpp`.
use vyges_mpl::cluster::{
    is_large_flat_cluster, is_merge_candidate, should_break, update_sub_tree, Cluster, ClusterType,
    Metrics,
};

fn c(id: i32, name: &str) -> Cluster {
    Cluster::new(id, name)
}

fn with_children(id: i32, name: &str, children: Vec<Cluster>) -> Cluster {
    let mut p = c(id, name);
    p.children = children;
    p
}

// ------------------------------------------------------------- type masking

#[test]
fn a_hard_macro_cluster_reports_no_standard_cells() {
    // 🔑 The type MASKS the metric. Reading `metrics` directly would change every threshold
    // comparison downstream, and the cluster would still look right in a debug dump.
    let mut k = c(1, "m");
    k.cluster_type = ClusterType::HardMacro;
    k.metrics = Metrics { num_std_cell: 50, num_macro: 3 , ..Default::default() };
    assert_eq!(k.num_std_cell(), 0, "masked");
    assert_eq!(k.num_macro(), 3, "not masked");
}

#[test]
fn a_std_cell_cluster_reports_no_macros() {
    let mut k = c(1, "s");
    k.cluster_type = ClusterType::StdCell;
    k.metrics = Metrics { num_std_cell: 50, num_macro: 3 , ..Default::default() };
    assert_eq!(k.num_std_cell(), 50);
    assert_eq!(k.num_macro(), 0, "masked");
}

#[test]
fn a_mixed_cluster_masks_neither() {
    let mut k = c(1, "x");
    k.cluster_type = ClusterType::Mixed;
    k.metrics = Metrics { num_std_cell: 50, num_macro: 3 , ..Default::default() };
    assert_eq!((k.num_std_cell(), k.num_macro()), (50, 3));
}

// ------------------------------------------------------------- predicates

#[test]
fn a_cluster_with_one_module_and_no_leaves_corresponds_to_a_logical_module() {
    let mut k = c(1, "m");
    k.db_modules = vec![7];
    assert!(k.corresponds_to_logical_module());
    assert!(!k.is_empty(), "it has a module, so it is not empty");
}

#[test]
fn glue_instances_stop_a_cluster_corresponding_to_a_logical_module() {
    // ⚠️ One module PLUS loose leaves takes the merged-cluster branch of breakCluster instead.
    let mut k = c(1, "m");
    k.db_modules = vec![7];
    k.leaf_std_cells = vec![100];
    assert!(!k.corresponds_to_logical_module());
}

#[test]
fn two_modules_stop_it_too() {
    let mut k = c(1, "m");
    k.db_modules = vec![7, 8];
    assert!(!k.corresponds_to_logical_module(), "exactly one, not at least one");
}

#[test]
fn empty_means_no_leaves_and_no_modules() {
    assert!(c(1, "e").is_empty());
    let mut k = c(1, "e");
    k.leaf_macros = vec![1];
    assert!(!k.is_empty());
}

#[test]
fn any_of_the_three_io_flags_makes_it_an_io_cluster() {
    for set in [
        |k: &mut Cluster| k.is_cluster_of_unplaced_io_pins = true,
        |k: &mut Cluster| k.is_io_pad_cluster = true,
        |k: &mut Cluster| k.is_io_bundle = true,
    ] {
        let mut k = c(1, "io");
        assert!(!k.is_io_cluster());
        set(&mut k);
        assert!(k.is_io_cluster());
    }
}

// ------------------------------------------------------------- break vs merge

#[test]
fn breaking_needs_either_count_over_its_maximum() {
    // ⚠️ `||`. One count over the limit is enough.
    let mut k = c(1, "k");
    k.metrics = Metrics { num_std_cell: 5000, num_macro: 1 , ..Default::default() };
    assert!(should_break(&k, 100, 10), "std cells alone");
    k.metrics = Metrics { num_std_cell: 1, num_macro: 50 , ..Default::default() };
    assert!(should_break(&k, 100, 10), "macros alone");
    k.metrics = Metrics { num_std_cell: 100, num_macro: 10 , ..Default::default() };
    assert!(!should_break(&k, 100, 10), "strictly greater, so equal does not break");
}

#[test]
fn merging_needs_both_counts_under_their_minimum() {
    // ⚠️ `&&`. The mirror of should_break, and deliberately NOT the same operator.
    let mut k = c(1, "k");
    k.metrics = Metrics { num_std_cell: 1, num_macro: 1 , ..Default::default() };
    assert!(is_merge_candidate(&k, 100, 10));
    k.metrics = Metrics { num_std_cell: 1, num_macro: 50 , ..Default::default() };
    assert!(!is_merge_candidate(&k, 100, 10), "one count too big is enough to spare it");
    k.metrics = Metrics { num_std_cell: 100, num_macro: 1 , ..Default::default() };
    assert!(!is_merge_candidate(&k, 100, 10), "strictly less, so equal is not small");
}

#[test]
fn an_io_cluster_is_never_a_merge_candidate() {
    let mut k = c(1, "io");
    k.metrics = Metrics { num_std_cell: 0, num_macro: 0 , ..Default::default() };
    assert!(is_merge_candidate(&k, 100, 10), "as an ordinary cluster it would merge");
    k.is_io_bundle = true;
    assert!(!is_merge_candidate(&k, 100, 10), "as an IO cluster it never does");
}

// ------------------------------------------------------------- the par gate

#[test]
fn a_large_flat_cluster_is_one_with_no_modules_and_too_many_leaves() {
    let mut k = c(1, "flat");
    k.leaf_std_cells = (0..6000).collect();
    assert!(is_large_flat_cluster(&k, 5000, 5));
    // A module to split on means it is not flat, however large.
    k.db_modules = vec![1];
    assert!(!is_large_flat_cluster(&k, 5000, 5), "hierarchy still has something to split on");
}

#[test]
fn the_par_gate_counts_leaf_vectors_not_the_masked_metrics() {
    // 🔑 isLargeFlatCluster reads getLeafStdCells().size(), NOT getNumStdCell(). A cluster typed
    // HardMacro reports 0 standard cells through the masked accessor but still has its leaves,
    // and upstream partitions it on those.
    let mut k = c(1, "flat");
    k.cluster_type = ClusterType::HardMacro;
    k.leaf_std_cells = (0..6000).collect();
    k.metrics = Metrics { num_std_cell: 6000, num_macro: 0 , ..Default::default() };
    assert_eq!(k.num_std_cell(), 0, "the masked accessor says zero");
    assert!(is_large_flat_cluster(&k, 5000, 5), "but the gate still fires");
}

#[test]
fn either_leaf_kind_can_trip_the_par_gate() {
    let mut k = c(1, "flat");
    k.leaf_macros = (0..6).collect();
    assert!(is_large_flat_cluster(&k, 5000, 5), "macros alone");
}

#[test]
fn the_tinyrocket_flow_shape_trips_the_par_gate() {
    // ⛔ **MEASURED, not constructed.** OpenROAD `test/flow.tcl` driven by
    // `tinyRocket_nangate45.tcl`, at pin 945a9f48dc6e5cc91d865daa92c45a1094cb682c, emits
    //
    //   [DEBUG MPL-multilevel_autoclustering] num level: 1, max_macro: 5, min_macro: 1,
    //                                         max_inst:10945, min_inst:2189
    //   [DEBUG MPL-multilevel_autoclustering] Attempting flat cluster (root)_glue_logic
    //                                         partitioning with balance constraint = 1
    //
    // 🔑 **Why it is flat.** `tinyRocket_nangate45.v` declares exactly ONE module and `flow.tcl`
    // links it with a bare `link_design` (no `-hier`). So `breakCluster(root)` takes upstream's
    // `module->getChildren().empty()` branch, `createFlatCluster` puts all 21,890 std cells into
    // one `(root)_glue_logic` cluster carrying no `dbModule`s, and `isLargeFlatCluster`
    // (clusterEngine.cpp:1101) is true by construction rather than by size alone.
    //
    // ⚠️ This is the case the 36-design regression suite does NOT cover: those read a prepared
    // database whose hierarchy survives, so TritonPart fires in 0 of them. A green suite is
    // therefore not evidence that `mpl` runs on a synthesized flat netlist — and this test is
    // here so that claim cannot be made by accident again.
    let mut root_glue_logic = c(1, "(root)_glue_logic");
    root_glue_logic.leaf_std_cells = (0..21_890).collect();
    root_glue_logic.leaf_macros = (21_890..21_892).collect();

    // The thresholds the reference derived for this design at level 1.
    assert!(
        is_large_flat_cluster(&root_glue_logic, 10_945, 5),
        "the only macro-bearing top-level flow design reaches the partitioner"
    );

    // ⚠️ Upstream then recurses on the split halves. The larger one, 13,724 std cells, is still
    // over the threshold — which is why the reference logs the line TWICE, not once.
    let mut half = c(2, "(root)_glue_logic_0");
    half.leaf_std_cells = (0..13_724).collect();
    assert!(is_large_flat_cluster(&half, 10_945, 5), "and again on the larger half");

    // The smaller halves are under it, and that is where upstream's recursion stops.
    let mut quarter = c(3, "(root)_glue_logic_0_1");
    quarter.leaf_std_cells = (0..7_350).collect();
    assert!(!is_large_flat_cluster(&quarter, 10_945, 5), "recursion terminates here");
}

// ------------------------------------------------------------- update_sub_tree

#[test]
fn the_subtree_collapses_to_its_leaves() {
    // parent -> [a -> [a1, a2], b] collapses to parent -> [b, a1, a2]:
    // b is already a leaf and is dequeued first; a is dissolved and its children queued behind.
    let mut parent = with_children(
        0,
        "root",
        vec![
            with_children(1, "a", vec![c(11, "a1"), c(12, "a2")]),
            c(2, "b"),
        ],
    );
    let r = update_sub_tree(&mut parent, i32::MAX, i32::MAX);
    let names: Vec<&str> = parent.children.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, vec!["b", "a1", "a2"], "breadth-first order");
    assert_eq!(r.dissolved, vec![1], "the intermediate cluster ceases to exist");
}

#[test]
fn the_collapse_is_breadth_first_not_depth_first() {
    // ⚠️ The observable difference. Upstream uses std::queue (FIFO); a stack would give
    // ["a1", "x", "y", "b"] here. That order survives into the annealer's sequence pair, so
    // this is not cosmetic.
    let mut parent = with_children(
        0,
        "root",
        vec![
            with_children(1, "a", vec![c(11, "a1"), with_children(12, "a2", vec![c(21, "x"), c(22, "y")])]),
            c(2, "b"),
        ],
    );
    update_sub_tree(&mut parent, i32::MAX, i32::MAX);
    let names: Vec<&str> = parent.children.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, vec!["b", "a1", "x", "y"]);
}

#[test]
fn a_flat_parent_is_left_alone() {
    let mut parent = with_children(0, "root", vec![c(1, "a"), c(2, "b")]);
    let r = update_sub_tree(&mut parent, i32::MAX, i32::MAX);
    let names: Vec<&str> = parent.children.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"], "order preserved");
    assert!(r.dissolved.is_empty());
}

#[test]
fn every_dissolved_id_is_reported_so_the_id_map_can_be_pruned() {
    // Upstream erases these from id_to_cluster. A leaked id would keep a destroyed cluster
    // reachable by id -- and ODB row names taught us what stale keyed lookups cost.
    let mut parent = with_children(
        0,
        "root",
        vec![with_children(
            1,
            "a",
            vec![with_children(2, "b", vec![c(3, "leaf")])],
        )],
    );
    let r = update_sub_tree(&mut parent, i32::MAX, i32::MAX);
    assert_eq!(r.dissolved, vec![1, 2]);
    assert_eq!(parent.children.len(), 1);
    assert_eq!(parent.children[0].name, "leaf");
}

#[test]
fn a_resulting_child_needing_the_partitioner_is_reported_not_approximated() {
    // ⛔ Stage 1 has no TritonPart. The caller refuses on this list.
    let mut big = c(1, "flat");
    big.leaf_std_cells = (0..6000).collect();
    let mut parent = with_children(0, "root", vec![big, c(2, "small")]);
    let r = update_sub_tree(&mut parent, 5000, 5);
    assert_eq!(r.needs_partitioning, vec![1]);
}

#[test]
fn nothing_needs_partitioning_when_every_child_fits() {
    let mut parent = with_children(0, "root", vec![c(1, "a"), c(2, "b")]);
    let r = update_sub_tree(&mut parent, 5000, 5);
    assert!(r.needs_partitioning.is_empty());
}

// ---------------------------------------------------------------- the cluster's own soft macro

/// 🔑 **Four kinds of cluster get their soft macro at CLUSTERING time**, not during placement — the
/// three IO kinds and a fixed macro. Everything else has none until placement gives it one.
#[test]
fn only_four_kinds_get_a_soft_macro_at_clustering_time() {
    let mut plain = Cluster::new(1, "plain");
    assert!(plain.soft_macro.is_none());
    plain.cluster_type = ClusterType::Mixed;
    assert!(plain.soft_macro.is_none(), "a mixed cluster waits for placement");

    let mut bundle = Cluster::new(2, "L_0");
    bundle.set_as_io_bundle((0, 1000), 0, 500);
    assert!(bundle.soft_macro.is_some());
    assert!(bundle.is_io_cluster());
}

/// ⛔ **Every geometry accessor returns ZERO without a soft macro** — not an error and not an
/// absence. A cluster that has not been given one reads as a point at the origin.
#[test]
fn a_cluster_without_a_soft_macro_reads_as_the_origin() {
    let c = Cluster::new(1, "c");
    assert_eq!(c.location(), (0, 0));
    assert_eq!(c.size(), (0, 0));
    assert_eq!(c.center(), (0, 0));
}

/// ⛔ **`set_location` is a silent NO-OP without one.** Upstream guards the write and reports
/// nothing, so moving such a cluster simply does not happen.
#[test]
fn moving_a_cluster_without_a_soft_macro_does_nothing() {
    let mut c = Cluster::new(1, "c");
    c.set_location((500, 600));
    assert_eq!(c.location(), (0, 0), "silently unmoved");

    c.set_as_io_bundle((10, 20), 0, 100);
    c.set_location((500, 600));
    assert_eq!(c.location(), (500, 600), "and now it moves");
}

/// ⚠️ **An IO cluster's area is ZERO whatever its extent** — upstream's point-and-name constructor
/// leaves it at its default, so it occupies a region and contributes nothing to any area sum.
///
/// ⛔ **A BUNDLE cannot show this**, and a mutation proved it: a bundle is a LINE on a die edge, so
/// one of its dimensions is zero and the product is zero however the area is computed. Only a PAD
/// cluster, which has real extent on both axes, separates "area is defaulted" from "area is
/// width times height".
#[test]
fn an_io_clusters_area_is_zero_however_large_its_region() {
    let mut pads = Cluster::new(1, "io_pads");
    pads.set_as_io_pad_cluster((0, 0), 4_000, 25_000);
    assert_eq!(pads.soft_macro.unwrap().area, 0, "not 100_000_000");
    assert_eq!(pads.size(), (4_000, 25_000), "the region is still there");

    let mut bundle = Cluster::new(2, "L_0");
    bundle.set_as_io_bundle((0, 0), 0, 100_000);
    assert_eq!(bundle.soft_macro.unwrap().area, 0);

    let mut ios = Cluster::new(3, "ios");
    ios.set_as_cluster_of_unplaced_io_pins((0, 0), 3_000, 3_000, false);
    assert_eq!(ios.soft_macro.unwrap().area, 0, "all three kinds, not just the line-shaped one");
}

/// ⛔ **A FIXED macro reports its SOFT MACRO's area**, not its metrics' — and its soft macro is
/// built WITHOUT an outline, so it is absolute and unclipped.
#[test]
fn a_fixed_macro_reports_its_soft_macro_area() {
    let mut fixed = Cluster::new(1, "MACRO_1");
    fixed.set_as_fixed_macro((100, 200, 300, 500));
    assert_eq!(fixed.location(), (100, 200), "absolute, not rebased");
    assert_eq!(fixed.size(), (200, 300));
    assert_eq!(fixed.area(), 200 * 300);
}

/// ⚠️ Anything else reports its metrics.
#[test]
fn an_ordinary_cluster_reports_its_metrics_area() {
    let mut c = Cluster::new(1, "c");
    c.cluster_type = ClusterType::Mixed;
    c.metrics.std_cell_area = 700;
    c.metrics.macro_area = 300;
    assert_eq!(c.area(), 1000);
}

/// ⚠️ **The centre halves in INTEGERS**, and is not `SoftMacro::getPinX` — that one computes
/// `x + 0.5 * width` in floating point. They agree for non-negative coordinates and are different
/// expressions; upstream uses this one for a fixed terminal and the other for a pin.
#[test]
fn the_centre_halves_in_integers() {
    let mut c = Cluster::new(1, "c");
    c.set_as_io_bundle((0, 0), 101, 101);
    assert_eq!(c.center(), (50, 50), "not 50.5");
}

/// ⚠️ An unplaced-IO cluster sets BOTH flags, the second only when the pins carry no constraint.
#[test]
fn an_unplaced_io_cluster_records_whether_it_is_constrained() {
    let mut constrained = Cluster::new(1, "ios");
    constrained.set_as_cluster_of_unplaced_io_pins((0, 0), 0, 100, false);
    assert!(constrained.is_cluster_of_unplaced_io_pins);
    assert!(!constrained.is_cluster_of_unconstrained_io_pins);

    let mut free = Cluster::new(2, "ios");
    free.set_as_cluster_of_unplaced_io_pins((0, 0), 0, 100, true);
    assert!(free.is_cluster_of_unconstrained_io_pins);
}
