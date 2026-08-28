// SPDX-License-Identifier: Apache-2.0
//! IO bundles. Rules from upstream `createIOBundle` / `findAssociatedBundledIOId`.
//!
//! ⚠️ The die here is 1000 x 1000, so each span is 200 — chosen so a bundle boundary lands on a
//! round number and an off-by-one in the ring direction is unmistakable rather than plausible.
use vyges_mpl::design::Rect;
use vyges_mpl::halo::Boundary;
use vyges_mpl::ioclusters::{
    all_bundle_names, bundle_name, bundle_offset, bundle_rect, bundle_spans, BundleSpans,
    IO_BUNDLES_PER_EDGE,
};

fn die() -> Rect {
    Rect { x_min: 0, y_min: 0, x_max: 1000, y_max: 1000 }
}

fn spans() -> BundleSpans {
    bundle_spans(&die())
}

/// A small pin box centred on (x, y).
fn pin(x: i64, y: i64) -> Rect {
    Rect { x_min: x - 1, y_min: y - 1, x_max: x + 1, y_max: y + 1 }
}

/// A pin touching the given edge at the given position along it.
fn edge_pin(edge: Boundary, along: i64) -> Rect {
    match edge {
        Boundary::L => Rect { x_min: 0, y_min: along - 1, x_max: 2, y_max: along + 1 },
        Boundary::R => Rect { x_min: 998, y_min: along - 1, x_max: 1000, y_max: along + 1 },
        Boundary::T => Rect { x_min: along - 1, y_min: 998, x_max: along + 1, y_max: 1000 },
        Boundary::B => Rect { x_min: along - 1, y_min: 0, x_max: along + 1, y_max: 2 },
    }
}

// ------------------------------------------------------------------ spans and names

#[test]
fn the_die_is_divided_into_five_per_edge() {
    assert_eq!(spans(), BundleSpans { x: 200, y: 200 });
    assert_eq!(IO_BUNDLES_PER_EDGE, 5);
}

#[test]
fn bundles_are_named_and_ordered_L_T_R_B() {
    // ⚠️ The creation order IS the id order, so this list is the id mapping.
    let names = all_bundle_names();
    assert_eq!(names.len(), 20);
    assert_eq!(names[0], "L_0");
    assert_eq!(names[4], "L_4");
    assert_eq!(names[5], "T_0");
    assert_eq!(names[10], "R_0");
    assert_eq!(names[15], "B_0");
    assert_eq!(bundle_name(Boundary::R, 3), "R_3");
}

// ------------------------------------------------------------------ the ring

#[test]
fn the_left_edge_indexes_FORWARD_from_the_bottom() {
    assert_eq!(bundle_offset(&edge_pin(Boundary::L, 100), &die(), spans()), Some(0));
    assert_eq!(bundle_offset(&edge_pin(Boundary::L, 900), &die(), spans()), Some(4));
}

#[test]
fn the_top_edge_indexes_FORWARD_from_the_left() {
    assert_eq!(bundle_offset(&edge_pin(Boundary::T, 100), &die(), spans()), Some(5));
    assert_eq!(bundle_offset(&edge_pin(Boundary::T, 900), &die(), spans()), Some(9));
}

#[test]
fn the_right_edge_indexes_BACKWARD_from_the_top() {
    // 🔴 The reversal. A pin near the TOP of the right edge is R_0, not R_4. Indexing this edge
    // forward would mirror the whole half, and every pin would still land in a plausible bundle.
    assert_eq!(bundle_offset(&edge_pin(Boundary::R, 900), &die(), spans()), Some(10), "top -> R_0");
    assert_eq!(bundle_offset(&edge_pin(Boundary::R, 100), &die(), spans()), Some(14), "bottom -> R_4");
}

#[test]
fn the_bottom_edge_indexes_BACKWARD_from_the_right() {
    // 🔴 The same reversal, completing the ring.
    assert_eq!(bundle_offset(&edge_pin(Boundary::B, 900), &die(), spans()), Some(15), "right -> B_0");
    assert_eq!(bundle_offset(&edge_pin(Boundary::B, 100), &die(), spans()), Some(19), "left -> B_4");
}

