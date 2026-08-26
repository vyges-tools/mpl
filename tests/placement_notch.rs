// SPDX-License-Identifier: Apache-2.0
//! The notch term: where an empty region is too thin to be useful.

use vyges_mpl::placement::{
    check_notch_vicinity, notch_grid, notch_penalty, single_notch_penalty, AreaKind, NotchMacro,
    NotchVicinity,
};

fn nm(x: i32, y: i32, w: i32, h: i32, kind: AreaKind) -> NotchMacro {
    NotchMacro { x, y, width: w, height: h, kind }
}

fn hard(x: i32, y: i32, w: i32, h: i32) -> NotchMacro {
    nm(x, y, w, h, AreaKind::HardMacroCluster)
}

// ---------------------------------------------------------------- the grid

/// 🔑 **Every obstructing macro's edges become grid lines, plus the outline's own two.**
#[test]
fn the_grid_is_cut_at_every_macro_edge() {
    let macros = [hard(200, 300, 100, 100)];
    let (xs, ys) = notch_grid(&macros, (1000, 1000));
    assert_eq!(xs, vec![0, 200, 300, 1000]);
    assert_eq!(ys, vec![0, 300, 400, 1000]);
}

/// ⛔ **A standard-cell cluster, a blockage, an IO cluster and a FIXED MACRO all obstruct
/// nothing.** The fixed macro is the surprising one: its soft macro has no cluster behind it, so
/// upstream's two predicates both come back false.
#[test]
fn only_macro_and_mixed_clusters_cut_the_grid() {
    let outline = (1000, 1000);
    let bare = notch_grid(&[], outline);
    for kind in [
        AreaKind::StdCellCluster,
        AreaKind::Blockage,
        AreaKind::IoCluster,
        AreaKind::FixedMacro,
    ] {
        let macros = [nm(200, 300, 100, 100, kind)];
        assert_eq!(notch_grid(&macros, outline), bare, "{kind:?} should not cut the grid");
    }
    // A mixed cluster does.
    let mixed = [nm(200, 300, 100, 100, AreaKind::MixedCluster)];
    assert_ne!(notch_grid(&mixed, outline), bare);
}

/// ⚠️ **Near-coincident lines are coalesced at a hundredth of the outline's extent**, and it is
/// strictly greater — a gap of exactly one tolerance vanishes.
#[test]
fn near_coincident_grid_lines_are_coalesced() {
    // Tolerance is 1000 / 100 = 10 on both axes.
    let macros = [hard(400, 400, 100, 100), hard(409, 409, 100, 100)];
    let (xs, ys) = notch_grid(&macros, (1000, 1000));
    // 400 and 409 are 9 apart, as are 500 and 509: each pair collapses to the larger.
    assert_eq!(xs, vec![0, 409, 509, 1000]);
    assert_eq!(ys, vec![0, 409, 509, 1000]);

    // Exactly one tolerance apart still collapses — the test is `>`, not `>=`.
    let exact = [hard(400, 400, 100, 100), hard(410, 410, 100, 100)];
    let (xs, _) = notch_grid(&exact, (1000, 1000));
    assert_eq!(xs, vec![0, 410, 510, 1000]);

    // One more unit and both survive.
    let apart = [hard(400, 400, 100, 100), hard(411, 411, 100, 100)];
    let (xs, _) = notch_grid(&apart, (1000, 1000));
    assert_eq!(xs, vec![0, 400, 411, 500, 511, 1000]);
}

/// ⛔ **The survivor of a coalesced group is the LARGEST**, because the walk runs downwards from
/// the top. Keeping the smallest is the natural way to write this and puts a macro's far edge on
/// the wrong side of [`segment_index`]'s `lower_bound`.
#[test]
fn a_coalesced_group_keeps_its_largest_member() {
    // Three lines within one tolerance of each other, at 300, 305 and 309.
    let macros = [hard(300, 0, 5, 10), hard(309, 0, 1, 10)];
    let (xs, _) = notch_grid(&macros, (1000, 1000));
    assert!(xs.contains(&310), "the far edge of the higher macro survives");
    assert!(!xs.contains(&300), "the smallest of the group is gone, not the largest");
}

/// ⚠️ **Each axis coalesces at its OWN extent's tolerance** — x from the width, y from the height.
/// A square outline cannot tell the two apart, which is why this one is lopsided.
#[test]
fn each_axis_coalesces_at_its_own_tolerance() {
    // A 4000 x 200 outline: the x tolerance is 40, the y tolerance is 2.
    // Edges 20 apart survive on y (20 > 2) and collapse on x (20 is not > 40).
    let macros = [hard(1000, 50, 500, 20), hard(1020, 70, 500, 20)];
    let (xs, ys) = notch_grid(&macros, (4000, 200));
    assert_eq!(xs, vec![0, 1020, 1520, 4000], "x collapsed both pairs");
    assert_eq!(ys, vec![0, 50, 70, 90, 200], "y kept every edge");
}

