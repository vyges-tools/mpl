// SPDX-License-Identifier: Apache-2.0
//! Halo resolution. Every case pins a rule read from `ClusteringEngine::buildMacroHalo` and
//! `buildPinAwareHalo`, at the pin that took the minimum-channel rework (`VYGMPL halo` trace).
//!
//! ⚠️ Deliberately asymmetric everywhere. A halo bug that swaps two sides is invisible on a
//! symmetric halo, and symmetric halos are the common case -- so a test using one proves nothing.
use vyges_mpl::halo::{
    build_macro_halo, closest_boundary, floored_to_channel, floored_to_halo, pin_aware_halo,
    Boundary, LayerDir, Orient, PinBox,
};
use vyges_mpl::options::{Channel, Halo};

fn h(left: i64, bottom: i64, right: i64, top: i64) -> Halo {
    Halo { left, bottom, right, top }
}

fn ch(width: i64, height: i64) -> Channel {
    Channel { width, height }
}

fn pin(x_min: i64, y_min: i64, x_max: i64, y_max: i64, layer_dir: LayerDir) -> PinBox {
    PinBox { x_min, y_min, x_max, y_max, layer_dir }
}

// ------------------------------------------------------------------ the two floors

#[test]
fn floored_to_halo_raises_each_side_independently() {
    // max() per side, NOT "whichever halo is bigger" -- the result takes two sides from each.
    assert_eq!(floored_to_halo(h(10, 1, 10, 1), h(2, 20, 2, 20)), h(10, 20, 10, 20));
}

#[test]
fn floored_to_channel_takes_half_the_width_sideways_and_half_the_height_vertically() {
    // 🔑 A channel is shared by the macros either side of it, so each takes HALF. Width feeds
    // left/right, height bottom/top -- swapping them is invisible on a square channel.
    assert_eq!(floored_to_channel(h(1, 1, 1, 1), ch(48, 24)), h(24, 12, 24, 12));
    // A side already wider than half the channel keeps its own value.
    assert_eq!(floored_to_channel(h(30, 1, 1, 1), ch(48, 24)), h(30, 12, 24, 12));
}

#[test]
fn an_odd_channel_truncates_as_integer_division_does() {
    // Upstream's `width / 2` on `int`: 7 / 2 == 3, never 4.
    assert_eq!(floored_to_channel(h(0, 0, 0, 0), ch(7, 5)), h(3, 2, 3, 2));
}

// ------------------------------------------------------------------ closest_boundary

#[test]
fn the_nearest_edge_wins_when_there_is_no_tie() {
    // Master 100x100. Pin hugging the left edge.
    let p = pin(1, 40, 5, 60, LayerDir::Horizontal);
    assert_eq!(closest_boundary(p, 100, 100), Boundary::L);
    // ...and hugging the top: height - y_max = 2.
    let p = pin(40, 90, 60, 98, LayerDir::Horizontal);
    assert_eq!(closest_boundary(p, 100, 100), Boundary::T);
}

#[test]
fn right_and_bottom_distances_use_the_master_extents() {
    // R is width - x_max and T is height - y_max; using x_min/y_min there would pick the
    // opposite edge for every pin on the far side.
    let p = pin(90, 40, 98, 60, LayerDir::Horizontal);
    assert_eq!(closest_boundary(p, 100, 100), Boundary::R);
    let p = pin(40, 2, 60, 5, LayerDir::Horizontal);
    assert_eq!(closest_boundary(p, 100, 100), Boundary::B);
}

#[test]
fn a_corner_pin_is_decided_by_its_layer_direction() {
    // Equidistant from L (x_min = 5) and B (y_min = 5): a corner pin.
    // A VERTICALLY routed pin escapes through the horizontal edge -> B.
    let p = pin(5, 5, 10, 10, LayerDir::Vertical);
    assert_eq!(closest_boundary(p, 100, 100), Boundary::B);
    // A HORIZONTALLY routed pin escapes through the vertical edge -> L.
    let p = pin(5, 5, 10, 10, LayerDir::Horizontal);
    assert_eq!(closest_boundary(p, 100, 100), Boundary::L);
}