#[test]
fn the_ring_is_continuous_around_the_die() {
    // 🔑 Walking anticlockwise from the bottom-left, the offsets increase monotonically. That is
    // what "a ring, not four edges" means, and it only holds if the reversal is right.
    let walk = [
        bundle_offset(&edge_pin(Boundary::L, 100), &die(), spans()).unwrap(), // up the left
        bundle_offset(&edge_pin(Boundary::L, 900), &die(), spans()).unwrap(),
        bundle_offset(&edge_pin(Boundary::T, 100), &die(), spans()).unwrap(), // right along the top
        bundle_offset(&edge_pin(Boundary::T, 900), &die(), spans()).unwrap(),
        bundle_offset(&edge_pin(Boundary::R, 900), &die(), spans()).unwrap(), // down the right
        bundle_offset(&edge_pin(Boundary::R, 100), &die(), spans()).unwrap(),
        bundle_offset(&edge_pin(Boundary::B, 900), &die(), spans()).unwrap(), // left along the bottom
        bundle_offset(&edge_pin(Boundary::B, 100), &die(), spans()).unwrap(),
    ];
    assert!(walk.windows(2).all(|w| w[0] < w[1]), "monotonic around the ring: {walk:?}");
}

// ------------------------------------------------------------------ corners and misses

#[test]
fn a_corner_pin_takes_the_FIRST_matching_edge() {
    // ⚠️ An if/else chain in the order L, T, R, B. A bottom-left pin satisfies both LEFT and
    // BOTTOM, and is a LEFT pin.
    let bottom_left = Rect { x_min: 0, y_min: 0, x_max: 2, y_max: 2 };
    assert_eq!(bundle_offset(&bottom_left, &die(), spans()), Some(0), "L_0, not B_4");

    // Top-right satisfies TOP and RIGHT, and is a TOP pin.
    let top_right = Rect { x_min: 998, y_min: 998, x_max: 1000, y_max: 1000 };
    assert_eq!(bundle_offset(&top_right, &die(), spans()), Some(9), "T_4, not R_0");
}

#[test]
fn a_pin_touching_no_edge_belongs_to_no_bundle() {
    assert_eq!(bundle_offset(&pin(500, 500), &die(), spans()), None);
}

#[test]
fn a_degenerate_span_yields_the_first_bundle_rather_than_a_wild_index() {
    // ⚠️ Removing the zero guard does NOT panic -- float division by zero is infinity in Rust,
    // and the cast then saturates. So `is_some()` cannot see it; the VALUE is what changes, from
    // a sane 0 to i32::MAX. A wild index would point at a bundle that does not exist.
    let flat = Rect { x_min: 0, y_min: 0, x_max: 1000, y_max: 0 };
    let s = bundle_spans(&flat);
    assert_eq!(s.y, 0, "a die with no height has no y span");
    // ⚠️ The pin must sit OFF the die's y origin. A pin centred at y == 0 divides 0 by 0, which
    // is NaN, and NaN casts to 0 -- the same answer the guard gives, so it proves nothing. At
    // y == 5 the unguarded division is infinity, which saturates to i32::MAX.
    assert_eq!(
        bundle_offset(&edge_pin(Boundary::L, 5), &flat, s),
        Some(0),
        "the first bundle, not a saturated index"
    );
}

// ------------------------------------------------------------------ geometry

#[test]
fn bundle_rectangles_advance_on_the_left_and_retreat_on_the_right() {
    // 🔑 The geometry mirrors the id order, so rectangles and pin assignment agree by
    // construction. If one reversed and the other did not, pins would land in bundles drawn
    // somewhere else entirely.
    let l0 = bundle_rect(Boundary::L, 0, &die(), spans());
    assert_eq!((l0.y_min, l0.y_max), (0, 200), "L_0 is the BOTTOM of the left edge");

    let r0 = bundle_rect(Boundary::R, 0, &die(), spans());
    assert_eq!((r0.y_min, r0.y_max), (800, 1000), "R_0 is the TOP of the right edge");

    let b0 = bundle_rect(Boundary::B, 0, &die(), spans());
    assert_eq!((b0.x_min, b0.x_max), (800, 1000), "B_0 is the RIGHT of the bottom edge");
}

