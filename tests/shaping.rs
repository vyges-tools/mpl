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

// ------------------------------------------------------------------ the recursion

use vyges_mpl::cluster::{Cluster, ClusterType};
use vyges_mpl::shaping::{calculate_children_tilings, ShapingCtx, ShapingRefusal};

/// A macro cluster holding `n` macros, all instance indices.
fn macro_cluster(id: i32, name: &str, macros: &[usize]) -> Cluster {
    let mut c = Cluster::new(id, name);
    c.cluster_type = ClusterType::HardMacro;
    c.leaf_macros = macros.to_vec();
    c.metrics.num_macro = macros.len() as i32;
    c
}

fn ctx(w: i64, h: i64) -> ShapingCtx<'static> {
    ShapingCtx { outline: outline(w, h), macro_dims: &|_| (10, 10) }
}

#[test]
fn a_cluster_with_no_macros_is_not_shaped_and_neither_is_anything_below_it() {
    // ⚠️ The base case is `num_macro == 0`, not "is a leaf". A standard-cell branch has no shape
    // to choose, and descending into it would shape clusters upstream never touches.
    let mut root = Cluster::new(0, "root");
    root.cluster_type = ClusterType::StdCell;
    root.children.push(macro_cluster(1, "M", &[0, 1]));
    calculate_children_tilings(&mut root, &ctx(1000, 1000)).expect("no macros, nothing to do");
    assert!(root.tilings.is_empty());
    assert!(root.children[0].tilings.is_empty(), "the child was never reached");
}

#[test]
fn a_hard_macro_cluster_gets_the_tilings_of_its_macro_count() {
    let mut c = macro_cluster(1, "M", &[0, 1, 2, 3]);
    calculate_children_tilings(&mut c, &ctx(1000, 1000)).expect("shapeable");
    assert_eq!(c.tilings, vec![t(10, 40), t(20, 20), t(40, 10)]);
}

#[test]
fn a_FIXED_macro_cluster_is_left_with_no_tilings() {
    // 🔑 A fixed macro is not the placer's to shape. ⚠️ "No tilings" here is not the empty result
    // of a search — the search never runs.
    let mut c = macro_cluster(1, "M", &[0]);
    c.is_fixed_macro = true;
    calculate_children_tilings(&mut c, &ctx(1, 1)).expect("not shaped, and not an error either");
    assert!(c.tilings.is_empty());
}

#[test]
fn a_parent_with_one_macro_bearing_child_takes_that_childs_tilings() {
    let mut root = Cluster::new(0, "root");
    root.metrics.num_macro = 2;
    root.children.push(Cluster::new(1, "glue"));
    root.children.push(macro_cluster(2, "M", &[0, 1]));
    calculate_children_tilings(&mut root, &ctx(1000, 1000)).expect("shortcut");
    assert_eq!(root.tilings, vec![t(10, 20), t(20, 10)]);
    assert_eq!(root.tilings, root.children[1].tilings, "verbatim, not recomputed");
}

#[test]
fn a_lone_FIXED_macro_leaves_the_parent_with_no_tilings() {
    // ⚠️ Upstream re-scans the children for the first with `num_macro > 0` instead of reusing the
    // contributor it just built. A fixed macro cluster contributes, but has no tilings to give.
    let mut root = Cluster::new(0, "root");
    root.metrics.num_macro = 1;
    // ⚠️ A fixed macro cluster still REPORTS its macro — `fixed_covers` prints
    // `Type: Fixed Macro Leaf, Macros: 1`. What it does not have is tilings.
    let mut fixed = macro_cluster(1, "F", &[0]);
    fixed.is_fixed_macro = true;
    root.children.push(fixed);
    calculate_children_tilings(&mut root, &ctx(1000, 1000)).expect("no annealing needed");
    assert!(root.tilings.is_empty(), "it copied the fixed cluster's tilings, and there are none");
}

#[test]
fn two_macro_bearing_children_need_the_annealing_search() {
    // ⛔ Refused by name. A plausible tiling set is not the same tiling set.
    let mut root = Cluster::new(0, "root");
    root.metrics.num_macro = 2;
    root.children.push(macro_cluster(1, "A", &[0]));
    root.children.push(macro_cluster(2, "B", &[1]));
    let e = calculate_children_tilings(&mut root, &ctx(1000, 1000)).expect_err("needs SA");
    assert_eq!(e, ShapingRefusal::NeedsAnnealing(0), "and it names the cluster");
}

