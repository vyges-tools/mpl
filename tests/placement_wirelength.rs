// SPDX-License-Identifier: Apache-2.0
//! The placement stage's netlist and wirelength model.

use vyges_mpl::halo::Boundary;
use vyges_mpl::placement::{
    build_bundled_nets, compute_nets_wire_length, dist_to_nearest_region, is_outside_the_outline,
    nearest_point_in_region, pin_center, BundledNet, Region, WirelengthMacro,
    VIRTUAL_CONNECTION_WEIGHT,
};

fn macro_at(x: i32, y: i32, width: i32, height: i32) -> WirelengthMacro {
    WirelengthMacro {
        x,
        y,
        width,
        height,
        is_cluster_of_unplaced_io_pins: false,
        is_unconstrained_io_cluster: false,
    }
}

/// ⚠️ **`>`, strictly** — so a connection is emitted once per undirected pair, and a
/// self-connection (equal ids) is dropped entirely.
#[test]
fn only_the_higher_id_of_each_pair_emits_a_net() {
    let children = vec![
        (1usize, vec![(2usize, 3.0f32)]),
        (2usize, vec![(1usize, 3.0f32), (2usize, 9.0f32)]),
    ];
    let nets = build_bundled_nets(&[], &children, &|id| id);
    assert_eq!(nets.len(), 1, "one net for the pair, none for the self-connection");
    assert_eq!(nets[0], BundledNet { source: 2, target: 1, weight: 3.0 });
}

/// 🔑 **Virtual connections come FIRST and carry a fixed weight**, ahead of every child's own.
#[test]
fn virtual_connections_lead_and_carry_their_own_weight() {
    let children = vec![(5usize, vec![(1usize, 2.0f32)])];
    let nets = build_bundled_nets(&[(3, 4)], &children, &|id| id);
    assert_eq!(nets.len(), 2);
    assert_eq!(nets[0], BundledNet { source: 3, target: 4, weight: VIRTUAL_CONNECTION_WEIGHT });
    assert_eq!(nets[0].weight, 10.0);
    assert_eq!(nets[1].source, 5, "the child's net follows");
}

/// ⚠️ The centre truncates, so an odd extent rounds down.
#[test]
fn the_pin_centre_truncates() {
    assert_eq!(pin_center(0, 100), 50);
    assert_eq!(pin_center(0, 101), 50, "not 51");
    assert_eq!(pin_center(10, 5), 12, "10 + 2.5 truncates to 12");
}

/// ⚠️ **A vertical edge clamps in Y; a horizontal one clamps in X.** Using the wrong axis returns
/// a point on the line but at the wrong end.
#[test]
fn the_nearest_point_clamps_along_the_edges_own_axis() {
    let left = Region { x0: 0, y0: 100, x1: 0, y1: 300, boundary: Boundary::L };
    assert_eq!(nearest_point_in_region(&left, (50, 200)), (0, 200), "projected onto the edge");
    assert_eq!(nearest_point_in_region(&left, (50, 400)), (0, 300), "clamped to the top end");
    assert_eq!(nearest_point_in_region(&left, (50, 10)), (0, 100), "clamped to the bottom end");

    let bottom = Region { x0: 100, y0: 0, x1: 300, y1: 0, boundary: Boundary::B };
    assert_eq!(nearest_point_in_region(&bottom, (200, 50)), (200, 0));
    assert_eq!(nearest_point_in_region(&bottom, (400, 50)), (300, 0));
}

/// ⚠️ A target level with an endpoint snaps to it rather than projecting.
#[test]
fn a_target_level_with_an_endpoint_snaps_to_it() {
    let left = Region { x0: 0, y0: 100, x1: 0, y1: 300, boundary: Boundary::L };
    assert_eq!(nearest_point_in_region(&left, (50, 300)), (0, 300));
    assert_eq!(nearest_point_in_region(&left, (50, 100)), (0, 100));
}

/// ⚠️ **Minimised on squared distance, rooted once, then truncated.**
#[test]
fn the_distance_is_rooted_once_and_truncated() {
    let near = Region { x0: 0, y0: 0, x1: 0, y1: 100, boundary: Boundary::L };
    let far = Region { x0: 1000, y0: 0, x1: 1000, y1: 100, boundary: Boundary::R };
    // From (3, 50): 3 to the left edge, 997 to the right one.
    assert_eq!(dist_to_nearest_region((3, 50), &[near, far]), Some(3));
    assert_eq!(dist_to_nearest_region((3, 50), &[far]), Some(997), "the far one alone");
    // A diagonal: from (3, 150) to the left edge's top end (0, 100) is sqrt(9 + 2500) = 50.08 -> 50.
    assert_eq!(dist_to_nearest_region((3, 150), &[near]), Some(50));
    assert_eq!(dist_to_nearest_region((0, 0), &[]), None, "no regions at all");
}

