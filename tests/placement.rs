// SPDX-License-Identifier: Apache-2.0
//! The placement stage's setup steps.

use vyges_mpl::placement::{adjusted_soft_blockage_weight, tiny_cluster_max_number_of_std_cells};

/// ⚠️ **Truncated, so a small block gets a threshold of ZERO** — and since the comparison against
/// it is a strict `<`, zero means no cluster is ever tiny.
#[test]
fn a_small_block_has_no_tiny_clusters_at_all() {
    assert_eq!(tiny_cluster_max_number_of_std_cells(0), 0);
    assert_eq!(tiny_cluster_max_number_of_std_cells(999), 0, "just under the first whole unit");
    assert_eq!(tiny_cluster_max_number_of_std_cells(1000), 1);
    assert_eq!(tiny_cluster_max_number_of_std_cells(1999), 1, "still one, truncated");
    assert_eq!(tiny_cluster_max_number_of_std_cells(2000), 2);
}

/// ⚠️ Every one of the regression designs is far below the first whole unit, so the threshold is
/// zero throughout — worth knowing before reading anything into it.
#[test]
fn the_regression_designs_are_all_below_the_threshold() {
    for instance_count in [4usize, 152, 500, 900] {
        assert_eq!(
            tiny_cluster_max_number_of_std_cells(instance_count),
            0,
            "{instance_count} instances"
        );
    }
}

/// 🔑 **Only a single-level tree is adjusted**, to half the outline weight. A deeper tree keeps
/// whatever it had.
#[test]
fn only_a_single_level_tree_has_its_soft_blockage_weight_raised() {
    assert_eq!(adjusted_soft_blockage_weight(1, 100.0, 50.0), 50.0, "half of 100 is 50");
    assert_eq!(adjusted_soft_blockage_weight(1, 40.0, 50.0), 20.0, "and it can go DOWN");
    assert_eq!(adjusted_soft_blockage_weight(2, 100.0, 50.0), 50.0, "two levels: untouched");
    assert_eq!(adjusted_soft_blockage_weight(0, 100.0, 7.0), 7.0, "zero levels: untouched");
}