#[test]
fn a_fixed_macro_beside_a_movable_one_still_needs_the_search() {
    // 🔑 This is why `fixed_covers` and `fixed_macros*` are in the annealing group despite having
    // one movable macro apiece: the fixed one occupies space the parent must shape around.
    let mut root = Cluster::new(0, "root");
    root.metrics.num_macro = 2;
    let mut fixed = macro_cluster(1, "F", &[0]);
    fixed.is_fixed_macro = true;
    root.children.push(fixed);
    root.children.push(macro_cluster(2, "M", &[1]));
    let e = calculate_children_tilings(&mut root, &ctx(1000, 1000)).expect_err("needs SA");
    assert_eq!(e, ShapingRefusal::NeedsAnnealing(0));
}

#[test]
fn children_are_shaped_before_the_parent_reads_them() {
    // The order IS the algorithm: the shortcut copies the child's tilings, so a parent shaped
    // first would copy an empty list.
    let mut root = Cluster::new(0, "root");
    root.metrics.num_macro = 4;
    let mut branch = Cluster::new(1, "branch");
    branch.metrics.num_macro = 4;
    branch.children.push(macro_cluster(2, "M", &[0, 1, 2, 3]));
    root.children.push(branch);
    calculate_children_tilings(&mut root, &ctx(1000, 1000)).expect("shortcut twice");
    assert_eq!(root.children[0].children[0].tilings.len(), 3, "the leaf was shaped");
    assert_eq!(root.children[0].tilings, root.children[0].children[0].tilings);
    assert_eq!(root.tilings, root.children[0].tilings, "and it reached the root");
}

#[test]
fn an_unshapeable_macro_cluster_names_itself() {
    let mut root = Cluster::new(0, "root");
    root.metrics.num_macro = 7;
    root.children.push(macro_cluster(9, "M", &[0, 1, 2, 3, 4, 5, 6]));
    let e = calculate_children_tilings(&mut root, &ctx(5, 5)).expect_err("nothing fits");
    match e {
        ShapingRefusal::Unshapeable(id, why) => {
            assert_eq!(id, 9);
            assert_eq!(why.number_of_macros, 7);
        }
        other => panic!("expected an unshapeable cluster, got {other:?}"),
    }
}

// ------------------------------------------------------------------ pin access depth

use vyges_mpl::shaping::{pin_access_base_depth, pin_access_depth_limits, DepthLimits};

#[test]
fn the_depth_limits_are_ten_and_four_percent_of_the_die() {
    // A tiling that leaves plenty of margin, so the tight-design override does not fire.
    let got = pin_access_depth_limits(&outline(1000, 2000), t(100, 200));
    assert_eq!(got, DepthLimits { x_min: 40, x_max: 100, y_min: 80, y_max: 200 });
}

#[test]
fn the_limits_are_per_axis_and_a_square_die_hides_that() {
    // ⚠️ A square die makes the x and y limits equal, so an implementation that computed both
    // from `dx` would pass. This one is deliberately not square.
    let got = pin_access_depth_limits(&outline(1000, 5000), t(10, 10));
    assert_ne!(got.x_max, got.y_max);
    assert_eq!((got.x_max, got.y_max), (100, 500));
}

#[test]
fn a_design_tight_in_BOTH_directions_replaces_BOTH_minima() {
    // 🔑 The override is all-or-nothing. The tiling leaves 5 on each side of a 1000-wide die,
    // below the 40 and 80 the proportions would give.
    let got = pin_access_depth_limits(&outline(1000, 2000), t(990, 1990));
    assert_eq!(got, DepthLimits { x_min: 5, x_max: 100, y_min: 5, y_max: 200 });
}

#[test]
fn a_design_tight_in_ONE_direction_keeps_both_proportional_minima() {
    // ⛔ It is an `&&`, not a per-axis test. Tight in x only: nothing changes, on either axis.
    let got = pin_access_depth_limits(&outline(1000, 2000), t(990, 100));
    assert_eq!(got.x_min, 40, "still the proportional minimum, not the 5 the tiling would give");
    assert_eq!(got.y_min, 80);
}

