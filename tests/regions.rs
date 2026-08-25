// SPDX-License-Identifier: Apache-2.0
//! Where unconstrained IO pins may go. Rules from upstream `HierRTLMP`.
use vyges_mpl::design::Rect;
use vyges_mpl::halo::Boundary;
use vyges_mpl::regions::{available_regions, boundary_of, boundary_rect, subtract_overlap};

fn r(x0: i64, y0: i64, x1: i64, y1: i64) -> Rect {
    Rect { x_min: x0, y_min: y0, x_max: x1, y_max: y1 }
}

const DIE: Rect = Rect { x_min: 0, y_min: 0, x_max: 100, y_max: 200 };

// ------------------------------------------------------------------ which edge

#[test]
fn a_zero_width_region_is_left_only_at_the_left_edge() {
    assert_eq!(boundary_of(&DIE, &r(0, 10, 0, 90)), Boundary::L);
    assert_eq!(boundary_of(&DIE, &r(100, 10, 100, 90)), Boundary::R);
}

#[test]
fn a_zero_height_region_is_bottom_only_at_the_bottom_edge() {
    assert_eq!(boundary_of(&DIE, &r(10, 0, 90, 0)), Boundary::B);
    assert_eq!(boundary_of(&DIE, &r(10, 200, 90, 200)), Boundary::T);
}

#[test]
fn a_region_on_no_edge_at_all_still_gets_an_answer() {
    // ⚠️ Nothing validates that the region touches the die. A zero-width line in the middle
    // reports RIGHT, and a zero-height one reports TOP, because those are the fall-through cases.
    // Reproduced deliberately: a caller passing a stray rectangle gets upstream's answer.
    assert_eq!(boundary_of(&DIE, &r(50, 10, 50, 90)), Boundary::R);
    assert_eq!(boundary_of(&DIE, &r(10, 50, 90, 50)), Boundary::T);
}

#[test]
fn width_is_tested_before_height() {
    // A degenerate point is zero-width AND zero-height; the width test wins.
    assert_eq!(boundary_of(&DIE, &r(0, 0, 0, 0)), Boundary::L);
}

// ------------------------------------------------------------------ the edges themselves

#[test]
fn each_boundary_rect_is_the_whole_edge_collapsed_to_a_line() {
    assert_eq!(boundary_rect(&DIE, Boundary::L), r(0, 0, 0, 200));
    assert_eq!(boundary_rect(&DIE, Boundary::R), r(100, 0, 100, 200));
    assert_eq!(boundary_rect(&DIE, Boundary::B), r(0, 0, 100, 0));
    assert_eq!(boundary_rect(&DIE, Boundary::T), r(0, 200, 100, 200));
}

#[test]
fn a_boundary_rect_lies_on_the_boundary_it_names() {
    // The two functions have to agree, or a region computed for one edge is filed under another.
    for b in [Boundary::B, Boundary::L, Boundary::T, Boundary::R] {
        assert_eq!(boundary_of(&DIE, &boundary_rect(&DIE, b)), b, "{b:?}");
    }
}

// ------------------------------------------------------------------ subtraction

#[test]
fn cutting_the_middle_out_of_an_edge_leaves_the_two_ends() {
    let edge = boundary_rect(&DIE, Boundary::L);
    let got = subtract_overlap(&DIE, &edge, &r(0, 50, 0, 150)).expect("same boundary");
    assert_eq!(got, vec![r(0, 0, 0, 50), r(0, 150, 0, 200)]);
}

#[test]
fn a_zero_width_piece_is_kept_because_the_test_is_an_OR() {
    // ⛔ Every region here IS zero-width; an `&&` would discard all of them and report the whole
    // edge as unavailable.
    let edge = boundary_rect(&DIE, Boundary::L);
    let got = subtract_overlap(&DIE, &edge, &r(0, 50, 0, 150)).unwrap();
    assert!(got.iter().all(|p| p.x_max - p.x_min == 0), "they are lines");
    assert_eq!(got.len(), 2);
}

#[test]
fn a_piece_that_collapses_to_a_point_is_dropped() {
    // Blocked from the very start: the leading piece is a single point and does not survive.
    let edge = boundary_rect(&DIE, Boundary::B);
    let got = subtract_overlap(&DIE, &edge, &r(0, 0, 40, 0)).unwrap();
    assert_eq!(got, vec![r(40, 0, 100, 0)], "only the trailing piece");
}

