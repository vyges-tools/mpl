// SPDX-License-Identifier: Apache-2.0
//! Halo resolution. Every case pins a rule read from `ClusteringEngine::buildMacroHalo`.
//!
//! ⚠️ Deliberately asymmetric everywhere. A halo bug that swaps two sides is invisible on a
//! symmetric halo, and symmetric halos are the common case -- so a test using one proves nothing.
use vyges_mpl::halo::{
    build_macro_halo, closest_boundary, full_halo, pin_aware_halo, Boundary, LayerDir, Orient,
    PinBox,
};
use vyges_mpl::options::Halo;

fn h(left: i64, bottom: i64, right: i64, top: i64) -> Halo {
    Halo { left, bottom, right, top }
}

fn pin(x_min: i64, y_min: i64, x_max: i64, y_max: i64, layer_dir: LayerDir) -> PinBox {
    PinBox { x_min, y_min, x_max, y_max, layer_dir }
}

// ------------------------------------------------------------------ full_halo

#[test]
fn an_explicit_halo_wins_outright() {
    // set_macro_halo beats both the instance halo and the base halo.
    assert_eq!(
        full_halo(Some(h(1, 2, 3, 4)), Some((h(9, 9, 9, 9), false)), h(8, 8, 8, 8)),
        h(1, 2, 3, 4)
    );
}

#[test]
fn a_hard_instance_halo_is_raised_to_the_base_componentwise() {
    // max() per side, NOT "whichever rect is bigger" -- the result can take two sides from each.
    assert_eq!(
        full_halo(None, Some((h(10, 1, 10, 1), false)), h(2, 20, 2, 20)),
        h(10, 20, 10, 20)
    );
}

#[test]
fn a_soft_instance_halo_is_taken_as_is_and_is_not_floored() {
    // 🔑 The asymmetry that matters: a soft halo is NOT raised to the base halo. Other tools
    // already respect it, so upstream declines to enlarge it. Flooring it here would silently
    // grow every macro that carries a soft halo.
    assert_eq!(
        full_halo(None, Some((h(1, 1, 1, 1), true)), h(50, 50, 50, 50)),
        h(1, 1, 1, 1)
    );
}

#[test]
fn no_instance_halo_means_the_base_halo() {
    assert_eq!(full_halo(None, None, h(1, 2, 3, 4)), h(1, 2, 3, 4));
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
    // 🔑 The whole point of pin-aware mode. One pin on the left: left widens, the other three
    // sides stay at the minimum spacing.
    let pins = [pin(1, 40, 5, 60, LayerDir::Horizontal)];
    assert_eq!(
        pin_aware_halo(&pins, 100, 100, h(70, 80, 90, 100), 7),
        h(70, 7, 7, 7)
    );
}

#[test]
fn each_pin_widens_only_its_own_side() {
    let pins = [
        pin(1, 40, 5, 60, LayerDir::Horizontal),   // L
        pin(95, 40, 99, 60, LayerDir::Horizontal), // R
    ];
    assert_eq!(
        pin_aware_halo(&pins, 100, 100, h(70, 80, 90, 100), 7),
        h(70, 7, 90, 7),
        "left and right widen; bottom and top do not"
    );
}

#[test]
fn no_pins_at_all_leaves_the_minimum_spacing_on_every_side() {
    // A macro with only power pins reaches here with an empty list.
    assert_eq!(pin_aware_halo(&[], 100, 100, h(70, 80, 90, 100), 7), h(7, 7, 7, 7));
}

#[test]
fn the_widened_side_takes_that_sides_full_halo_value_not_another() {
    // If the sides were mixed up, a symmetric full halo would hide it. This one is all different.
    let pins = [pin(40, 1, 60, 5, LayerDir::Vertical)]; // closest to B
    let got = pin_aware_halo(&pins, 100, 100, h(11, 22, 33, 44), 1);
    assert_eq!(got.bottom, 22, "bottom takes the FULL halo's bottom");
    assert_eq!((got.left, got.right, got.top), (1, 1, 1));
}

// ------------------------------------------------------------------ reorientation

#[test]
fn a_fixed_macro_has_its_halo_reoriented() {
    // Upstream does this ONLY for fixed macros: unfixed ones are adjusted later by the
    // orientation-improve pass, which fixed macros skip.
    let pins = [pin(40, 1, 60, 5, LayerDir::Vertical)]; // bottom
    let flipped = build_macro_halo(
        None, None, h(11, 22, 33, 44), false, &pins, 100, 100, 1, true, Orient::Mx,
    );
    assert_eq!(flipped.top, 22, "MX swaps bottom and top");
    assert_eq!(flipped.bottom, 1);
}

#[test]
fn r180_swaps_both_axes_and_my_swaps_only_left_right() {
    let pins = [pin(1, 40, 5, 60, LayerDir::Horizontal)]; // left
    let my = build_macro_halo(
        None, None, h(11, 22, 33, 44), false, &pins, 100, 100, 1, true, Orient::My,
    );
    assert_eq!(my.right, 11, "MY swaps left and right");

    let pins = [pin(40, 1, 60, 5, LayerDir::Vertical)]; // bottom
    let r180 = build_macro_halo(
        None, None, h(11, 22, 33, 44), false, &pins, 100, 100, 1, true, Orient::R180,
    );
    assert_eq!(r180.top, 22, "R180 swaps both axes, so bottom lands on top");
}

#[test]
fn an_unfixed_macro_is_not_reoriented() {
    // Same inputs as the MX case above but unfixed: no swap.
    let pins = [pin(40, 1, 60, 5, LayerDir::Vertical)];
    let got = build_macro_halo(
        None, None, h(11, 22, 33, 44), false, &pins, 100, 100, 1, false, Orient::Mx,
    );
    assert_eq!(got.bottom, 22, "unfixed keeps the halo where the pins put it");
    assert_eq!(got.top, 1);
}

// ------------------------------------------------------------------ the whole rule

#[test]
fn use_full_halo_skips_the_pin_analysis_entirely() {
    let pins = [pin(1, 40, 5, 60, LayerDir::Horizontal)];
    assert_eq!(
        build_macro_halo(
            None, None, h(11, 22, 33, 44), true, &pins, 100, 100, 1, false, Orient::R0
        ),
        h(11, 22, 33, 44),
        "every side gets the full halo, pins ignored"
    );
}

#[test]
fn an_explicit_halo_bypasses_use_full_halo_and_reorientation() {
    // ⚠️ Upstream returns the explicit halo before either is consulted. A fixed, MX-oriented
    // macro with an explicit halo keeps it exactly as written.
    let pins = [pin(1, 40, 5, 60, LayerDir::Horizontal)];
    assert_eq!(
        build_macro_halo(
            Some(h(1, 2, 3, 4)), Some((h(9, 9, 9, 9), false)), h(8, 8, 8, 8),
            false, &pins, 100, 100, 5, true, Orient::Mx,
        ),
        h(1, 2, 3, 4)
    );
}
