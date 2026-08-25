// SPDX-License-Identifier: Apache-2.0
//! Coarse shaping. Rules from upstream `HierRTLMP`.
use vyges_mpl::design::Rect;
use vyges_mpl::shaping::{
    compute_width_intervals, generate_tilings_for_macro_cluster, macro_tilings, root_shape,
    Interval, Tiling,
};

fn outline(w: i64, h: i64) -> Rect {
    Rect { x_min: 0, y_min: 0, x_max: w, y_max: h }
}

fn t(w: i64, h: i64) -> Tiling {
    Tiling { width: w, height: h }
}

// ------------------------------------------------------------------ tilings

#[test]
fn every_factorisation_that_fits_is_a_tiling_in_column_order() {
    // 6 macros factor as 1x6, 2x3, 3x2, 6x1 — and `cols` runs ascending, so that is the order.
    let got = generate_tilings_for_macro_cluster(10, 10, 6, &outline(1000, 1000));
    assert_eq!(got, vec![t(10, 60), t(20, 30), t(30, 20), t(60, 10)]);
}

#[test]
fn a_tiling_must_fit_in_BOTH_dimensions() {
    // ⚠️ Tall outline: the wide tilings are rejected on width, the tall ones survive.
    let got = generate_tilings_for_macro_cluster(10, 10, 6, &outline(25, 1000));
    assert_eq!(got, vec![t(10, 60), t(20, 30)]);
    // And the mirror, so neither dimension can be the only one checked.
    let got = generate_tilings_for_macro_cluster(10, 10, 6, &outline(1000, 25));
    assert_eq!(got, vec![t(30, 20), t(60, 10)]);
}

#[test]
fn a_tiling_exactly_filling_the_outline_is_kept() {
    // 🔑 The comparison is `<=`. A cluster that exactly fills its outline is shapeable.
    assert_eq!(generate_tilings_for_macro_cluster(10, 10, 1, &outline(10, 10)), vec![t(10, 10)]);
    assert!(generate_tilings_for_macro_cluster(10, 10, 1, &outline(9, 10)).is_empty());
}

#[test]
fn a_prime_count_offers_only_the_two_extreme_shapes() {
    // The reason the n+1 retry exists, stated as a fact about primes.
    let got = generate_tilings_for_macro_cluster(10, 10, 7, &outline(1000, 1000));
    assert_eq!(got, vec![t(10, 70), t(70, 10)]);
}

// ------------------------------------------------------------------ the n+1 retry

#[test]
fn when_nothing_fits_the_search_is_retried_with_one_more_macro() {
    // 🔑 7 macros give only 1x7 and 7x1, both too long for a 45x45 outline. Pretending there are
    // 8 gives 2x4 and 4x2, which fit. ⚠️ A second, different search — not a tolerance.
    let got = macro_tilings(10, 10, 7, &outline(45, 45)).expect("shapeable via the retry");
    assert_eq!(got, vec![t(20, 40), t(40, 20)]);
}

#[test]
fn the_retry_is_only_reached_when_the_first_search_found_nothing() {
    // ⚠️ Not "add the n+1 tilings too": if anything fits n, the retry never runs and its tilings
    // never appear. 8 macros in a wide outline already fit, so no 9-macro tiling is present.
    let got = macro_tilings(10, 10, 8, &outline(1000, 1000)).expect("shapeable");
    assert!(got.contains(&t(20, 40)), "the 8-macro factorisations are there");
    assert!(!got.iter().any(|x| x.area() == 900), "and no 9-macro tiling is");
}

#[test]
fn a_cluster_that_fits_neither_count_is_unshapeable() {
    let e = macro_tilings(10, 10, 7, &outline(5, 5)).expect_err("nothing fits");
    assert_eq!(e.number_of_macros, 7, "the error names the REAL count, not the retried one");
    assert_eq!((e.macro_width, e.macro_height), (10, 10));
}

// ------------------------------------------------------------------ width intervals

#[test]
fn each_tiling_becomes_one_degenerate_interval_sorted_by_width() {
    // ℹ️ Degenerate on purpose: a macro array has a discrete set of legal widths, not a range.
    let got = compute_width_intervals(&[t(60, 10), t(10, 60), t(30, 20)]);
    assert_eq!(
        got,
        vec![
            Interval { min: 10, max: 10 },
            Interval { min: 30, max: 30 },
            Interval { min: 60, max: 60 },
        ]
    );
}

#[test]
fn no_tilings_means_no_intervals() {
    assert!(compute_width_intervals(&[]).is_empty());
}

// ------------------------------------------------------------------ tiling itself

#[test]
fn tilings_order_by_width_first_then_height() {
    // ⚠️ Upstream keeps them in a std::set, so this ordering is part of the output.
    let mut v = vec![t(20, 10), t(10, 90), t(20, 5)];
    v.sort();
    assert_eq!(v, vec![t(10, 90), t(20, 5), t(20, 10)]);
}

#[test]
fn aspect_ratio_is_height_over_width() {
    // ⚠️ Not width over height — the two disagree for every non-square tiling.
    assert_eq!(t(10, 40).aspect_ratio(), 4.0);
    assert_eq!(t(40, 10).aspect_ratio(), 0.25);
}

#[test]
fn area_does_not_overflow_a_full_size_die() {
    // 🔑 `width * height` in 32 bits overflows at ~46k x 46k DBU, which is a 23 µm square on a
    // 2000-DBU grid. Upstream widens to int64 for exactly this reason.
    let big = t(1_000_000, 1_000_000);
    assert_eq!(big.area(), 1_000_000_000_000);
}

// ------------------------------------------------------------------ the root

#[test]
fn the_root_takes_the_floorplan_shape_exactly() {
    let fp = Rect { x_min: 100, y_min: 200, x_max: 1100, y_max: 1700 };
    let (width, area) = root_shape(&fp);
    assert_eq!(width, Interval { min: 1000, max: 1000 }, "a single degenerate width");
    assert_eq!(area, 1_500_000, "and the floorplan's own area");
}