/// ⚠️ **The PIN is compared against the outline's DIMENSIONS**, and it is `>` — so a pin exactly
/// on the boundary is inside.
#[test]
fn a_pin_on_the_boundary_counts_as_inside() {
    let outline = (1000, 800);
    // A macro whose centre lands exactly on each limit.
    // Centre lands exactly on (1000, 800): on the limit, so inside.
    assert!(!is_outside_the_outline(&macro_at(900, 700, 200, 200), outline));
    assert!(is_outside_the_outline(&macro_at(901, 700, 200, 200), outline), "one past is outside");
}

/// 🔑 **Normalised by the total weight AND the outline's semi-perimeter**, so the result is a
/// fraction rather than a distance.
#[test]
fn the_wirelength_is_normalised_twice() {
    let macros = vec![macro_at(0, 0, 100, 100), macro_at(900, 0, 100, 100)];
    let nets = vec![BundledNet { source: 0, target: 1, weight: 2.0 }];
    let outline = (1000, 1000);
    let got = compute_nets_wire_length(&nets, &nets, &macros, outline, 4000, &[], &|_| None);
    // Centres are (50, 50) and (950, 50): half-perimeter 900, weight 2, sum 2, semi-perimeter 2000.
    assert_eq!(got, (2.0 * 900.0) / 2.0 / 2000.0);
}

/// ⚠️ A zero total weight returns zero rather than dividing by it.
#[test]
fn a_zero_weight_sum_gives_zero_rather_than_a_division() {
    let macros = vec![macro_at(0, 0, 100, 100), macro_at(900, 0, 100, 100)];
    let nets = vec![BundledNet { source: 0, target: 1, weight: 0.0 }];
    let got = compute_nets_wire_length(&nets, &nets, &macros, (1000, 1000), 4000, &[], &|_| None);
    assert_eq!(got, 0.0);
    assert!(got.is_finite());
}

/// ⛔ **The weight sum comes from a SEPARATE list.** Upstream sums over its member while measuring
/// the argument — one character apart — and its only caller passes the same list, so the two agree
/// today. This pins that the two are genuinely distinct inputs.
#[test]
fn the_weight_sum_is_taken_from_its_own_list() {
    let macros = vec![macro_at(0, 0, 100, 100), macro_at(900, 0, 100, 100)];
    let measured = vec![BundledNet { source: 0, target: 1, weight: 2.0 }];
    let summed = vec![BundledNet { source: 0, target: 1, weight: 8.0 }];
    let got = compute_nets_wire_length(&measured, &summed, &macros, (1000, 1000), 4000, &[], &|_| None);
    assert_eq!(got, (2.0 * 900.0) / 8.0 / 2000.0, "measured with one, divided by the other");
}

/// 🔑 **A macro outside the outline is charged the whole die**, which dominates any refinement.
#[test]
fn a_macro_outside_the_outline_is_charged_the_whole_die() {
    let mut io = macro_at(0, 0, 0, 0);
    io.is_cluster_of_unplaced_io_pins = true;
    io.is_unconstrained_io_cluster = true;
    let macros = vec![macro_at(5000, 0, 100, 100), io];
    let nets = vec![BundledNet { source: 0, target: 1, weight: 3.0 }];
    let regions = [Region { x0: 0, y0: 0, x1: 0, y1: 1000, boundary: Boundary::L }];
    let got =
        compute_nets_wire_length(&nets, &nets, &macros, (1000, 1000), 4000, &regions, &|_| None);
    // 3 * 4000 = 12000, then normalised by weight 3 and semi-perimeter 2000.
    assert_eq!(got, 12000.0 / 3.0 / 2000.0);
}

/// ⚠️ **Only the TARGET is tested.** With the IO cluster as the SOURCE the ordinary half-perimeter
/// path is taken instead, which is a different number.
#[test]
fn only_the_target_takes_the_io_path() {
    let mut io = macro_at(0, 500, 0, 0);
    io.is_cluster_of_unplaced_io_pins = true;
    io.is_unconstrained_io_cluster = true;
    let placed = macro_at(400, 400, 100, 100);
    let outline = (1000, 1000);
    let regions = [Region { x0: 0, y0: 0, x1: 0, y1: 1000, boundary: Boundary::L }];

    let as_target = compute_nets_wire_length(
        &[BundledNet { source: 0, target: 1, weight: 1.0 }],
        &[BundledNet { source: 0, target: 1, weight: 1.0 }],
        &[placed, io],
        outline, 4000, &regions, &|_| None,
    );
    let as_source = compute_nets_wire_length(
        &[BundledNet { source: 0, target: 1, weight: 1.0 }],
        &[BundledNet { source: 0, target: 1, weight: 1.0 }],
        &[io, placed],
        outline, 4000, &regions, &|_| None,
    );
    assert_ne!(as_target, as_source, "the model is asymmetric in the pair");
}
