// SPDX-License-Identifier: Apache-2.0
//! Cluster-size thresholds.
//!
//! 🔑 **The two headline cases are pinned to output captured from upstream itself**, run at our
//! pin under `set_debug_level MPL multilevel_autoclustering 1`. That is stronger than a test
//! written from reading the source: it would catch a misreading, which a test derived from the
//! same misreading cannot.
use vyges_mpl::thresholds::{
    set_base_thresholds, update_size_thresholds, DesignMetrics, Thresholds,
};

const AUTO: Thresholds = Thresholds { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 };
const RATIO: f32 = 10.0; // upstream's -coarsening_ratio default
const LEVELS: i32 = 2; // upstream's -max_num_level default

// ------------------------------------------------------- pinned to upstream output

#[test]
fn halos1_matches_upstreams_reported_thresholds() {
    // Upstream, verbatim:
    //   [DEBUG MPL-multilevel_autoclustering] Number of macros is below 150. Resetting number
    //     of levels to 1
    //   [DEBUG MPL-multilevel_autoclustering] num level: 1, max_macro: 5, min_macro: 1,
    //     max_inst:5000, min_inst:1000
    // Design: 150 standard cells, 2 macros, no thresholds supplied.
    let got = set_base_thresholds(
        AUTO,
        DesignMetrics { num_macro: 2, num_std_cell: 150 },
        RATIO,
        LEVELS,
        false,
    );
    assert_eq!(got.max_level, 1, "2 macros is below 150, so the level count resets");
    assert_eq!(got.thresholds.max_macro, 5);
    assert_eq!(got.thresholds.min_macro, 1);
    assert_eq!(got.thresholds.max_std_cell, 5000);
    assert_eq!(got.thresholds.min_std_cell, 1000);
}

#[test]
fn keep_clustering_data2_matches_upstreams_reported_thresholds() {
    // Upstream, verbatim:
    //   [DEBUG MPL-multilevel_autoclustering] num level: 2, max_macro: 20, min_macro: 10,
    //     max_inst:60, min_inst:20
    // Design: 8 standard cells, 2 macros, and the case supplies all four thresholds:
    //   -max_num_inst 6 -min_num_inst 2 -max_num_macro 2 -min_num_macro 1
    let supplied = Thresholds { max_macro: 2, min_macro: 1, max_std_cell: 6, min_std_cell: 2 };
    let got = set_base_thresholds(
        supplied,
        DesignMetrics { num_macro: 2, num_std_cell: 8 },
        RATIO,
        LEVELS,
        false,
    );
    // 🔑 THE subtle rule: 2 macros is below 150, yet the level count stays at 2. The reset lives
    // INSIDE the derivation block, which supplying all four thresholds skips entirely.
    assert_eq!(got.max_level, 2, "supplying thresholds also disables the level reset");
    // Only the root-level coarsening applies: ratio^(2-1) = 10.
    assert_eq!(got.thresholds.max_macro, 20);
    assert_eq!(got.thresholds.min_macro, 10);
    assert_eq!(got.thresholds.max_std_cell, 60);
    assert_eq!(got.thresholds.min_std_cell, 20);
}

// ------------------------------------------------------- the derivation rules

#[test]
fn a_partially_supplied_threshold_set_derives_all_four() {
    // ⚠️ The guard is "ANY of the four is non-positive", not per-field. Supplying only
    // -max_num_inst gets the other three derived AND the level reset applied -- so the one
    // value the user did supply is overwritten too.
    let partial = Thresholds { max_macro: 0, min_macro: 0, max_std_cell: 6, min_std_cell: 0 };
    let got = set_base_thresholds(
        partial,
        DesignMetrics { num_macro: 2, num_std_cell: 150 },
        RATIO,
        LEVELS,
        false,
    );
    assert_eq!(got.thresholds.max_std_cell, 5000, "the supplied 6 was discarded");
    assert_eq!(got.max_level, 1);
}

#[test]
fn a_fixed_macro_forces_a_single_level() {
    // Upstream's TODO: hierarchical clustering with fixed macros is not supported.
    let got = set_base_thresholds(
        Thresholds { max_macro: 2, min_macro: 1, max_std_cell: 6, min_std_cell: 2 },
        DesignMetrics { num_macro: 400, num_std_cell: 90_000 },
        RATIO,
        4,
        true,
    );
    assert_eq!(got.max_level, 1);
    // ...and with one level the root coarsening is ratio^0 = 1, so the supplied values stand.
    assert_eq!(got.thresholds.max_std_cell, 6);
}

