// SPDX-License-Identifier: Apache-2.0
//! Snapping a placed macro onto the track grid.

use vyges_mpl::halo::Orient;
use vyges_mpl::placement::{
    align_with_manufacturing_grid, pin_offset, snap_origin_to_position, starting_position_index,
    SnapAxis, SNAP_PASSES,
};

/// ⚠️ **A VERTICAL pass constrains X.** Vertical routing layers have vertical tracks, whose
/// positions are X coordinates — reading it as "moves the macro vertically" gets both passes
/// backwards.
#[test]
fn the_snap_passes_are_vertical_then_horizontal() {
    assert_eq!(SNAP_PASSES, [SnapAxis::Vertical, SnapAxis::Horizontal]);
}

// ---------------------------------------------------------------- the pin offset

/// ⚠️ The offset is the master terminal's near edge plus half the placed pin's width.
#[test]
fn the_offset_is_the_terminal_edge_plus_half_the_pin() {
    assert_eq!(pin_offset(40, 100, Orient::R0, SnapAxis::Vertical), 120);
}

/// ⚠️ **The half-width is an INTEGER division**, so an odd pin width loses its half unit.
#[test]
fn an_odd_pin_width_loses_its_half_unit() {
    assert_eq!(pin_offset(41, 100, Orient::R0, SnapAxis::Vertical), 120, "not 120.5");
    assert_eq!(pin_offset(43, 100, Orient::R0, SnapAxis::Vertical), 121);
}

/// ⛔ **Only THREE orientations negate, and they differ per axis**: `MY` and `R180` vertically,
/// `MX` and `R180` horizontally.
#[test]
fn each_axis_is_negated_by_its_own_two_orientations() {
    let v = |o| pin_offset(40, 100, o, SnapAxis::Vertical);
    let h = |o| pin_offset(40, 100, o, SnapAxis::Horizontal);

    assert_eq!(v(Orient::My), -120, "MY flips X");
    assert_eq!(v(Orient::Mx), 120, "MX does not");
    assert_eq!(h(Orient::Mx), -120, "MX flips Y");
    assert_eq!(h(Orient::My), 120, "MY does not");

    assert_eq!(v(Orient::R180), -120, "R180 flips both");
    assert_eq!(h(Orient::R180), -120);

    assert_eq!(v(Orient::R0), 120, "R0 flips neither");
    assert_eq!(h(Orient::R0), 120);
}

/// ⛔ **The four ROTATED orientations are not handled at all** and take the unnegated offset — the
/// reference's `switch` lists only `MX`, `MY` and `R180`.
#[test]
fn a_rotated_orientation_is_not_negated() {
    assert_eq!(pin_offset(40, 100, Orient::Other, SnapAxis::Vertical), 120);
    assert_eq!(pin_offset(40, 100, Orient::Other, SnapAxis::Horizontal), 120);
}

// ---------------------------------------------------------------- the manufacturing grid

/// ⚠️ **`std::round` rounds half AWAY FROM ZERO** — not to even, and not toward zero.
#[test]
fn the_grid_rounds_half_away_from_zero() {
    assert_eq!(align_with_manufacturing_grid(15, 10), 20, "1.5 rounds up");
    assert_eq!(align_with_manufacturing_grid(25, 10), 30, "2.5 rounds up too, not to even");
    assert_eq!(align_with_manufacturing_grid(-15, 10), -20, "and away from zero going down");
    assert_eq!(align_with_manufacturing_grid(14, 10), 10);
    assert_eq!(align_with_manufacturing_grid(16, 10), 20);
}

/// ⚠️ An origin already on the grid is untouched.
#[test]
fn an_aligned_origin_is_untouched() {
    assert_eq!(align_with_manufacturing_grid(200, 10), 200);
    assert_eq!(align_with_manufacturing_grid(0, 10), 0);
}

/// ⛔ **The grid alignment happens AFTER the track is chosen, and can undo it.** The origin is
/// placed so the pin lands exactly on a track, then rounded to the manufacturing grid — which
/// moves it off that track whenever the pitch is not a multiple of the grid.
#[test]
fn the_manufacturing_grid_can_move_the_pin_off_its_track() {
    // A track at 205, a pin offset of 0, and a grid of 10.
    let origin = snap_origin_to_position(205, 0, 10);
    assert_eq!(origin, 210, "rounded off the track it was just snapped to");

    // With a pitch that IS a multiple of the grid, the snap survives.
    assert_eq!(snap_origin_to_position(200, 0, 10), 200);
}