#[test]
fn the_tiling_margin_truncates_rather_than_rounding() {
    // (1000 - 991) / 2 = 4, not 4.5.
    let got = pin_access_depth_limits(&outline(1000, 2000), t(991, 1991));
    assert_eq!((got.x_min, got.y_min), (4, 4));
}

#[test]
fn the_base_depth_comes_from_the_std_cell_children() {
    // 1000 area over a span of 10, with no macros at all: the factor is 1.
    assert_eq!(pin_access_base_depth(1000, 0, 0, 500, 10), Ok(100));
}

#[test]
fn the_mixed_children_are_used_ONLY_when_there_are_no_std_cell_ones() {
    // 🔑 Two passes, not one condition. With std-cell children present the mixed ones contribute
    // nothing; with none, they are the whole of it.
    assert_eq!(pin_access_base_depth(1000, 9999, 0, 500, 10), Ok(100), "the mixed area is ignored");
    assert_eq!(pin_access_base_depth(0, 1000, 0, 500, 10), Ok(100), "and used when alone");
}

#[test]
fn macro_dominance_is_SQUARED_so_it_bites_hard() {
    // Half the floorplan taken by macros leaves a QUARTER of the depth, not a half.
    assert_eq!(pin_access_base_depth(1000, 0, 250, 500, 10), Ok(25));
}

#[test]
fn a_root_of_zero_area_is_refused_rather_than_divided_by() {
    assert_eq!(pin_access_base_depth(1000, 0, 0, 0, 10), Err(vyges_mpl::shaping::RootAreaIsZero));
}

// ------------------------------------------------------------------ the composer

use vyges_mpl::regions::{BoundaryRegion, IoRegion};
use vyges_mpl::halo::Boundary;
use vyges_mpl::shaping::{run_coarse_shaping, CoarseInput};

const DIE: Rect = Rect { x_min: 0, y_min: 0, x_max: 1000, y_max: 1000 };

fn base(die: Rect) -> CoarseInput<'static> {
    CoarseInput {
        die,
        floorplan: die,
        has_only_macros: false,
        has_io_pads: false,
        top_std_cell_area: 5_000,
        blockages: &[],
        macro_dims: &|_| (100, 100),
        io_bundles: &[],
        fixed_ios: 1,
        constrained_regions: &[],
        unfixed_ios: 1,
        blocked_regions_for_pins: &[],
        has_unconstrained_ios: false,
        base_depth: &|_| 50,
    }
}

fn root_with_one_macro_child() -> Cluster {
    let mut root = Cluster::new(0, "root");
    root.metrics.num_macro = 4;
    root.children.push(macro_cluster(1, "M", &[0, 1, 2, 3]));
    root
}

#[test]
fn the_root_takes_the_FLOORPLAN_shape_not_the_die() {
    // ⛔ A fixture with `floorplan == die` cannot tell these apart, and a global fence is exactly
    // the case where they differ — the fence reaches the whole stage only through this.
    let mut root = root_with_one_macro_child();
    let mut input = base(DIE);
    let fenced = Rect { x_min: 100, y_min: 100, x_max: 700, y_max: 900 };
    input.floorplan = fenced;
    let got = run_coarse_shaping(&mut root, &input).expect("shapeable");
    assert_eq!(
        got.root_shape,
        (vyges_mpl::shaping::Interval { min: 600, max: 600 }, 480_000),
        "the fenced width and area, not the die's 1000 and 1,000,000"
    );
}

#[test]
fn a_design_of_only_macros_stops_after_the_root_shape() {
    // 🔑 MPL-27 RETURNS. No tilings, no blockages of either kind — and the root is retyped.
    let mut root = root_with_one_macro_child();
    let mut input = base(DIE);
    input.has_only_macros = true;
    input.blockages = &[Rect { x_min: 0, y_min: 0, x_max: 10, y_max: 10 }];
    let got = run_coarse_shaping(&mut root, &input).expect("not an error");
    assert_eq!(root.cluster_type, ClusterType::HardMacro, "the root is retyped");
    assert!(root.children[0].tilings.is_empty(), "and nothing was shaped");
    assert!(got.placement_blockages.is_empty(), "not even the placement blockages");
}

