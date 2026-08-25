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

// ---------------------------------------------------------------- pin access blockages

use crate::shaping::DepthLimits;

/// A stretch of one die edge, and which edge it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryRegion {
    /// ⚠️ A LINE, held as a degenerate rectangle — one of its dimensions is always zero.
    pub line: Rect,
    pub boundary: Boundary,
}

/// The length of a region.
///
/// ℹ️ Upstream writes this as `Rect::margin() / 2`, and `margin()` is the full perimeter
/// (`2·dx + 2·dy`), so half of it is `dx + dy`. For a line one of those is zero, which makes it
/// the length. ⚠️ Kept as `dx + dy` rather than `max(dx, dy)`: the two agree only for a line, and
/// a caller that passed a real rectangle would get upstream's answer rather than a better one.
pub fn region_length(line: &Rect) -> i64 {
    (line.x_max - line.x_min) + (line.y_max - line.y_min)
}

/// How much deeper a blockage grows for a region carrying more than its share of the IOs.
///
/// ⚠️ Computed in `f32` and applied by truncating multiplication, both as upstream does.
pub fn io_density_factor(ios_here: i64, ios_total: i64) -> f32 {
    1.0 + (ios_here as f32 / ios_total as f32)
}

/// `base_depth * factor`, truncated.
pub fn scale_depth(base_depth: i64, factor: f32) -> i64 {
    (base_depth as f32 * factor) as i64
}

/// Clamp a depth into the limits for its axis.
///
/// 🔑 **A VERTICAL boundary uses the X limits.** `is_vertical` asks whether the *edge* runs
/// vertically, and a blockage on the left or right edge grows in **x** — so the pairing that
/// reads inverted is the correct one, and swapping it produces blockages of the wrong depth on
/// every edge without changing their shape.
pub fn clamp_depth(depth: i64, boundary: Boundary, limits: &DepthLimits) -> i64 {
    let (min, max) = if boundary.is_vertical() {
        (limits.x_min, limits.x_max)
    } else {
        (limits.y_min, limits.y_max)
    };
    // ⚠️ `else if`, not two independent tests: with min > max the maximum wins, because the
    // second branch is never reached. Reproduced.
    if depth > max {
        max
    } else if depth < min {
        min
    } else {
        depth
    }
}

/// The blockage a region casts into the core: the line, grown inward by `depth`.
pub fn pin_access_blockage(region: &BoundaryRegion, depth: i64, limits: &DepthLimits) -> Rect {
    let d = clamp_depth(depth, region.boundary, limits);
    let mut r = region.line;
    match region.boundary {
        Boundary::L => r.x_max = r.x_min + d,
        Boundary::R => r.x_min = r.x_max - d,
        Boundary::B => r.y_max = r.y_min + d,
        Boundary::T => r.y_min = r.y_max - d,
    }
    r
}

// ---------------------------------------------------------------- the three builders

/// One region and how many IO pins it carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoRegion {
    pub region: BoundaryRegion,
    pub ios: i64,
}

/// Upstream `createBlockagesForIOBundles` and `createBlockagesForConstraintRegions`.
///
/// The two are the same shape and differ only in what they count: bundles divide by the number of
/// **fixed** block ports, constraint regions by the number of **unfixed** ones. Both sum their own
/// regions' lengths for the span, then scale the one base depth per region by its IO density.
///
/// ⚠️ **The span is the sum over the regions, and the base depth is computed ONCE from it** — not
/// per region. A region twice as long does not get twice the depth; it lowers the depth of every
/// region, its own included.
pub fn blockages_for_regions(
    regions: &[IoRegion],
    ios_total: i64,
    base_depth_for_span: &dyn Fn(i64) -> i64,
    limits: &DepthLimits,
) -> Vec<Rect> {
    if regions.is_empty() {
        return Vec::new();
    }
    let span: i64 = regions.iter().map(|r| region_length(&r.region.line)).sum();
    let base = base_depth_for_span(span);
    regions
        .iter()
        .map(|r| {
            let depth = scale_depth(base, io_density_factor(r.ios, ios_total));
            pin_access_blockage(&r.region, depth, limits)
        })
        .collect()
}

/// Upstream `createBlockagesForAvailableRegions`.
///
/// 🔑 **No density factor at all** — every available region gets the same base depth. The other
/// two builders scale per region; this one does not, because an available region is space no pin
/// has claimed, so there is no count to scale by.
///
/// ⚠️ **The guard is on the BLOCKED regions, not the available ones.** A design with nothing
/// blocked produces no blockages here even though every edge is available — which is the point,
/// since with nothing blocked the pins are unconstrained everywhere.
pub fn blockages_for_available_regions(
    available: &[BoundaryRegion],
    any_blocked: bool,
    base_depth_for_span: &dyn Fn(i64) -> i64,
    limits: &DepthLimits,
) -> Vec<Rect> {
    if !any_blocked {
        return Vec::new();
    }
    let span: i64 = available.iter().map(|r| region_length(&r.line)).sum();
    let base = base_depth_for_span(span);
    available.iter().map(|r| pin_access_blockage(r, base, limits)).collect()
}