/// ⛔ **The offset is subtracted BEFORE the rounding, and the order is observable.** Rounding first
/// and subtracting after gives the same answer whenever the offset is itself a multiple of the
/// grid — which every tidy fixture makes it. A mutation proved that: the offsets here are now
/// deliberately NOT multiples of the grid.
#[test]
fn the_offset_is_removed_before_rounding() {
    // 123 is not a multiple of 10. Correct: round(300 - 123) = round(177) = 180.
    // Rounding first would give round(300) - 123 = 177.
    assert_eq!(snap_origin_to_position(300, 123, 10), 180);

    assert_eq!(snap_origin_to_position(300, -123, 10), 420, "round(423) = 420");

    // And the tidy case, which cannot tell the two apart — kept to show why it could not.
    assert_eq!(snap_origin_to_position(300, 120, 10), 180);
}

// ---------------------------------------------------------------- choosing the track

/// ⛔ **The first track AT OR AFTER the pin centre, not the NEAREST one.** A pin just past a track
/// is moved FORWARD to the next, never back — the snap is biased along the axis.
#[test]
fn the_snap_takes_the_next_track_not_the_nearest() {
    let tracks = [0, 100, 200, 300];
    // 101 is one unit past the track at 100 and 99 units from the one at 200 — it still goes to 200.
    assert_eq!(starting_position_index(&tracks, 101), Some(2));
    assert_eq!(starting_position_index(&tracks, 199), Some(2));
}

/// ⚠️ A pin exactly on a track stays on it — `lower_bound` is inclusive.
#[test]
fn a_pin_already_on_a_track_stays() {
    let tracks = [0, 100, 200, 300];
    assert_eq!(starting_position_index(&tracks, 100), Some(1));
    assert_eq!(starting_position_index(&tracks, 0), Some(0));
}

/// ⚠️ **A pin past the LAST track steps back to it** — the only case that ever moves backwards.
#[test]
fn a_pin_past_the_last_track_steps_back() {
    let tracks = [0, 100, 200, 300];
    assert_eq!(starting_position_index(&tracks, 301), Some(3));
    assert_eq!(starting_position_index(&tracks, 999_999), Some(3));
}

/// ⚠️ A pin before the first track goes forward to it.
#[test]
fn a_pin_before_the_first_track_goes_forward() {
    let tracks = [100, 200];
    assert_eq!(starting_position_index(&tracks, -50), Some(0));
}

/// ℹ️ No tracks at all is `None` — the caller aligns to the manufacturing grid alone.
#[test]
fn no_tracks_is_none() {
    assert_eq!(starting_position_index(&[], 100), None);
}

// ---------------------------------------------------------------- the extra-pattern search

use vyges_mpl::placement::{aligned_pins_on_layer, search_extra_patterns, spiral_step, SnapSearch};

/// 🔑 **An alternating outward spiral, POSITIVE first** — so a track just after the pin is
/// preferred to the equally-distant one just before it.
#[test]
fn the_search_spirals_outward_positive_first() {
    let steps: Vec<i32> = (0..=8).map(spiral_step).collect();
    assert_eq!(steps, vec![0, 1, -1, 2, -2, 3, -3, 4, -4]);
}

/// ⚠️ **101 attempts, not 100** — the loop is inclusive, so it reaches ±50.
#[test]
fn the_search_reaches_fifty_either_way() {
    assert_eq!(spiral_step(99), 50);
    assert_eq!(spiral_step(100), -50);
}

/// ⛔ **The inclusive bound is what reaches `-50`**, and only a candidate exactly fifty tracks
/// BELOW the start can show it — `+50` arrives at step 99 either way. Testing `spiral_step` alone
/// cannot catch a change to the loop bound; a mutation proved that.
#[test]
fn the_last_attempt_reaches_fifty_tracks_below_the_start() {
    // 101 positions, starting at 50. Only index 0 aligns, and index 0 is start - 50.
    let mut aligned_for = |i: usize| usize::from(i == 0);
    let got = search_extra_patterns(50, 101, 4, &mut aligned_for);
    assert_eq!(got.best_index, 0, "reached only on the 101st attempt");
    assert_eq!(got.best_aligned, 1);
}

// ---------------------------------------------------------------- counting aligned pins

/// 🔑 A two-pointer merge: a pin counts only when its centre is exactly on a track.
#[test]
fn a_pin_counts_only_when_it_is_exactly_on_a_track() {
    assert_eq!(aligned_pins_on_layer(&[100, 200, 300], &[0, 100, 200, 300]), 3);
    assert_eq!(aligned_pins_on_layer(&[101, 200, 300], &[0, 100, 200, 300]), 2);
    assert_eq!(aligned_pins_on_layer(&[], &[0, 100]), 0);
    assert_eq!(aligned_pins_on_layer(&[100], &[]), 0);
}

/// ⚠️ A pin that falls short of the current track can never align with a later one, so it is
/// dropped rather than retried.
#[test]
fn a_pin_short_of_the_current_track_is_dropped() {
    // 50 falls between tracks and is skipped; 100 and 200 align.
    assert_eq!(aligned_pins_on_layer(&[50, 100, 200], &[100, 200]), 2);
}

