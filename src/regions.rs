// SPDX-License-Identifier: Apache-2.0
//! Where unconstrained IO pins may go: the die edges, minus what is blocked.
//!
//! 🔑 **Every region here is a LINE** — a rectangle with zero width or zero height, lying on a die
//! edge. That is why the "is it empty" test below is an `||` and not an `&&`: a piece with zero
//! width is still a region, and only a single point is nothing.

use crate::design::Rect;
use crate::halo::Boundary;

/// The four boundaries in upstream's own order.
///
/// ⚠️ Upstream keeps these in a `std::map` keyed by `Boundary`, so they are visited in **enum
/// order — B, L, T, R** — and the regions come out in that order. It is not the order they were
/// declared in, and it is observable in the result.
pub const BOUNDARY_ORDER: [Boundary; 4] = [Boundary::B, Boundary::L, Boundary::T, Boundary::R];

/// Which edge a region lies on.
///
/// ⚠️ **Nothing checks that the region is on an edge at all.** A zero-width region that is not at
/// the left edge is reported as `R`, and a zero-height one that is not at the bottom as `T` — the
/// tests are `== die.x_min` and `== die.y_min`, with everything else falling through. Reproduced,
/// because a caller passing a stray rectangle gets upstream's answer, not a better one.
pub fn boundary_of(die: &Rect, region: &Rect) -> Boundary {
    if region.x_max - region.x_min == 0 {
        return if region.x_min == die.x_min { Boundary::L } else { Boundary::R };
    }
    if region.y_min == die.y_min {
        Boundary::B
    } else {
        Boundary::T
    }
}

/// The whole of one die edge, as a line.
pub fn boundary_rect(die: &Rect, boundary: Boundary) -> Rect {
    let mut r = *die;
    match boundary {
        Boundary::L => r.x_max = die.x_min,
        Boundary::R => r.x_min = die.x_max,
        Boundary::T => r.y_min = die.y_max,
        Boundary::B => r.y_max = die.y_min,
    }
    r
}

/// `base` with `overlay` cut out of it, leaving the piece before and the piece after.
///
/// ⚠️ **A piece survives when its width OR its height is non-zero** — an `||`, not an `&&`. These
/// regions are lines, so one dimension is always zero; an `&&` would discard every one of them.
/// Only a piece that collapses to a single point is dropped.
///
/// ℹ️ Returns `None` when the two lie on different boundaries — upstream calls that a critical
/// error (MPL-46), and it cannot arise from `available_regions`, which groups by boundary first.
pub fn subtract_overlap(die: &Rect, base: &Rect, overlay: &Rect) -> Option<Vec<Rect>> {
    let boundary = boundary_of(die, base);
    if boundary != boundary_of(die, overlay) {
        return None;
    }
    let (mut a, mut b) = (*base, *base);
    if boundary.is_vertical() {
        a.y_max = overlay.y_min;
        b.y_min = overlay.y_max;
    } else {
        a.x_max = overlay.x_min;
        b.x_min = overlay.x_max;
    }
    let mut out = Vec::new();
    for piece in [a, b] {
        if piece.x_max - piece.x_min != 0 || piece.y_max - piece.y_min != 0 {
            out.push(piece);
        }
    }
    Some(out)
}

/// Upstream `computeAvailableRegions`: each die edge, minus the blocked regions on it.
///
/// ⚠️ **A blocked region is subtracted only where it is fully CONTAINED** in an available piece.
/// One that merely overlaps a piece is left alone, and the space it covers stays available. That
/// reads like a bug and is the behaviour being matched.
pub fn available_regions(die: &Rect, blocked: &[Rect]) -> Vec<Rect> {
    let mut out = Vec::new();
    for boundary in BOUNDARY_ORDER {
        let mut regions = vec![boundary_rect(die, boundary)];
        for block in blocked.iter().filter(|b| boundary_of(die, b) == boundary) {
            let mut next = Vec::new();
            for region in &regions {
                match contains(region, block).then(|| subtract_overlap(die, region, block)) {
                    Some(Some(pieces)) => next.extend(pieces),
                    _ => next.push(*region),
                }
            }
            regions = next;
        }
        out.extend(regions);
    }
    out
}

/// Does `outer` wholly contain `inner`? ⚠️ Inclusive on every side, as `odb::Rect::contains` is —
/// a blocked region flush against the end of an edge is still contained.
fn contains(outer: &Rect, inner: &Rect) -> bool {
    outer.x_min <= inner.x_min
        && outer.y_min <= inner.y_min
        && outer.x_max >= inner.x_max
        && outer.y_max >= inner.y_max
}
