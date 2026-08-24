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