#[test]
fn a_bundle_rectangle_sits_ON_its_edge_with_no_thickness() {
    // ⚠️ They mark where pins are; they do not enclose area. A bundle given thickness would
    // overlap the core and be treated as an obstruction.
    let l = bundle_rect(Boundary::L, 2, &die(), spans());
    assert_eq!((l.x_min, l.x_max), (0, 0), "vertical edge, zero width");
    let t = bundle_rect(Boundary::T, 2, &die(), spans());
    assert_eq!((t.y_min, t.y_max), (1000, 1000), "horizontal edge, zero height");
}

#[test]
fn every_bundle_rectangle_lies_within_the_die() {
    for edge in [Boundary::L, Boundary::T, Boundary::R, Boundary::B] {
        for i in 0..IO_BUNDLES_PER_EDGE {
            let r = bundle_rect(edge, i, &die(), spans());
            assert!(r.x_min >= 0 && r.x_max <= 1000, "{edge:?} {i} x out of die: {r:?}");
            assert!(r.y_min >= 0 && r.y_max <= 1000, "{edge:?} {i} y out of die: {r:?}");
        }
    }
}

#[test]
fn the_bundles_on_an_edge_tile_it_without_gaps_or_overlap() {
    // ⚠️ A SQUARE die makes spans.x == spans.y, so a vertical edge stepping by the WRONG span is
    // invisible. This die is 1000 x 500, giving spans 200 and 100 -- and a left edge stepping by
    // 200 would run to 1000 on a 500-tall die.
    let tall = Rect { x_min: 0, y_min: 0, x_max: 1000, y_max: 500 };
    let s = bundle_spans(&tall);
    assert_eq!((s.x, s.y), (200, 100), "the two spans differ, which is the point");

    let mut prev_end = 0;
    for i in 0..IO_BUNDLES_PER_EDGE {
        let r = bundle_rect(Boundary::L, i, &tall, s);
        assert_eq!(r.y_min, prev_end, "L_{i} starts where the previous one ended");
        prev_end = r.y_max;
    }
    assert_eq!(prev_end, 500, "the five bundles cover the whole edge, not more");
}

// ------------------------------------------------------------------ createIOClusters

use vyges_mpl::ioclusters::{create_io_clusters, Pin};

fn fixed_at(edge: Boundary, along: i64) -> Pin {
    Pin { name: format!("p{along}"), bbox: edge_pin(edge, along), is_fixed: true, constraint: None }
}

fn floating(name: &str, constraint: Option<Rect>) -> Pin {
    Pin { name: name.into(), bbox: pin(500, 500), is_fixed: false, constraint }
}

#[test]
fn a_design_with_no_ports_has_no_io_clusters() {
    // Upstream warns MPL-26 and records it rather than failing.
    let r = create_io_clusters(&[], &die(), 100);
    assert!(!r.has_io_clusters);
    assert!(r.bundles.is_empty() && r.pin_clusters.is_empty());
}

#[test]
fn bundles_are_created_only_when_a_pin_is_FIXED() {
    // 🔑 A design whose pins are all still floating has nothing to bundle AROUND.
    let floating_only = vec![floating("a", None), floating("b", None)];
    let r = create_io_clusters(&floating_only, &die(), 100);
    assert!(r.bundles.is_empty(), "no fixed pin, no bundles");
    assert_eq!(r.pin_clusters.len(), 1, "they share one unconstrained cluster");
    // ⚠️ "No bundles survive" is NOT enough on its own: creating twenty and then releasing them
    // all leaves the same empty list. The IDS are what reveal it -- twenty bundles would consume
    // ids 100..119 and push the pin cluster to 120.
    assert_eq!(r.pin_clusters[0].id, 100, "no bundle consumed an id");
    assert_eq!(r.next_id, 101);
}

#[test]
fn a_fixed_pin_lands_in_the_bundle_its_position_selects() {
    let pins = vec![fixed_at(Boundary::L, 100), fixed_at(Boundary::R, 900)];
    let r = create_io_clusters(&pins, &die(), 100);
    // L_0 is offset 0, R_0 is offset 10 -- the ring reversal, end to end.
    assert_eq!(r.assignment[0].1, 100, "L_0");
    assert_eq!(r.assignment[1].1, 110, "R_0, the TOP of the right edge");
}