#[test]
fn a_horizontal_edge_is_cut_along_x_and_a_vertical_one_along_y() {
    // ⚠️ Getting this the wrong way round produces pieces of the full edge length every time.
    let bottom = subtract_overlap(&DIE, &boundary_rect(&DIE, Boundary::B), &r(30, 0, 70, 0));
    assert_eq!(bottom.unwrap(), vec![r(0, 0, 30, 0), r(70, 0, 100, 0)]);
    let left = subtract_overlap(&DIE, &boundary_rect(&DIE, Boundary::L), &r(0, 30, 0, 70));
    assert_eq!(left.unwrap(), vec![r(0, 0, 0, 30), r(0, 70, 0, 200)]);
}

#[test]
fn subtracting_across_boundaries_is_refused() {
    // Upstream calls this a critical error; it cannot arise from the search, which groups first.
    assert!(subtract_overlap(&DIE, &boundary_rect(&DIE, Boundary::L), &r(30, 0, 70, 0)).is_none());
}

// ------------------------------------------------------------------ the search

#[test]
fn with_nothing_blocked_every_edge_is_available_in_enum_order() {
    // 🔑 B, L, T, R — upstream keeps the boundaries in a `std::map`, so this is the enum order,
    // not the order they were written down in, and it is observable in the result.
    assert_eq!(
        available_regions(&DIE, &[]),
        vec![
            r(0, 0, 100, 0),
            r(0, 0, 0, 200),
            r(0, 200, 100, 200),
            r(100, 0, 100, 200),
        ]
    );
}

#[test]
fn a_blocked_region_is_removed_from_its_own_edge_only() {
    let got = available_regions(&DIE, &[r(0, 50, 0, 150)]);
    assert!(got.contains(&r(0, 0, 0, 50)) && got.contains(&r(0, 150, 0, 200)));
    assert!(got.contains(&r(0, 0, 100, 0)), "the bottom edge is untouched");
    assert_eq!(got.len(), 5, "three edges whole, the left one in two pieces");
}

#[test]
fn two_blocked_regions_on_one_edge_are_both_removed() {
    let got = available_regions(&DIE, &[r(0, 20, 0, 40), r(0, 120, 0, 140)]);
    let left: Vec<&Rect> = got.iter().filter(|x| x.x_max == 0).collect();
    assert_eq!(left, vec![&r(0, 0, 0, 20), &r(0, 40, 0, 120), &r(0, 140, 0, 200)]);
}

#[test]
fn a_region_that_only_OVERLAPS_is_left_alone() {
    // ⚠️ The test is `contains`, not `overlaps`. A blocked region hanging off the end of the edge
    // is not subtracted at all, and the space it covers stays available. This reads like a defect
    // and is the behaviour being matched.
    let got = available_regions(&DIE, &[r(0, -50, 0, 50)]);
    assert!(got.contains(&r(0, 0, 0, 200)), "the whole left edge survives");
}

#[test]
fn a_blocked_region_covering_a_whole_edge_leaves_nothing_of_it() {
    let got = available_regions(&DIE, &[r(0, 0, 0, 200)]);
    assert!(!got.iter().any(|x| x.x_max == 0 && x.x_min == 0), "the left edge is gone");
    assert_eq!(got.len(), 3, "and the other three remain");
}

#[test]
fn a_blocked_region_flush_with_the_edge_start_is_still_contained() {
    // 🔑 Containment is INCLUSIVE on every side. A blocked region that begins exactly where the
    // edge begins — every comparison at equality — is contained, and is subtracted. A strict
    // comparison would leave it in place and report the space as available.
    let got = available_regions(&DIE, &[r(0, 0, 0, 60)]);
    assert!(!got.contains(&r(0, 0, 0, 200)), "the left edge was cut, not left whole");
    assert!(got.contains(&r(0, 60, 0, 200)), "what remains starts where the block ended");
}

// ------------------------------------------------------------------ pin access blockages

use vyges_mpl::regions::{
    clamp_depth, io_density_factor, pin_access_blockage, region_length, scale_depth,
    BoundaryRegion,
};
use vyges_mpl::shaping::DepthLimits;

const LIMITS: DepthLimits = DepthLimits { x_min: 10, x_max: 100, y_min: 20, y_max: 200 };

fn region(line: Rect) -> BoundaryRegion {
    BoundaryRegion { boundary: boundary_of(&DIE, &line), line }
}

