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