#[test]
fn the_corner_rule_holds_in_the_far_corner_too() {
    // Equidistant from R and T. Same rule, mirrored -- a rule written for one corner and
    // silently applied to another is exactly how ppl lost three milestones.
    let p = pin(90, 90, 95, 95, LayerDir::Vertical);
    assert_eq!(closest_boundary(p, 100, 100), Boundary::T);
    let p = pin(90, 90, 95, 95, LayerDir::Horizontal);
    assert_eq!(closest_boundary(p, 100, 100), Boundary::R);
}

#[test]
fn equidistant_same_direction_falls_back_to_the_enum_order() {
    // ⚠️ Getting this case to arise at all takes care, and my first attempt did not: a pin
    // "centred horizontally" is equidistant from L and R only if those are also the two NEAREST
    // edges, which needs a WIDE pin, not a centred one. With a narrow centred pin, B or T is
    // nearer and the tie never happens.
    //
    // Master 100x100, pin spanning x 10..90 -> L = R = 10, while B = T = 40.
    // Both are vertical edges, so the layer-direction rule does NOT apply and the sort decides.
    let p = pin(10, 40, 90, 60, LayerDir::Vertical);
    assert_eq!(
        closest_boundary(p, 100, 100),
        Boundary::L,
        "B=0 L=1 T=2 R=3: L precedes R"
    );
}

#[test]
fn a_centred_pin_prefers_bottom_over_top() {
    // The mirror: a TALL pin, so B and T are the nearest pair. Both horizontal, B=0 precedes T=2.
    let p = pin(40, 10, 60, 90, LayerDir::Horizontal);
    assert_eq!(closest_boundary(p, 100, 100), Boundary::B);
}

// ------------------------------------------------------------------ pin_aware_halo

#[test]
fn a_side_with_no_pin_keeps_the_minimum_spacing() {
    // 🔑 The whole point of pin-aware mode. One pin on the left: left takes half the channel, the
    // other three sides stay at the minimum spacing.
    let pins = [pin(1, 40, 5, 60, LayerDir::Horizontal)];
    assert_eq!(pin_aware_halo(&pins, 100, 100, ch(140, 160), 7), h(70, 7, 7, 7));
}

#[test]
fn each_pin_widens_only_its_own_side() {
    // The reference on `halos3` (`-min_channel_size {24 12} -pin_aware_channels`, 2000 DBU/um,
    // minimum spacing 140): pins I1 on the left edge and O1 on the right give
    // `out 24000 140 24000 140` -- left/right at half of 48000, bottom/top at the spacing.
    let pins = [
        pin(0, 150000, 280, 150140, LayerDir::Horizontal),
        pin(199720, 150000, 200000, 150140, LayerDir::Horizontal),
    ];
    assert_eq!(
        pin_aware_halo(&pins, 200000, 300000, ch(48000, 24000), 140),
        h(24000, 140, 24000, 140)
    );
}

#[test]
fn no_pins_at_all_leaves_the_minimum_spacing_on_every_side() {
    // A macro with only power pins reaches here with an empty list.
    assert_eq!(pin_aware_halo(&[], 100, 100, ch(140, 160), 7), h(7, 7, 7, 7));
}

#[test]
fn a_widened_side_is_never_narrower_than_the_minimum_spacing() {
    // The widened value is the channel floor OF the spacing: a channel smaller than twice the
    // spacing leaves the side at the spacing, not at half the channel.
    let pins = [pin(40, 1, 60, 5, LayerDir::Vertical)]; // closest to B
    assert_eq!(pin_aware_halo(&pins, 100, 100, ch(4, 4), 9), h(9, 9, 9, 9));
}

// ------------------------------------------------------------------ reorientation

