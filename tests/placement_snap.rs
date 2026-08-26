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