/// ⚠️ An outline narrower than 100 units gets a tolerance of ZERO — integer division — so nothing
/// coalesces at all.
#[test]
fn a_tiny_outline_coalesces_nothing() {
    let macros = [hard(10, 10, 1, 1), hard(11, 11, 1, 1)];
    let (xs, ys) = notch_grid(&macros, (50, 50));
    assert_eq!(xs, vec![0, 10, 11, 12, 50]);
    assert_eq!(ys, vec![0, 10, 11, 12, 50]);
}

// ---------------------------------------------------------------- the vicinity

/// ⚠️ **A side facing the edge of the grid is never tested, so it stays TRUE** — the outline's own
/// boundary counts as a wall.
#[test]
fn the_outline_boundary_counts_as_enclosing() {
    let grid = vec![vec![false; 3]; 3];
    let v = check_notch_vicinity(&grid, 0, 0, 0, 0);
    assert_eq!(v, NotchVicinity { top: false, bottom: true, left: true, right: false });
    assert_eq!(v.total(), 2, "bottom and left are walls only because they are the grid edge");
}

/// ⚠️ **A side is cleared by the FIRST empty neighbour** — it asks whether the region is walled
/// in, not how much of the wall exists.
#[test]
fn one_gap_in_a_wall_clears_that_side() {
    // A 1 x 2 empty region at row 1, columns 0..1, with the row above occupied except one cell.
    let mut grid = vec![vec![false; 3]; 3];
    grid[2][0] = true;
    let full_wall = check_notch_vicinity(&grid, 1, 0, 1, 0);
    assert!(full_wall.top, "the single cell above is occupied");

    let partial = check_notch_vicinity(&grid, 1, 0, 1, 1);
    assert!(!partial.top, "widening the region exposed the gap above column 1");
}

// ---------------------------------------------------------------- the penalty

/// ⚠️ **The root is what makes it length-like**: two notches of half the area cost more together
/// than one notch of the whole.
#[test]
fn the_single_notch_penalty_is_a_rooted_area_fraction() {
    assert_eq!(single_notch_penalty(100, 100, 10_000), 1.0, "the whole outline");
    assert_eq!(single_notch_penalty(50, 50, 10_000), 0.5, "a quarter of the area, half the cost");
    let halves = 2.0 * single_notch_penalty(50, 100, 10_000);
    assert!(halves > 1.0, "two half-notches cost more than one whole one: {halves}");
}

/// ⚠️ **An invalid floorplan is one huge notch**, at least the whole outline, and the scan is
/// skipped entirely.
#[test]
fn an_invalid_floorplan_is_charged_as_one_huge_notch() {
    let macros = [hard(0, 0, 1000, 1000)];
    // Fits, but invalid for some other reason: the outline's own dimensions win the `max`.
    let got = notch_penalty(&macros, (1000, 1000), (500, 500), false, 50.0);
    assert_eq!(got, 1.0);

    // Overflowing the outline: the packing's dimensions win instead.
    let big = notch_penalty(&macros, (1000, 1000), (2000, 2000), false, 50.0);
    assert_eq!(big, 2.0, "sqrt(4_000_000 / 1_000_000)");
}

/// 🔑 **A shallow full-width gap between two macros is a notch.** Enclosed above and below by the
/// macros, and left and right by the outline's own boundary.
#[test]
fn a_shallow_gap_between_two_macros_is_a_notch() {
    // Threshold is 1000/10 = 100 on both axes. The gap is 1000 wide and 50 tall.
    let macros = [hard(0, 0, 1000, 400), hard(0, 450, 1000, 550)];
    let got = notch_penalty(&macros, (1000, 1000), (1000, 1000), true, 50.0);
    assert_eq!(got, single_notch_penalty(1000, 50, 1_000_000));
    assert!(got > 0.0);
}

/// ⚠️ **A gap at or above the threshold is not a notch.** The test is a strict `<`.
#[test]
fn a_gap_at_the_threshold_is_not_a_notch() {
    // 100 tall, and the threshold is exactly 100.
    let macros = [hard(0, 0, 1000, 400), hard(0, 500, 1000, 500)];
    assert_eq!(notch_penalty(&macros, (1000, 1000), (1000, 1000), true, 50.0), 0.0);

    // One unit shallower and it counts.
    let macros = [hard(0, 0, 1000, 400), hard(0, 499, 1000, 501)];
    assert!(notch_penalty(&macros, (1000, 1000), (1000, 1000), true, 50.0) > 0.0);
}

