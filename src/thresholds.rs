// SPDX-License-Identifier: Apache-2.0
//! Cluster-size thresholds — how many macros and standard cells a cluster may hold at each level.
//!
//! Upstream: `ClusteringEngine::setBaseThresholds` and `ClusteringEngine::updateSizeThresholds`.
//!
//! ⚠️ **The arithmetic here is deliberately faithful to upstream's TYPES, not merely to its
//! formulas.** Upstream stores the base thresholds as `int` and `cluster_size_ratio` as `float`,
//! and the two functions do not even agree with each other on the coarsening factor's type:
//! `setBaseThresholds` truncates it to `unsigned`, `updateSizeThresholds` keeps it `double`.
//! Computing all of this in `f64` would be *more* accurate and *less* correct.

/// Upstream's defaults and the values the user may override.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub max_macro: i32,
    pub min_macro: i32,
    pub max_std_cell: i32,
    pub min_std_cell: i32,
}

/// What the design contributes to the derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesignMetrics {
    pub num_macro: i32,
    pub num_std_cell: i32,
}

/// ⚠️ Below this many macros, multilevel clustering is not worth it and the level count is reset
/// to 1. Upstream's constant, and its comment says it comes from the original implementation.
const MIN_NUM_MACROS_FOR_MULTILEVEL: i32 = 150;

/// ⚠️ A derived minimum below this is raised to it. Upstream's `min_num_std_cells_allowed`.
const MIN_NUM_STD_CELLS_ALLOWED: i32 = 1000;

/// `int = float_expr` in C++ truncates toward zero. Rust's `as` does too, and also saturates
/// rather than invoking UB, which is the behaviour we want at the edges.
fn trunc(v: f64) -> i32 {
    v as i32
}

/// `base * ratio / 2.0`, in upstream's precision.
///
/// 🔑 `int * float` is computed in **f32** (the int converts to float), and only the following
/// `/ 2.0` promotes to f64 because `2.0` is a double literal. Doing the whole thing in f64 would
/// give a different answer wherever f32 cannot represent the product exactly.
fn half_ratio(base: i32, ratio: f32) -> i32 {
    trunc((base as f32 * ratio) as f64 / 2.0)
}

/// The outcome of `setBaseThresholds`: the base thresholds plus the level count, which this
/// function may lower.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaseThresholds {
    pub thresholds: Thresholds,
    pub max_level: i32,
}

/// `ClusteringEngine::setBaseThresholds`.
///
/// `supplied` carries the four `-max_num_*` / `-min_num_*` values, where **0 means auto**.
///
/// 🔑 **Supplying all four thresholds also disables the level reset.** The
/// `num_macro <= 150 -> max_level = 1` rule lives *inside* the derivation block, so a design with
/// two macros keeps `max_level = 2` when the user supplies thresholds and drops to 1 when it does
/// not. Verified against upstream: `keep_clustering_data2` supplies all four and reports
/// `num level: 2`, while `halos1` supplies none and reports `num level: 1`.
pub fn set_base_thresholds(
    supplied: Thresholds,
    metrics: DesignMetrics,
    cluster_size_ratio: f32,
    max_num_level: i32,
    has_fixed_macros: bool,
) -> BaseThresholds {
    let mut t = supplied;
    let mut max_level = max_num_level;

    // Upstream's TODO says it plainly: hierarchical clustering with fixed macros is not
    // supported, so a fixed macro forces a single level.
    if has_fixed_macros {
        max_level = 1;
    }

    // ⚠️ ANY of the four being non-positive triggers the derivation of ALL four -- it is not
    // per-field. A user supplying only `-max_num_inst` gets the other three derived AND the
    // level reset applied.
    if t.max_macro <= 0 || t.min_macro <= 0 || t.max_std_cell <= 0 || t.min_std_cell <= 0 {
        if metrics.num_macro <= MIN_NUM_MACROS_FOR_MULTILEVEL {
            max_level = 1;
        }

        // floor(num_std_cell / ratio^max_level), then raised to the allowed minimum.
        let divisor = (cluster_size_ratio as f64).powi(max_level);
        t.min_std_cell = trunc((metrics.num_std_cell as f64 / divisor).floor());
        t.min_std_cell = t.min_std_cell.max(MIN_NUM_STD_CELLS_ALLOWED);
        t.max_std_cell = half_ratio(t.min_std_cell, cluster_size_ratio);

        t.min_macro = trunc((metrics.num_macro as f64 / divisor).floor());
        // ⚠️ Macros floor to 1, standard cells floor to 1000. The asymmetry is upstream's:
        // one macro is a legitimate cluster, one standard cell is not.
        if t.min_macro <= 0 {
            t.min_macro = 1;
        }
        t.max_macro = half_ratio(t.min_macro, cluster_size_ratio);
    }

    // Scale to the ROOT level. 🔑 Truncated to `unsigned` upstream, so a fractional factor
    // becomes 0 and zeroes every threshold -- reachable only with `max_level < 1`, which is why
    // this is a saturating cast rather than a panic.
    let coarsening_factor = {
        let f = (cluster_size_ratio as f64).powi(max_level - 1);
        if f < 0.0 { 0u32 } else { f as u32 }
    };
    t.max_macro = t.max_macro.saturating_mul(coarsening_factor as i32);
    t.min_macro = t.min_macro.saturating_mul(coarsening_factor as i32);
    t.max_std_cell = t.max_std_cell.saturating_mul(coarsening_factor as i32);
    t.min_std_cell = t.min_std_cell.saturating_mul(coarsening_factor as i32);

    BaseThresholds { thresholds: t, max_level }
}

/// `ClusteringEngine::updateSizeThresholds`, called on entering each level.
///
/// ⚠️ **The coarsening factor is a `double` here** and an `unsigned` in `setBaseThresholds`.
/// The asymmetry is upstream's and it is observable: at a level where `ratio^(level-1)` is not
/// an integer, dividing by the true value and dividing by its truncation differ.
///
/// 🔑 The floors differ from `setBaseThresholds`: a degenerate `min_std_cell` becomes **100**
/// here, not 1000. Upstream's comment explains the division: a high ratio per level makes the
/// clustering converge fast.
pub fn update_size_thresholds(
    base: Thresholds,
    level: i32,
    cluster_size_ratio: f32,
) -> Thresholds {
    let coarse_factor = (cluster_size_ratio as f64).powi(level - 1);

    let mut t = Thresholds {
        max_macro: trunc(base.max_macro as f64 / coarse_factor),
        min_macro: trunc(base.min_macro as f64 / coarse_factor),
        max_std_cell: trunc(base.max_std_cell as f64 / coarse_factor),
        min_std_cell: trunc(base.min_std_cell as f64 / coarse_factor),
    };

    if t.min_macro <= 0 {
        t.min_macro = 1;
        t.max_macro = half_ratio(t.min_macro, cluster_size_ratio);
    }

    if t.min_std_cell <= 0 {
        t.min_std_cell = 100;
        t.max_std_cell = half_ratio(t.min_std_cell, cluster_size_ratio);
    }

    t
}
