// SPDX-License-Identifier: Apache-2.0
//! The three stages the DEF golden needs and the traces never showed.

use vyges_mpl::anneal::SoftWeights;
use vyges_mpl::placement::{
    reported_wirelength, reset_sa_parameters, temporary_std_cell_location,
    temporary_std_cell_placement, StdCellPlacementCluster, WIRELENGTH_METRIC,
};

fn cluster(is_leaf: bool, num_std_cell: i32) -> StdCellPlacementCluster {
    StdCellPlacementCluster {
        is_leaf,
        num_std_cell,
        module_core_insts: Vec::new(),
        leaf_std_cells: Vec::new(),
        children: Vec::new(),
    }
}

/// ⛔ **Every standard cell of a cluster goes to the SAME point** — the cluster's centre, offset by
/// each cell's own half-size so its own centre lands there. They all overlap, deliberately: this
/// exists so the orientation step has somewhere to measure from, not to be a placement.
#[test]
fn every_cell_lands_on_the_clusters_centre() {
    let cluster_box = Some(((1000, 2000), (400, 600)));
    // Centre is (1200, 2300). A 100 x 50 cell goes to (1150, 2275).
    assert_eq!(temporary_std_cell_location(cluster_box, (100, 50)), Some((1150, 2275)));
    // A different-sized cell lands on the same CENTRE, at a different origin.
    assert_eq!(temporary_std_cell_location(cluster_box, (200, 100)), Some((1100, 2250)));
}

/// ⚠️ **Two integer divisions, and each loses its half unit** — the cluster's own pin centre, and
/// the cell's half-extent.
#[test]
fn both_halvings_truncate() {
    // Width 401 -> centre x + 200, not x + 200.5.
    assert_eq!(temporary_std_cell_location(Some(((0, 0), (401, 401))), (0, 0)), Some((200, 200)));
    // Cell width 101 -> half is 50.
    assert_eq!(temporary_std_cell_location(Some(((0, 0), (0, 0))), (101, 101)), Some((-50, -50)));
}

/// ⚠️ A cluster with no soft macro places nothing.
#[test]
fn a_cluster_without_a_soft_macro_places_nothing() {
    assert_eq!(temporary_std_cell_location(None, (100, 100)), None);
}

// ---------------------------------------------------------------- the walk

/// ⛔ **Only a LEAF with standard cells places anything.** A non-leaf never places its own cells —
/// it only recurses — so a mixed cluster's cells are placed by whichever leaf descendant owns them.
#[test]
fn only_a_leaf_with_cells_places_anything() {
    let mut root = cluster(false, 500);
    root.leaf_std_cells = vec![1, 2];
    root.children = vec![1];

    let mut leaf = cluster(true, 3);
    leaf.leaf_std_cells = vec![7, 8];

    let got = temporary_std_cell_placement(&[root, leaf], 0);
    assert_eq!(got, vec![(7, 1), (8, 1)], "the root placed none of its own");
}

/// ⛔ **A leaf with NO standard cells places nothing**, however many instances its modules hold.
#[test]
fn a_leaf_without_cells_places_nothing() {
    let mut leaf = cluster(true, 0);
    leaf.module_core_insts = vec![1, 2, 3];
    leaf.leaf_std_cells = vec![4];
    assert!(temporary_std_cell_placement(&[leaf], 0).is_empty());
}

/// ⚠️ **Both loops run for a placing leaf** — modules first, then the explicit list. A cell
/// reachable both ways is placed twice, to the same point.
#[test]
fn modules_are_walked_before_the_explicit_list() {
    let mut leaf = cluster(true, 2);
    leaf.module_core_insts = vec![5, 9];
    leaf.leaf_std_cells = vec![9, 1];
    let got = temporary_std_cell_placement(&[leaf], 0);
    let insts: Vec<usize> = got.iter().map(|(i, _)| *i).collect();
    assert_eq!(insts, vec![5, 9, 9, 1], "9 appears twice, and modules come first");
}

/// ⚠️ Recursion is depth-first in child order, and every placing leaf reports its own id.
#[test]
fn the_walk_is_depth_first_and_names_the_owning_cluster() {
    let mut root = cluster(false, 0);
    root.children = vec![1, 2];
    let mut a = cluster(true, 1);
    a.leaf_std_cells = vec![10];
    let mut b = cluster(true, 1);
    b.leaf_std_cells = vec![20];

    let got = temporary_std_cell_placement(&[root, a, b], 0);
    assert_eq!(got, vec![(10, 1), (20, 2)]);
}

// ---------------------------------------------------------------- the no-cells reset

/// 🔑 **It zeroes RESIZE** — the action that changes a cluster's shape. With no standard cells
/// there is no soft area to trade, so the annealer keeps only the four ordering moves.
#[test]
fn a_design_without_cells_never_resizes() {
    let got = reset_sa_parameters(SoftWeights::placement_defaults());
    assert_eq!(got.resize, 0.0);
    assert_eq!((got.pos_swap, got.neg_swap, got.double_swap, got.exchange_swap), (0.2, 0.2, 0.2, 0.2));
}

/// ⛔ **It zeroes FENCE too**, alongside boundary, notch and soft blockage. The fence weight is
/// otherwise `10.0` from the command — this is the only path that ever turns it off.
#[test]
fn the_reset_is_the_only_path_that_zeroes_the_fence() {
    let base = SoftWeights::placement_defaults();
    assert_eq!(base.fence, 10.0, "live by default");

    let got = reset_sa_parameters(base).weights;
    assert_eq!(got.fence, 0.0);
    assert_eq!((got.boundary, got.notch, got.soft_blockage), (0.0, 0.0, 0.0));
}

/// ⚠️ It leaves the other weights alone.
#[test]
fn the_reset_touches_only_four_weights() {
    let base = SoftWeights::placement_defaults();
    let got = reset_sa_parameters(base).weights;
    assert_eq!(got.area, base.area);
    assert_eq!(got.outline, base.outline);
    assert_eq!(got.wirelength, base.wirelength);
    assert_eq!(got.guidance, base.guidance);
    assert_eq!(got.fixed_macros, base.fixed_macros);
}

// ---------------------------------------------------------------- the reported metric

/// ⚠️ **Reported in MICRONS**, so the metric is a `double` division rather than the database's own
/// integer.
#[test]
fn the_wirelength_metric_is_in_microns() {
    assert_eq!(WIRELENGTH_METRIC, "macro_place__wirelength");
    assert_eq!(reported_wirelength(2_000_000, 2000), 1000.0);
    assert_eq!(reported_wirelength(1, 2000), 0.0005, "a fraction, not zero");
}