#[test]
fn the_tilings_reach_the_root_before_the_depth_limits_read_them() {
    // The limits are derived from the root's FIRST tiling, so a composer that computed them
    // before the descent would read an empty list.
    let mut root = root_with_one_macro_child();
    let got = run_coarse_shaping(&mut root, &base(DIE)).expect("shapeable");
    assert!(!root.tilings.is_empty(), "the shortcut carried the child's tilings up");
    assert_eq!(got.depth_limits.x_max, 100, "and the limits were computed from the die");
}

#[test]
fn a_design_with_io_pads_casts_no_pin_access_blockages() {
    let mut root = root_with_one_macro_child();
    let mut input = base(DIE);
    input.has_io_pads = true;
    input.blocked_regions_for_pins = &[Rect { x_min: 0, y_min: 0, x_max: 0, y_max: 500 }];
    input.has_unconstrained_ios = true;
    let got = run_coarse_shaping(&mut root, &input).expect("shapeable");
    assert!(got.io_blockages.is_empty(), "the pads carry the connectivity instead");
}

#[test]
fn a_design_with_no_standard_cells_casts_no_pin_access_blockages() {
    // ⚠️ Upstream's reason, verbatim: it avoids creating blockages with zero depth.
    let mut root = root_with_one_macro_child();
    let mut input = base(DIE);
    input.top_std_cell_area = 0;
    input.blocked_regions_for_pins = &[Rect { x_min: 0, y_min: 0, x_max: 0, y_max: 500 }];
    input.has_unconstrained_ios = true;
    let got = run_coarse_shaping(&mut root, &input).expect("shapeable");
    assert!(got.io_blockages.is_empty());
}

#[test]
fn the_three_builders_append_in_upstreams_order() {
    // ⚠️ Bundles, then available regions, then constraint regions. The placer reads the list in
    // order, so this is output, not bookkeeping.
    let bundle = IoRegion {
        region: BoundaryRegion {
            // ⛔ Deliberately NOT at the origin. A left edge grown by 100 and a bottom edge
            // grown by 100 are the SAME square when both start at (0,0), which left the order
            // unobservable even with whole-rectangle assertions.
            line: Rect { x_min: 0, y_min: 200, x_max: 0, y_max: 300 },
            boundary: Boundary::L,
        },
        ios: 1,
    };
    // ⛔ The available regions are DERIVED, not supplied — so to leave exactly one, every other
    // edge is blocked in full. A blocked region that covers a whole edge removes it: both pieces
    // of the subtraction collapse to a single point and are dropped. What survives here is the
    // right edge below y=100, which is the one region this test wants to see land second.
    let blocked = [
        Rect { x_min: 0, y_min: 0, x_max: 1000, y_max: 0 },
        Rect { x_min: 0, y_min: 0, x_max: 0, y_max: 1000 },
        Rect { x_min: 0, y_min: 1000, x_max: 1000, y_max: 1000 },
        Rect { x_min: 1000, y_min: 100, x_max: 1000, y_max: 1000 },
    ];
    let constrained = IoRegion {
        region: BoundaryRegion {
            line: Rect { x_min: 400, y_min: 0, x_max: 500, y_max: 0 },
            boundary: Boundary::B,
        },
        ios: 1,
    };
    let mut root = root_with_one_macro_child();
    let mut input = base(DIE);
    input.io_bundles = std::slice::from_ref(&bundle);
    input.blocked_regions_for_pins = &blocked;
    input.has_unconstrained_ios = true;
    input.constrained_regions = std::slice::from_ref(&constrained);
    // ⚠️ A different IO total for the constraint regions, so its depth differs from the bundle's.
    // With both at 1 the two rectangles come out IDENTICAL and the order is unobservable again.
    input.unfixed_ios = 4;
    let got = run_coarse_shaping(&mut root, &input).expect("shapeable");
    assert_eq!(got.io_blockages.len(), 3);
    // ⛔ Assert the WHOLE rectangle of each. Checking one coordinate is not enough: the bundle
    // and the constraint region below both have `x_min == 0`, so swapping the two builders left
    // the weaker assertions green.
    assert_eq!(
        got.io_blockages[0],
        Rect { x_min: 0, y_min: 200, x_max: 100, y_max: 300 },
        "the bundle: base 50 doubled by carrying all one fixed IO"
    );
    assert_eq!(
        got.io_blockages[1],
        Rect { x_min: 950, y_min: 0, x_max: 1000, y_max: 100 },
        "the available region: base 50, no density factor at all"
    );
    assert_eq!(
        got.io_blockages[2],
        Rect { x_min: 400, y_min: 0, x_max: 500, y_max: 62 },
        "the constraint region: base 50 scaled by 1.25 and truncated"
    );
}