#[test]
fn empty_bundles_are_RELEASED() {
    // ⚠️ A bundle nothing landed in does not survive, so the surviving count is a fact about the
    // design rather than always twenty.
    let pins = vec![fixed_at(Boundary::L, 100)];
    let r = create_io_clusters(&pins, &die(), 100);
    assert_eq!(r.bundles.len(), 1, "nineteen were released");
    assert_eq!(r.bundles[0].name, "L_0");
    assert_eq!(r.bundles[0].num_io_pins, 1);
}

#[test]
fn several_pins_in_one_bundle_are_counted() {
    let pins = vec![fixed_at(Boundary::L, 50), fixed_at(Boundary::L, 150)];
    let r = create_io_clusters(&pins, &die(), 100);
    assert_eq!(r.bundles.len(), 1, "both fell in L_0");
    assert_eq!(r.bundles[0].num_io_pins, 2);
}

#[test]
fn every_unconstrained_pin_shares_ONE_cluster() {
    // 🔑 The first such pin creates it; every later one joins it.
    let pins = vec![floating("a", None), floating("b", None), floating("c", None)];
    let r = create_io_clusters(&pins, &die(), 100);
    assert_eq!(r.pin_clusters.len(), 1);
    assert!(r.pin_clusters[0].is_cluster_of_unconstrained_io_pins);
    let ids: Vec<i32> = r.assignment.iter().map(|(_, id)| *id).collect();
    assert_eq!(ids, vec![100, 100, 100], "all three in the same cluster");
}

#[test]
fn pins_with_the_SAME_constraint_share_a_cluster() {
    let region = Rect { x_min: 0, y_min: 0, x_max: 100, y_max: 100 };
    let pins = vec![floating("a", Some(region)), floating("b", Some(region))];
    let r = create_io_clusters(&pins, &die(), 100);
    assert_eq!(r.pin_clusters.len(), 1);
    assert_eq!(r.assignment[0].1, r.assignment[1].1);
}

#[test]
fn pins_with_DIFFERENT_constraints_get_separate_clusters() {
    // ⚠️ Matched by equality, not overlap: two nested regions are still two clusters.
    let a = Rect { x_min: 0, y_min: 0, x_max: 100, y_max: 100 };
    let b = Rect { x_min: 0, y_min: 0, x_max: 200, y_max: 200 };
    let pins = vec![floating("a", Some(a)), floating("b", Some(b))];
    let r = create_io_clusters(&pins, &die(), 100);
    assert_eq!(r.pin_clusters.len(), 2);
    assert_ne!(r.assignment[0].1, r.assignment[1].1);
}

#[test]
fn a_constrained_pin_does_not_join_the_unconstrained_cluster() {
    // 🔑 They are different kinds of cluster. Merging them would place a restricted pin as if it
    // were free.
    let region = Rect { x_min: 0, y_min: 0, x_max: 100, y_max: 100 };
    let pins = vec![floating("free", None), floating("bound", Some(region))];
    let r = create_io_clusters(&pins, &die(), 100);
    assert_eq!(r.pin_clusters.len(), 2);
    assert!(r.pin_clusters[0].is_cluster_of_unconstrained_io_pins);
    assert!(!r.pin_clusters[1].is_cluster_of_unconstrained_io_pins);
    assert_eq!(r.pin_clusters[1].constraint_region, Some(region));
}

#[test]
fn a_pin_cluster_is_named_after_its_own_id() {
    // ⚠️ `ios_{id}`, not a running count -- so the name and the id cannot drift apart.
    let pins = vec![floating("a", None)];
    let r = create_io_clusters(&pins, &die(), 42);
    assert_eq!(r.pin_clusters[0].name, "ios_42");
    assert_eq!(r.pin_clusters[0].id, 42);
}

#[test]
fn ids_run_bundles_first_then_pin_clusters() {
    // With a fixed pin present, the twenty bundles take ids first, so a pin cluster starts after.
    let pins = vec![fixed_at(Boundary::L, 100), floating("a", None)];
    let r = create_io_clusters(&pins, &die(), 100);
    assert_eq!(r.assignment[0].1, 100, "the fixed pin's bundle");
    assert_eq!(r.assignment[1].1, 120, "the pin cluster follows all twenty bundles");
    assert_eq!(r.next_id, 121);
}