/// ⛔ **The thresholds are CROSSED relative to their names**: the height of a candidate is
/// compared against a tenth of the outline's HEIGHT, and its width against a tenth of the WIDTH.
/// A lopsided outline is what makes the two readings distinguishable.
#[test]
fn each_axis_is_measured_against_its_own_extent() {
    // A 2000 x 400 outline: the height threshold is 40, the width threshold is 200.
    // A full-width gap 60 tall is NOT a notch under the real rule (60 >= 40), but would be under
    // the crossed reading (60 < 200).
    let macros = [hard(0, 0, 2000, 170), hard(0, 230, 2000, 170)];
    assert_eq!(notch_penalty(&macros, (2000, 400), (2000, 400), true, 50.0), 0.0);

    // And 30 tall IS a notch (30 < 40).
    let macros = [hard(0, 0, 2000, 185), hard(0, 215, 2000, 185)];
    assert!(notch_penalty(&macros, (2000, 400), (2000, 400), true, 50.0) > 0.0);
}

/// 🔑 **A narrow full-height gap is a notch on the other axis**, judged against the width.
#[test]
fn a_narrow_full_height_gap_is_a_notch() {
    let macros = [hard(0, 0, 400, 1000), hard(450, 0, 550, 1000)];
    let got = notch_penalty(&macros, (1000, 1000), (1000, 1000), true, 50.0);
    assert_eq!(got, single_notch_penalty(50, 1000, 1_000_000));
}

/// ⚠️ **A wide open region is not a notch even though it is fully enclosed by the outline.** Being
/// walled in is necessary, not sufficient.
#[test]
fn an_empty_outline_is_not_a_notch() {
    assert_eq!(notch_penalty(&[], (1000, 1000), (1000, 1000), true, 50.0), 0.0);
}

/// ⛔ **A fixed macro obstructs nothing, so the space beside it is scanned as if it were empty.**
/// A gap that a hard-macro cluster would have made into a notch is invisible when the same
/// geometry is fixed.
#[test]
fn a_fixed_macro_does_not_create_a_notch() {
    let as_cluster = [hard(0, 0, 1000, 400), hard(0, 450, 1000, 550)];
    assert!(notch_penalty(&as_cluster, (1000, 1000), (1000, 1000), true, 50.0) > 0.0);

    let as_fixed = [
        nm(0, 0, 1000, 400, AreaKind::FixedMacro),
        nm(0, 450, 1000, 550, AreaKind::FixedMacro),
    ];
    assert_eq!(notch_penalty(&as_fixed, (1000, 1000), (1000, 1000), true, 50.0), 0.0);
}

/// ⚠️ **Notches accumulate**, one term per region found.
#[test]
fn two_separate_notches_are_both_charged() {
    // Two shallow full-width gaps, at y 400..450 and y 800..850.
    let macros = [
        hard(0, 0, 1000, 400),
        hard(0, 450, 1000, 350),
        hard(0, 850, 1000, 150),
    ];
    let got = notch_penalty(&macros, (1000, 1000), (1000, 1000), true, 50.0);
    assert_eq!(got, 2.0 * single_notch_penalty(1000, 50, 1_000_000));
}

/// ⚠️ **Each empty region is visited once.** The scan marks the whole expanded rectangle, so the
/// cells inside it are not re-seeded as further notches.
///
/// ⛔ The fixture has to be an ENCLOSED gap subdivided on BOTH axes. A gap subdivided on one axis
/// only cannot show the difference: every re-seeded sub-region has an empty neighbour on the side
/// it was grown from, which clears that axis of its vicinity, and the other axis then spans the
/// full outline and is far too large to be a notch. A first version of this test was subdivided on
/// one axis and a mutation that deleted the marking went straight through it.
#[test]
fn an_expanded_region_is_not_counted_twice() {
    // A 50 x 50 hole walled in on all four sides, cut into a 2 x 2 of grid cells by a zero-size
    // macro at its centre. Marked visited, that is ONE notch; unmarked, the scan re-seeds the
    // right column and the top row and charges three.
    let macros = [
        hard(0, 0, 1000, 400),
        hard(0, 450, 1000, 550),
        hard(0, 400, 400, 50),
        hard(450, 400, 550, 50),
        hard(425, 425, 0, 0),
    ];
    let got = notch_penalty(&macros, (1000, 1000), (1000, 1000), true, 50.0);
    assert_eq!(got, single_notch_penalty(50, 50, 1_000_000), "one notch, not three");
}

/// ℹ️ A zero weight leaves the term dark — which is why coarse shaping never sees it.
#[test]
fn the_notch_term_is_dark_without_weight() {
    let macros = [hard(0, 0, 1000, 400), hard(0, 450, 1000, 550)];
    assert_eq!(notch_penalty(&macros, (1000, 1000), (1000, 1000), true, 0.0), 0.0);
    // Even an invalid floorplan, which otherwise short-circuits to a full-outline notch.
    assert_eq!(notch_penalty(&macros, (1000, 1000), (2000, 2000), false, 0.0), 0.0);
}