#[test]
fn the_placement_blockages_come_through_untouched() {
    let bs = [Rect { x_min: 0, y_min: 0, x_max: 10, y_max: 10 }];
    let mut root = root_with_one_macro_child();
    let mut input = base(DIE);
    input.blockages = &bs;
    let got = run_coarse_shaping(&mut root, &input).expect("shapeable");
    assert_eq!(got.placement_blockages, bs.to_vec());
}

// ---------------------------------------------------------------- the search, in sequence

/// ⛔ **The gate closes the whole search, not one builder.** With no unconstrained IO cluster the
/// available-region list stays empty, so however much of the boundary is free, none of it casts a
/// blockage. Upstream `searchAvailableRegionsForUnconstrainedPins` returns before computing
/// anything.
#[test]
fn without_unconstrained_ios_no_available_region_is_searched() {
    let mut root = root_with_one_macro_child();
    let mut input = base(DIE);
    input.blocked_regions_for_pins = &[Rect { x_min: 0, y_min: 100, x_max: 0, y_max: 200 }];
    input.has_unconstrained_ios = false;
    let closed = run_coarse_shaping(&mut root, &input).expect("shapeable");

    let mut root = root_with_one_macro_child();
    input.has_unconstrained_ios = true;
    let open = run_coarse_shaping(&mut root, &input).expect("shapeable");

    assert!(closed.io_blockages.is_empty(), "the gate was shut");
    assert!(!open.io_blockages.is_empty(), "and the fixture can actually produce some");
}

/// 🔑 **The order is the algorithm.** Upstream runs the search BETWEEN the tilings and the
/// blockages, so its `Found blocked region` lines precede the pin-access depth table that the
/// blockages are clamped by. A search hoisted above the tilings would emit them the other way
/// round, and this is the cheapest place that difference is visible.
#[test]
fn the_search_traces_before_the_depth_table() {
    use vyges_mpl::shaping::run_coarse_shaping_traced;
    use vyges_mpl::trace::CoarseTrace;

    let mut root = root_with_one_macro_child();
    let mut input = base(DIE);
    input.blocked_regions_for_pins = &[Rect { x_min: 0, y_min: 100, x_max: 0, y_max: 200 }];
    input.has_unconstrained_ios = true;

    let mut trace = CoarseTrace::recording();
    run_coarse_shaping_traced(&mut root, &input, 2000, &mut trace).expect("shapeable");
    let out = trace.finish();

    let found = out.find("Found blocked region").expect("the search traced");
    let table = out.find("Pin Access Depth").expect("the limits traced");
    assert!(found < table, "the search runs before the blockages it feeds");
}

/// ⚠️ **A blocked region that covers a whole edge REMOVES it.** Both pieces of the subtraction
/// collapse to a single point and are dropped, so the edge casts no blockage at all — which is
/// how the order fixture above leaves exactly one region standing.
#[test]
fn an_edge_blocked_end_to_end_leaves_no_available_region() {
    let mut root = root_with_one_macro_child();
    let mut input = base(DIE);
    let whole_left = [Rect { x_min: 0, y_min: 0, x_max: 0, y_max: 1000 }];
    input.blocked_regions_for_pins = &whole_left;
    input.has_unconstrained_ios = true;
    let got = run_coarse_shaping(&mut root, &input).expect("shapeable");
    assert!(
        !got.io_blockages.iter().any(|b| b.x_min == 0 && b.x_max <= 0),
        "the left edge was consumed by the blocked region"
    );
}