/// ⛔ **A pin past the LAST track is never examined** — the merge ends when the track pointer runs
/// out. So unaligned pins at the high end are silently skipped, including for the
/// `RightWayOnGridOnly` error, which is only ever raised from the falls-short branch.
#[test]
fn pins_past_the_last_track_are_never_examined() {
    // 300 and 400 are past the last track; neither is counted and neither would be reported.
    assert_eq!(aligned_pins_on_layer(&[100, 300, 400], &[0, 100, 200]), 1);
}

/// ⛔ **A match does NOT advance the track pointer, so SEVERAL pins can share one track.** Two
/// pins stacked on the same vertical line have the same x centre, which is an ordinary arrangement
/// — and both count. Advancing the track pointer on a match would silently drop every pin after the
/// first at each position; a mutation proved no other fixture here could see it.
#[test]
fn several_pins_can_share_one_track() {
    assert_eq!(aligned_pins_on_layer(&[100, 100, 100], &[0, 100, 200]), 3);
    assert_eq!(aligned_pins_on_layer(&[100, 100, 200], &[100, 200]), 3);
}

/// ⛔ **It depends on both lists being SORTED**, and nothing re-checks that — unsorted pins
/// silently undercount rather than failing.
#[test]
fn unsorted_pins_silently_undercount() {
    let sorted = aligned_pins_on_layer(&[100, 200, 300], &[100, 200, 300]);
    let shuffled = aligned_pins_on_layer(&[300, 100, 200], &[100, 200, 300]);
    assert_eq!(sorted, 3);
    assert_eq!(shuffled, 1, "silently wrong, not an error");
}

// ---------------------------------------------------------------- choosing the best index

/// 🔑 The candidate aligning the most pins wins.
#[test]
fn the_candidate_aligning_the_most_pins_wins() {
    // index 5 aligns everything; everything else aligns one.
    let mut aligned_for = |i: usize| if i == 5 { 4 } else { 1 };
    let got = search_extra_patterns(3, 10, 4, &mut aligned_for);
    assert_eq!(got, SnapSearch { best_index: 5, best_aligned: 4, all_aligned: true });
}

/// ⚠️ **The search STOPS as soon as every pin is aligned** — it does not keep looking for a tie.
#[test]
fn the_search_stops_once_everything_aligns() {
    let mut seen = Vec::new();
    let mut aligned_for = |i: usize| {
        seen.push(i);
        if i == 4 { 4 } else { 0 }
    };
    search_extra_patterns(3, 10, 4, &mut aligned_for);
    // 3, then 4 -- and it stops there rather than trying 2, 5, 1, ...
    assert_eq!(seen, vec![3, 4]);
}

/// ⛔ **`>`, strictly, from a starting best of ZERO.** The starting index is not privileged — it is
/// evaluated like any other candidate — but if NOTHING aligns a single pin, the search falls back
/// to it rather than to whatever it happened to try last.
#[test]
fn nothing_aligning_falls_back_to_the_starting_index() {
    let mut aligned_for = |_: usize| 0;
    let got = search_extra_patterns(3, 10, 4, &mut aligned_for);
    assert_eq!(got.best_index, 3, "the start, not the last candidate tried");
    assert_eq!(got.best_aligned, 0);
    assert!(!got.all_aligned);
}

/// ⚠️ **A tie goes to the EARLIER candidate in spiral order**, because the comparison is strict.
#[test]
fn a_tie_goes_to_the_earlier_candidate() {
    // Both 4 and 2 align two pins; 4 is reached first (spiral is +1 before -1).
    let mut aligned_for = |i: usize| if i == 4 || i == 2 { 2 } else { 0 };
    let got = search_extra_patterns(3, 10, 4, &mut aligned_for);
    assert_eq!(got.best_index, 4);
}

/// ⚠️ **An out-of-range candidate is SKIPPED, not a stopping condition** — the spiral keeps
/// stepping past one end and continues to explore the other.
#[test]
fn the_spiral_steps_past_one_end_and_keeps_going() {
    // Start at 0 with only three positions: -1, -2 ... are all invalid, but +1 and +2 are tried.
    let mut seen = Vec::new();
    let mut aligned_for = |i: usize| {
        seen.push(i);
        0
    };
    search_extra_patterns(0, 3, 4, &mut aligned_for);
    assert_eq!(seen, vec![0, 1, 2], "the negative half was skipped, not fatal");
}

/// ℹ️ No positions at all means nothing is ever evaluated.
#[test]
fn no_positions_evaluates_nothing() {
    let mut calls = 0;
    let mut aligned_for = |_: usize| {
        calls += 1;
        0
    };
    let got = search_extra_patterns(0, 0, 4, &mut aligned_for);
    assert_eq!(calls, 0);
    assert_eq!(got.best_index, 0);
}