#[test]
fn a_regions_length_is_the_side_that_is_not_zero() {
    // ℹ️ Upstream computes `margin() / 2`, which is `dx + dy`; one of them is zero for a line.
    assert_eq!(region_length(&r(0, 30, 0, 90)), 60);
    assert_eq!(region_length(&r(30, 0, 90, 0)), 60);
}

#[test]
fn a_region_of_zero_length_has_zero_length() {
    assert_eq!(region_length(&r(0, 40, 0, 40)), 0);
}

#[test]
fn a_region_with_none_of_the_ios_still_gets_its_base_depth() {
    // 🔑 The factor starts at ONE and grows; it is not a share of the depth but an addition to it.
    assert_eq!(io_density_factor(0, 10), 1.0);
    assert_eq!(io_density_factor(10, 10), 2.0, "all of them doubles the depth");
    assert_eq!(io_density_factor(5, 10), 1.5);
}

#[test]
fn scaling_a_depth_truncates() {
    assert_eq!(scale_depth(10, 1.5), 15);
    assert_eq!(scale_depth(10, 1.19), 11, "11.9 becomes 11, not 12");
}

// ------------------------------------------------------------------ the clamp

#[test]
fn a_vertical_boundary_is_clamped_by_the_X_limits() {
    // 🔑 Reads inverted and is correct: `is_vertical` asks whether the EDGE runs vertically, and
    // a blockage on the left or right edge grows in x. Swapping the pair changes the depth on
    // every edge without changing any shape, which is why it needs saying.
    assert_eq!(clamp_depth(500, Boundary::L, &LIMITS), 100, "the x maximum");
    assert_eq!(clamp_depth(500, Boundary::B, &LIMITS), 200, "the y maximum");
    assert_eq!(clamp_depth(1, Boundary::L, &LIMITS), 10, "the x minimum");
    assert_eq!(clamp_depth(1, Boundary::B, &LIMITS), 20, "the y minimum");
}

#[test]
fn a_depth_between_the_limits_is_left_alone() {
    assert_eq!(clamp_depth(50, Boundary::L, &LIMITS), 50);
}

#[test]
fn a_depth_exactly_on_a_limit_is_left_alone() {
    // ⚠️ The comparisons are strict, so neither endpoint is rewritten to itself.
    assert_eq!(clamp_depth(100, Boundary::L, &LIMITS), 100);
    assert_eq!(clamp_depth(10, Boundary::L, &LIMITS), 10);
}

#[test]
fn the_clamp_is_an_else_if_and_the_ORDER_of_the_two_tests_shows() {
    // ⚠️ `if > max … else if < min …`, not two independent clamps. The difference is invisible
    // while `min <= max`, and the engine never produces anything else — so this pins the
    // TRANSCRIPTION, with limits chosen to make the order observable.
    //
    // ⛔ A depth of 50 would prove nothing: every arrangement of these two tests returns 5 for it.
    // Three depths are needed, one either side of each limit.
    let odd = DepthLimits { x_min: 90, x_max: 5, y_min: 0, y_max: 0 };
    assert_eq!(clamp_depth(50, Boundary::L, &odd), 5, "above the maximum: the first branch");
    assert_eq!(
        clamp_depth(3, Boundary::L, &odd),
        90,
        "below the maximum, so it falls through to the minimum and is RAISED past the maximum —          clamping min first and max second would give 5"
    );
    assert_eq!(
        clamp_depth(5, Boundary::L, &odd),
        90,
        "exactly ON the maximum: `>` is strict, so it falls through — `>=` would give 5"
    );
}

// ------------------------------------------------------------------ the blockage itself

#[test]
fn a_blockage_grows_INWARD_from_its_edge() {
    // Left grows right, right grows left, bottom grows up, top grows down — always into the core.
    assert_eq!(pin_access_blockage(&region(r(0, 20, 0, 80)), 30, &LIMITS), r(0, 20, 30, 80));
    assert_eq!(
        pin_access_blockage(&region(r(100, 20, 100, 80)), 30, &LIMITS),
        r(70, 20, 100, 80)
    );
    assert_eq!(pin_access_blockage(&region(r(20, 0, 80, 0)), 30, &LIMITS), r(20, 0, 80, 30));
    assert_eq!(
        pin_access_blockage(&region(r(20, 200, 80, 200)), 30, &LIMITS),
        r(20, 170, 80, 200)
    );
}

