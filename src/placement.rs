// SPDX-License-Identifier: Apache-2.0
//! Hierarchical macro placement — the stage that runs once every cluster has a shape.
//!
//! 🔑 **Where coarse shaping asked "what size should this cluster be", this stage asks "where does
//! it go".** The two share an annealer, but almost nothing else: shaping ran with wirelength,
//! guidance, fence and every soft weight at ZERO, so its cost was area and outline alone. Here the
//! defaults are area `0.1`, outline `100`, wirelength `100`, guidance `10` and soft blockage `50`
//! — five live terms instead of two, and the wirelength one needs a netlist model that shaping
//! never had to build.

/// Upstream `computeTinyClusterMaxNumberOfStdCells`.
///
/// ⚠️ **A thousandth of the block's instance count, TRUNCATED.** The product is computed in `f32`
/// and assigned to an `int`, so a block with fewer than 1000 instances gets a threshold of zero —
/// and a threshold of zero means no cluster is ever "tiny", because the test is a strict `<`.
///
/// ⚠️ It counts **every instance in the block**, not the standard cells among them, despite naming
/// standard cells. A macro-heavy block therefore gets a larger threshold than its cell count alone
/// would justify.
pub fn tiny_cluster_max_number_of_std_cells(block_instance_count: usize) -> i32 {
    const TINY_CLUSTER_RATIO: f32 = 0.001;
    (TINY_CLUSTER_RATIO * block_instance_count as f32) as i32
}

/// Upstream `adjustSoftBlockageWeight`.
///
/// 🔑 **Only a single-level tree is adjusted.** With one level there is no hierarchy to separate
/// clusters, so the soft-blockage term is raised to half the outline weight to do that work
/// instead. A deeper tree is left alone.
///
/// ⚠️ The division is by `2.0` — a `double` — so the result is computed in double and narrowed
/// back to the `float` the weight is held in.
pub fn adjusted_soft_blockage_weight(
    max_level: i32,
    outline_weight: f32,
    current_soft_blockage_weight: f32,
) -> f32 {
    if max_level == 1 {
        (outline_weight as f64 / 2.0) as f32
    } else {
        current_soft_blockage_weight
    }
}