#[test]
fn a_large_design_keeps_its_levels_and_derives_from_the_cell_count() {
    // 400 macros is above 150, so no reset: max_level stays 3.
    // min_std_cell = floor(90000 / 10^3) = 90 -> raised to 1000; max = 1000*10/2 = 5000
    // min_macro   = floor(400 / 10^3) = 0 -> 1;                 max = 1*10/2 = 5
    // coarsening  = 10^(3-1) = 100
    let got = set_base_thresholds(
        AUTO,
        DesignMetrics { num_macro: 400, num_std_cell: 90_000 },
        RATIO,
        3,
        false,
    );
    assert_eq!(got.max_level, 3, "above 150 macros, the level count survives");
    assert_eq!(got.thresholds.min_std_cell, 100_000, "1000 * 100");
    assert_eq!(got.thresholds.max_std_cell, 500_000);
    assert_eq!(got.thresholds.min_macro, 100, "1 * 100");
    assert_eq!(got.thresholds.max_macro, 500);
}

#[test]
fn the_std_cell_minimum_is_floored_at_1000_and_the_macro_minimum_at_1() {
    // ⚠️ The asymmetry is upstream's: one macro is a legitimate cluster, one standard cell is not.
    let got = set_base_thresholds(
        AUTO,
        DesignMetrics { num_macro: 1, num_std_cell: 1 },
        RATIO,
        1,
        false,
    );
    assert_eq!(got.thresholds.min_std_cell, 1000);
    assert_eq!(got.thresholds.min_macro, 1);
}

// ------------------------------------------------------- per-level

#[test]
fn level_one_leaves_the_base_thresholds_untouched() {
    // ratio^(1-1) = 1.
    let base = Thresholds { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 };
    assert_eq!(update_size_thresholds(base, 1, RATIO), base);
}

#[test]
fn each_level_divides_by_the_ratio() {
    let base =
        Thresholds { max_macro: 500, min_macro: 100, max_std_cell: 500_000, min_std_cell: 100_000 };
    let l2 = update_size_thresholds(base, 2, RATIO);
    assert_eq!(l2.max_macro, 50);
    assert_eq!(l2.min_std_cell, 10_000);
    let l3 = update_size_thresholds(base, 3, RATIO);
    assert_eq!(l3.max_macro, 5, "divides by ratio^2, not by ratio twice in sequence");
    assert_eq!(l3.min_std_cell, 1_000);
}

#[test]
fn a_degenerate_macro_minimum_becomes_one_and_recomputes_its_maximum() {
    // Dividing 1 by 10^2 truncates to 0, which is not a usable cluster size.
    let base = Thresholds { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 };
    let deep = update_size_thresholds(base, 3, RATIO);
    assert_eq!(deep.min_macro, 1, "floored");
    assert_eq!(deep.max_macro, 5, "and its maximum RECOMPUTED as 1 * 10 / 2, not left at 0");
}

#[test]
fn a_degenerate_std_cell_minimum_becomes_one_hundred_not_one_thousand() {
    // 🔑 The per-level floor is 100; the base floor is 1000. Using the same constant in both
    // places is the obvious mistake, and it would be invisible until a deep hierarchy.
    let base = Thresholds { max_macro: 500, min_macro: 100, max_std_cell: 5000, min_std_cell: 1000 };
    let deep = update_size_thresholds(base, 5, RATIO);
    assert_eq!(deep.min_std_cell, 100, "100 here, not the 1000 used at the base");
    assert_eq!(deep.max_std_cell, 500, "100 * 10 / 2");
}

#[test]
fn an_odd_ratio_truncates_the_half_rather_than_rounding_it() {
    // 🔑 Found by mutation testing, not by reading. With upstream's DEFAULT ratio of 10 the
    // `base * ratio / 2.0` step is always exact -- 10/2 is 2 -- so truncation there is
    // unobservable and every test above passes whether it truncates or rounds.
    //
    // An odd `-coarsening_ratio` exposes it: min_macro floors to 1, so max_macro is
    // 1 * 3.0 / 2.0 = 1.5, which truncates to 1 and would round to 2.
    let got = set_base_thresholds(
        AUTO,
        DesignMetrics { num_macro: 1, num_std_cell: 1 },
        3.0,
        1,
        false,
    );
    assert_eq!(got.thresholds.min_macro, 1);
    assert_eq!(got.thresholds.max_macro, 1, "1.5 truncates to 1, it does not round to 2");
}

#[test]
fn truncation_is_toward_zero_not_rounding() {
    // base 59 / 10 = 5.9 -> 5, not 6. Rounding here would drift a cluster boundary by one cell.
    let base = Thresholds { max_macro: 59, min_macro: 59, max_std_cell: 59, min_std_cell: 59 };
    let l2 = update_size_thresholds(base, 2, RATIO);
    assert_eq!(l2.max_macro, 5);
    assert_eq!(l2.max_std_cell, 5);
}
