// SPDX-License-Identifier: Apache-2.0
//! IO clusters — where the design's pins sit in the physical hierarchy.
//!
//! Upstream: `createIOClusters`, `createIOBundles`, `createIOBundle`,
//! `findAssociatedBundledIOId`, `computeIOBundleSpans`, `designHasFixedIOPins`.
//!
//! 🔑 **Bundles are a RING, not four independent edges**, and the direction reverses halfway
//! round. That is the whole difficulty of this file: index all four edges forward and every pin
//! still lands in a plausible-looking bundle, so the mistake is invisible without comparing
//! against upstream.

use crate::design::Rect;
use crate::halo::Boundary;

/// Upstream's `io_bundles_per_edge`.
pub const IO_BUNDLES_PER_EDGE: i32 = 5;

/// The edges, in the order upstream creates their bundles. ⚠️ **L, T, R, B** — the ring order,
/// which is also the order the id offsets follow.
pub const BUNDLE_EDGE_ORDER: [Boundary; 4] = [Boundary::L, Boundary::T, Boundary::R, Boundary::B];

/// How far one bundle spans along its edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleSpans {
    pub x: i64,
    pub y: i64,
}

/// `computeIOBundleSpans`: the die divided into five per edge.
pub fn bundle_spans(die: &Rect) -> BundleSpans {
    BundleSpans {
        x: (die.x_max - die.x_min) / IO_BUNDLES_PER_EDGE as i64,
        y: (die.y_max - die.y_min) / IO_BUNDLES_PER_EDGE as i64,
    }
}

/// The bundle name upstream gives: `L_0`, `T_3`, and so on.
pub fn bundle_name(edge: Boundary, index: i32) -> String {
    let letter = match edge {
        Boundary::L => "L",
        Boundary::T => "T",
        Boundary::R => "R",
        Boundary::B => "B",
    };
    format!("{letter}_{index}")
}

/// Which bundle does a pin belong to, as an offset from the first bundle's id?
///
/// 🔴 **The ring reverses.** Left and top index **forward** from the die's minimum; right and
/// bottom index **backward** from its maximum:
///
/// | Edge | Measured from | Offset |
/// | --- | --- | --- |
/// | L | `y_center − die.y_min` | `+0` |
/// | T | `x_center − die.x_min` | `+5` |
/// | R | `die.y_max − y_center` | `+10` |
/// | B | `die.x_max − x_center` | `+15` |
///
/// ⚠️ **The edge test is an if/else chain, so a CORNER pin takes the FIRST match** — L before T
/// before R before B. A pin in the bottom-left corner is a LEFT pin, not a bottom one.
///
/// Returns `None` when the pin touches no edge, which upstream leaves at the base id.
pub fn bundle_offset(pin: &Rect, die: &Rect, spans: BundleSpans) -> Option<i32> {
    let x_center = (pin.x_min + pin.x_max) / 2;
    let y_center = (pin.y_min + pin.y_max) / 2;
    let per_edge = IO_BUNDLES_PER_EDGE;

    if pin.x_min <= die.x_min {
        Some(div_floor(y_center - die.y_min, spans.y))
    } else if pin.y_max >= die.y_max {
        Some(per_edge + div_floor(x_center - die.x_min, spans.x))
    } else if pin.x_max >= die.x_max {
        // ⚠️ Backward: measured from the TOP edge down.
        Some(per_edge * 2 + div_floor(die.y_max - y_center, spans.y))
    } else if pin.y_min <= die.y_min {
        // ⚠️ Backward: measured from the RIGHT edge left.
        Some(per_edge * 3 + div_floor(die.x_max - x_center, spans.x))
    } else {
        None
    }
}

/// `std::floor(a / b)` on integers, guarding a zero span.
fn div_floor(a: i64, b: i64) -> i32 {
    if b == 0 {
        return 0;
    }
    (a as f64 / b as f64).floor() as i32
}

/// The rectangle a bundle occupies.
///
/// 🔑 **The geometry mirrors the id order**, so bundle rectangles and pin assignment agree by
/// construction: L and T advance from the minimum, R and B retreat from the maximum. ⚠️ Left and
/// right bundles are **zero-width** lines on the die edge, top and bottom **zero-height** — they
/// mark where pins are, they do not enclose area.
pub fn bundle_rect(edge: Boundary, index: i32, die: &Rect, spans: BundleSpans) -> Rect {
    // L and R run along the vertical edges, so they step by the y span.
    let ext = if edge.is_vertical() { spans.y } else { spans.x };
    let i = index as i64;
    match edge {
        Boundary::L => Rect {
            x_min: die.x_min,
            y_min: die.y_min + ext * i,
            x_max: die.x_min,
            y_max: die.y_min + ext * i + ext,
        },
        Boundary::T => Rect {
            x_min: die.x_min + ext * i,
            y_min: die.y_max,
            x_max: die.x_min + ext * i + ext,
            y_max: die.y_max,
        },
        Boundary::R => Rect {
            x_min: die.x_max,
            y_min: die.y_max - ext * (i + 1),
            x_max: die.x_max,
            y_max: die.y_max - ext * (i + 1) + ext,
        },
        Boundary::B => Rect {
            x_min: die.x_max - ext * (i + 1),
            y_min: die.y_min,
            x_max: die.x_max - ext * (i + 1) + ext,
            y_max: die.y_min,
        },
    }
}

/// The twenty bundle names, in creation order — which is also id order.
pub fn all_bundle_names() -> Vec<String> {
    BUNDLE_EDGE_ORDER
        .iter()
        .flat_map(|&e| (0..IO_BUNDLES_PER_EDGE).map(move |i| bundle_name(e, i)))
        .collect()
}