#[test]
fn a_mixed_design_bundles_the_fixed_and_groups_the_rest() {
    let pins = vec![
        fixed_at(Boundary::L, 100),
        floating("a", None),
        fixed_at(Boundary::T, 900),
        floating("b", None),
    ];
    let r = create_io_clusters(&pins, &die(), 0);
    assert_eq!(r.bundles.len(), 2, "L_0 and T_4 survived");
    assert_eq!(r.pin_clusters.len(), 1, "both floating pins share one cluster");
    assert_eq!(r.assignment.len(), 4, "every pin was assigned");
}

/// ⛔ **An IO cluster carries a SOFT MACRO, not just a flag.** `setAsIOBundle`,
/// `setAsIOPadCluster` and `setAsClusterOfUnplacedIOPins` each set their flag AND build the soft
/// macro; setting the flag alone leaves the placer a `0x0` box at the origin, so every wirelength
/// measured to that cluster is measured to the wrong place.
///
/// 🔑 **The shape is the CONSTRAINT REGION, or the whole DIE when there is none.** Upstream passes
/// `constraint_shape`, which it sets to the die area for an unconstrained cluster — so an
/// unconstrained IO cluster is a full-die RECTANGLE while a constrained one is a LINE on an edge.
/// ⚠️ The raw rect, not the line form: `rectToLine` feeds a separate constraint map, not this.
#[test]
fn an_io_pin_cluster_takes_its_constraint_region_or_the_whole_die() {
    let die = Rect { x_min: -18000, y_min: -19600, x_max: 222000, y_max: 220400 };
    let edge = Rect { x_min: 222000, y_min: 400, x_max: 222000, y_max: 40400 };
    let _ = &edge;

    // Constrained: a LINE on the right edge.
    let constrained = create_io_clusters(&[floating("a", Some(edge))], &die, 10);
    let c = &constrained.pin_clusters[0];
    let sm = c.soft_macro.expect("a pin cluster carries one");
    assert_eq!((sm.x, sm.y), (222000, 400));
    assert_eq!((sm.width, sm.height), (0, 40000), "a line: one dimension is zero");
    assert!(!c.is_cluster_of_unconstrained_io_pins);

    // Unconstrained: the whole DIE, which is a rectangle and not a line.
    let free = create_io_clusters(&[floating("a", None)], &die, 10);
    let f = &free.pin_clusters[0];
    let fsm = f.soft_macro.expect("carries one too");
    assert_eq!((fsm.x, fsm.y), (-18000, -19600), "the die's own corner");
    assert_eq!((fsm.width, fsm.height), (240000, 240000), "the whole die, not an edge");
    assert!(f.is_cluster_of_unconstrained_io_pins);

    // ⚠️ The control: the two must DIFFER, or the assertions above would hold for either shape.
    assert_ne!((sm.width, sm.height), (fsm.width, fsm.height));
}

/// ⛔ **An IO PAD cluster's soft macro is the pad instance's OWN bbox** — `pad->getBBox()`, the
/// placed instance box, and NOT the haloed `HardMacro` box a fixed macro uses. The two coexist in
/// this engine and picking the wrong one is invisible in the hierarchy dump.
///
/// ⚠️ `setAsIOPadCluster` builds the soft macro as well as setting the flag; with the flag alone
/// the pad is a `0x0` box at the origin and every net reaching it measures to the wrong place.
#[test]
fn an_io_pad_cluster_takes_the_pads_own_bbox() {
    use vyges_mpl::design::{Design, Instance, MasterKind};
    let pad = Instance {
        name: "PAD_1".into(),
        is_block: false,
        is_fixed: true,
        bbox: Rect { x_min: 300000, y_min: 100000, x_max: 580000, y_max: 150000 },
        master: MasterKind { is_pad: true, ..Default::default() },
        is_ignorable_macro: false,
    };
    let design = Design {
        instances: vec![pad],
        modules: Vec::new(),
        top: 0,
        core_area: die(),
        die_area: die(),
    };

    let r = vyges_mpl::ioclusters::create_io_pad_clusters(&[0], &design, 5);
    let c = &r.pin_clusters[0];
    assert!(c.is_io_pad_cluster);
    let sm = c.soft_macro.expect("a pad cluster carries one");
    assert_eq!((sm.x, sm.y), (300000, 100000));
    assert_eq!((sm.width, sm.height), (280000, 50000), "the pad's own extent");
}
