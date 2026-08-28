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

use crate::cluster::{Cluster, ClusterId};

/// A block port, as the clustering stage sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub name: String,
    pub bbox: Rect,
    /// ⚠️ Upstream tests the FIRST pin's placement status, not the terminal's.
    pub is_fixed: bool,
    /// The region the user restricted this pin to, if any.
    pub constraint: Option<Rect>,
}

/// What `createIOClusters` produced.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IoClusters {
    /// The bundles that survived — ⚠️ empty ones are **released**.
    pub bundles: Vec<Cluster>,
    /// Clusters created for unplaced pins, in creation order.
    pub pin_clusters: Vec<Cluster>,
    /// `(pin index, cluster id)`.
    pub assignment: Vec<(usize, ClusterId)>,
    /// ⚠️ False when the design has no ports at all — upstream warns (`MPL-26`) and records it.
    pub has_io_clusters: bool,
    pub next_id: ClusterId,
}

/// Upstream `createIOClusters`, for a design **without** IO pads.
///
/// 🔑 **Bundles are created only when at least one pin is FIXED.** A design whose pins are all
/// still floating gets constraint-sharing clusters instead — there is nothing to bundle *around*
/// yet.
///
/// **Unfixed pins group by constraint:**
/// - a pin with **no** constraint joins the single *unconstrained* cluster — the first such pin
///   creates it and every later one shares it;
/// - a pin **with** a constraint joins an existing cluster carrying the identical region, or
///   starts a new one.
///
/// ⚠️ **Empty bundles are released.** A bundle nothing landed in does not survive, so the surviving
/// count is a fact about the design rather than always twenty.
pub fn create_io_clusters(pins: &[Pin], die: &Rect, first_id: ClusterId) -> IoClusters {
    let mut out = IoClusters { has_io_clusters: true, next_id: first_id, ..Default::default() };

    if pins.is_empty() {
        // Upstream warns MPL-26 and carries on.
        out.has_io_clusters = false;
        return out;
    }

    let spans = bundle_spans(die);
    let any_fixed = pins.iter().any(|p| p.is_fixed);
    let first_bundle_id = out.next_id;

    if any_fixed {
        for edge in BUNDLE_EDGE_ORDER {
            for i in 0..IO_BUNDLES_PER_EDGE {
                let mut c = Cluster::new(out.next_id, bundle_name(edge, i));
                let rect = bundle_rect(edge, i, die, spans);
                // ⛔ `setAsIOBundle` sets the flag AND builds the soft macro. Setting the flag
                // alone leaves the placer a `0x0` box at the origin: an IO cluster then sits on
                // top of everything, and every wirelength measured to it is measured to the wrong
                // place.
                c.set_as_io_bundle(
                    (rect.x_min as i32, rect.y_min as i32),
                    (rect.x_max - rect.x_min) as i32,
                    (rect.y_max - rect.y_min) as i32,
                );
                // ⚠️ Kept alongside: `io_region` is the pin-access builders' input and is not the
                // same field as the soft macro, though they hold the same rectangle here.
                c.io_region = Some(rect);
                out.next_id += 1;
                out.bundles.push(c);
            }
        }
    }

    // The single cluster every unconstrained pin shares, once one exists.
    let mut unconstrained: Option<ClusterId> = None;

    for (idx, pin) in pins.iter().enumerate() {
        if any_fixed && pin.is_fixed {
            let Some(offset) = bundle_offset(&pin.bbox, die, spans) else { continue };
            let id = first_bundle_id + offset;
            if let Some(b) = out.bundles.iter_mut().find(|b| b.id == id) {
                b.num_io_pins += 1;
            }
            out.assignment.push((idx, id));
            continue;
        }

        // Unfixed: share by constraint.
        let existing = match &pin.constraint {
            None => unconstrained,
            Some(region) => out
                .pin_clusters
                .iter()
                .find(|c| c.constraint_region.as_ref() == Some(region))
                .map(|c| c.id),
        };

        if let Some(id) = existing {
            // ⚠️ A pin cluster counts its pins too. Only bundles did at first, so every pin
            // cluster printed `Pins: 0` -- caught by comparing against upstream, not by any test
            // here, because nothing in this crate knew what the number should be.
            if let Some(c) = out.pin_clusters.iter_mut().find(|c| c.id == id) {
                c.num_io_pins += 1;
            }
            out.assignment.push((idx, id));
            continue;
        }

        // 🔑 Named `ios_{id}` — the id, not a running count, so the name and the id agree.
        let mut c = Cluster::new(out.next_id, format!("ios_{}", out.next_id));
        // ⛔ **The soft macro is the CONSTRAINT REGION, or the whole DIE when there is none.**
        // Upstream passes `constraint_shape`, which it sets to the die area for an unconstrained
        // cluster — so an unconstrained IO cluster is a full-die RECTANGLE while a constrained one
        // is a LINE on an edge. ⚠️ The raw rect, not the line form: `rectToLine` is used for the
        // separate constraint map, not for this.
        let shape = match &pin.constraint {
            Some(region) => *region,
            None => *die,
        };
        c.set_as_cluster_of_unplaced_io_pins(
            (shape.x_min as i32, shape.y_min as i32),
            (shape.x_max - shape.x_min) as i32,
            (shape.y_max - shape.y_min) as i32,
            pin.constraint.is_none(),
        );
        match &pin.constraint {
            Some(region) => {
                c.constraint_region = Some(*region);
                // ⚠️ The same rectangle, kept under both names on purpose — see `io_region`.
                c.io_region = Some(*region);
            }
            None => {
                unconstrained = Some(c.id);
            }
        }
        c.num_io_pins = 1;
        out.assignment.push((idx, c.id));
        out.next_id += 1;
        out.pin_clusters.push(c);
    }

    // ⚠️ A bundle nothing landed in does not survive.
    out.bundles.retain(|b| b.num_io_pins > 0);
    out
}

/// Upstream `createIOPadClusters`: one cluster per IO pad, named after the pad instance.
///
/// 🔑 **This REPLACES the bundle/unplaced-pin path entirely.** `createIOClusters` returns
/// immediately after calling it, so a design with IO pads has no `ios_*` clusters and no IO
/// bundles at all — the pads are the design's connection to the outside.
pub fn create_io_pad_clusters(
    pads: &[usize],
    design: &crate::design::Design,
    first_id: ClusterId,
) -> IoClusters {
    let mut pin_clusters = Vec::new();
    let mut next_id = first_id;
    for &p in pads {
        let mut c = Cluster::new(next_id, design.instances[p].name.clone());
        // ⛔ The pad's own instance bbox, UNHALOED — `pad->getBBox()->getBox()`, not the
        // `HardMacro` box a fixed macro uses. `setAsIOPadCluster` builds the soft macro as well as
        // setting the flag; with only the flag the pad is a `0x0` box at the origin.
        let b = design.instances[p].bbox;
        c.set_as_io_pad_cluster(
            (b.x_min as i32, b.y_min as i32),
            (b.x_max - b.x_min) as i32,
            (b.y_max - b.y_min) as i32,
        );
        pin_clusters.push(c);
        next_id += 1;
    }
    IoClusters {
        bundles: Vec::new(),
        pin_clusters,
        // ⚠️ The assignment is by INSTANCE here, not by block port — pads are instances, and the
        // block ports are never consulted again on this path.
        assignment: Vec::new(),
        has_io_clusters: true,
        next_id,
    }
}