#[test]
fn a_blockage_keeps_the_length_of_its_region() {
    // ⚠️ Only the depth direction changes; growing the wrong axis would stretch it along the edge.
    let b = pin_access_blockage(&region(r(0, 20, 0, 80)), 30, &LIMITS);
    assert_eq!(b.y_max - b.y_min, 60);
}

#[test]
fn a_blockage_is_clamped_before_it_is_drawn() {
    // The depth asked for is 5000; the x maximum is 100.
    assert_eq!(pin_access_blockage(&region(r(0, 20, 0, 80)), 5000, &LIMITS), r(0, 20, 100, 80));
}

// ------------------------------------------------------------------ the builders

use vyges_mpl::regions::{blockages_for_available_regions, blockages_for_regions, IoRegion};

fn io(line: Rect, ios: i64) -> IoRegion {
    IoRegion { region: region(line), ios }
}

/// A base depth that just returns the span, so the span is visible in the result.
fn span_as_depth(span: i64) -> i64 {
    span
}

#[test]
fn the_span_is_summed_over_ALL_regions_and_the_base_depth_computed_once() {
    // 🔑 Two regions of 30 give a span of 60, and BOTH get a depth derived from 60 — not from
    // their own 30. A longer region lowers the depth of every region, its own included.
    let got = blockages_for_regions(
        &[io(r(0, 0, 0, 30), 1), io(r(0, 40, 0, 70), 1)],
        2,
        &span_as_depth,
        &DepthLimits { x_min: 0, x_max: 10_000, y_min: 0, y_max: 10_000 },
    );
    // span 60, each carrying half the IOs -> factor 1.5 -> depth 90.
    assert_eq!(got, vec![r(0, 0, 90, 30), r(0, 40, 90, 70)]);
}

#[test]
fn a_region_with_more_ios_gets_a_deeper_blockage() {
    let limits = DepthLimits { x_min: 0, x_max: 10_000, y_min: 0, y_max: 10_000 };
    let got = blockages_for_regions(
        &[io(r(0, 0, 0, 50), 1), io(r(0, 60, 0, 110), 3)],
        4,
        &span_as_depth,
        &limits,
    );
    // span 100; factors 1.25 and 1.75 -> depths 125 and 175.
    assert_eq!(got[0].x_max, 125);
    assert_eq!(got[1].x_max, 175);
}

#[test]
fn no_regions_means_the_base_depth_is_never_COMPUTED() {
    // ⛔ The empty result is not what the guard is for — mapping over an empty list gives that
    // anyway. What it prevents is calling the base-depth function with a span of ZERO, which
    // divides by it. Upstream reaches `(int)inf` there, which is undefined; we would reach a
    // silent 0. Either way it must not be reached, so the probe records whether it was.
    let called = std::cell::Cell::new(false);
    let watch = |span: i64| {
        called.set(true);
        span
    };
    assert!(blockages_for_regions(&[], 0, &watch, &LIMITS).is_empty());
    assert!(!called.get(), "the base depth was computed for an empty span");
}

#[test]
fn available_regions_all_get_the_SAME_depth() {
    // 🔑 No density factor here. Unequal lengths, identical depth.
    let limits = DepthLimits { x_min: 0, x_max: 10_000, y_min: 0, y_max: 10_000 };
    let got = blockages_for_available_regions(
        &[region(r(0, 0, 0, 20)), region(r(0, 30, 0, 130))],
        true,
        &span_as_depth,
        &limits,
    );
    assert_eq!(got[0].x_max, 120, "the span, not this region's own 20");
    assert_eq!(got[1].x_max, 120, "and the same for the longer one");
}

#[test]
fn with_nothing_blocked_no_available_region_casts_a_blockage() {
    // ⚠️ The guard is on the BLOCKED regions, not on the available ones — which are non-empty
    // here, and still produce nothing.
    let got = blockages_for_available_regions(
        &[region(r(0, 0, 0, 200))],
        false,
        &span_as_depth,
        &LIMITS,
    );
    assert!(got.is_empty());
}

#[test]
fn placement_blockages_are_taken_as_they_stand() {
    // ℹ️ No clipping, no union — unlike the feasibility test, which asks how much AREA they cover.
    let bs = [r(0, 0, 10, 10), r(5, 5, 15, 15)];
    assert_eq!(vyges_mpl::shaping::placement_blockages(&bs), bs.to_vec());
}