#[test]
fn a_fixed_macro_has_its_halo_reoriented() {
    // Upstream does this ONLY for fixed macros: unfixed ones are adjusted later by the
    // orientation-improve pass, which fixed macros skip.
    let pins = [pin(40, 1, 60, 5, LayerDir::Vertical)]; // bottom
    let flipped =
        build_macro_halo(None, ch(0, 44), true, &pins, 100, 100, 1, true, Orient::Mx);
    assert_eq!(flipped.top, 22, "MX swaps bottom and top");
    assert_eq!(flipped.bottom, 1);
}

#[test]
fn r180_swaps_both_axes_and_my_swaps_only_left_right() {
    let pins = [pin(1, 40, 5, 60, LayerDir::Horizontal)]; // left
    let my = build_macro_halo(None, ch(22, 0), true, &pins, 100, 100, 1, true, Orient::My);
    assert_eq!((my.left, my.right), (1, 11), "MY swaps left and right");

    let pins = [pin(40, 1, 60, 5, LayerDir::Vertical)]; // bottom
    let r180 = build_macro_halo(None, ch(0, 44), true, &pins, 100, 100, 1, true, Orient::R180);
    assert_eq!((r180.bottom, r180.top), (1, 22), "R180 swaps both axes, so bottom lands on top");
}

#[test]
fn an_unfixed_macro_is_not_reoriented() {
    // Same inputs as the MX case above but unfixed: no swap.
    let pins = [pin(40, 1, 60, 5, LayerDir::Vertical)];
    let got = build_macro_halo(None, ch(0, 44), true, &pins, 100, 100, 1, false, Orient::Mx);
    assert_eq!((got.bottom, got.top), (22, 1), "unfixed keeps the halo where the pins put it");
}

#[test]
fn a_def_halo_on_a_fixed_macro_is_reoriented_too() {
    // ⚠️ Reorientation is on EVERY path now, the DEF-halo one included.
    let got = build_macro_halo(
        Some(h(1, 2, 3, 4)), ch(0, 0), false, &[], 100, 100, 0, true, Orient::R180,
    );
    assert_eq!(got, h(3, 4, 1, 2));
}

// ------------------------------------------------------------------ the whole rule

#[test]
fn by_default_every_side_gets_the_spacing_floored_to_half_the_channel() {
    // The reference on `boundary_push1` (no channel, spacing 140): `out 140 140 140 140` for every
    // macro, pins ignored. Before the rework the default was pin-aware.
    let pins = [pin(1, 40, 5, 60, LayerDir::Horizontal)];
    assert_eq!(
        build_macro_halo(None, ch(0, 0), false, &pins, 100, 100, 140, false, Orient::R0),
        h(140, 140, 140, 140)
    );
    assert_eq!(
        build_macro_halo(None, ch(400, 600), false, &pins, 100, 100, 140, false, Orient::R0),
        h(200, 300, 200, 300)
    );
}

#[test]
fn a_def_halo_floors_the_default_per_side_and_its_soft_flag_is_not_read() {
    // The reference on `halos1` (hard DEF halo 5000 10000 15000 20000): taken as is above the
    // spacing. On `halos2` (SOFT 16000 8000 4000 2000) the same: buildMacroHalo never asks.
    assert_eq!(
        build_macro_halo(
            Some(h(5000, 10000, 15000, 20000)), ch(0, 0), false, &[], 100, 100, 140, false,
            Orient::R0,
        ),
        h(5000, 10000, 15000, 20000)
    );
    // A side below the floor is raised to it.
    assert_eq!(
        build_macro_halo(Some(h(1, 500, 1, 1)), ch(0, 0), false, &[], 100, 100, 140, false, Orient::R0),
        h(140, 500, 140, 140)
    );
}

#[test]
fn a_def_halo_turns_pin_awareness_off_for_that_macro() {
    // `if (inst->getHalo()) ... else if (pin_aware_channels_)` -- the DEF halo wins, so a pin on
    // the left does NOT leave the other sides at the spacing.
    let pins = [pin(1, 40, 5, 60, LayerDir::Horizontal)];
    assert_eq!(
        build_macro_halo(Some(h(0, 0, 0, 0)), ch(40, 40), true, &pins, 100, 100, 7, false, Orient::R0),
        h(20, 20, 20, 20)
    );
}
