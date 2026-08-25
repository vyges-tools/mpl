// SPDX-License-Identifier: Apache-2.0
//! The feasibility checks `ClusteringEngine::init` runs before any clustering happens.
//!
//! 🔑 **These come FIRST, and one of them comes before the vacuous test.** Upstream's order is
//! `setFloorplanShape` → `createHardMacros` (MPL-6) → `movableCellsFitInMacroPlacementArea`
//! (MPL-65) → module metrics → *no unfixed macros* (MPL-17) → the halo-area test (MPL-16). A
//! design that fails MPL-65 never reaches clustering at all, so running them late would produce
//! a tree for a design upstream refuses.

use crate::design::{Design, Rect};

/// Upstream `movableCellsFitInMacroPlacementArea` — the MPL-65 test.
///
/// ⚠️ **Every instance counts, with no ignored-instance filter.** Tapcells, end caps and pads all
/// contribute their bounding box here, unlike almost everywhere else in the engine.
///
/// The three cases, in upstream's order:
/// - **fixed** → only the part INSIDE the placement area, because a fixed cell is allowed to sit
///   outside it, and a physical marker may straddle the edge;
/// - **an unfixed macro** → its area **with halo**, not its bounding box;
/// - **anything else** → its bounding box.
///
/// Blockages are added as a **union**, not a sum: two overlapping blockages occupy the area once.
pub fn movable_cells_fit(
    design: &Design,
    area: &Rect,
    macro_area_with_halo: &dyn Fn(usize) -> i64,
    blockages: &[Rect],
) -> bool {
    let mut occupied: i64 = 0;
    for (i, inst) in design.instances.iter().enumerate() {
        if inst.is_fixed {
            occupied += intersection(&inst.bbox, area).map_or(0, |r| r.area());
            continue;
        }
        occupied += if inst.is_block { macro_area_with_halo(i) } else { inst.bbox.area() };
    }
    let clipped: Vec<Rect> = blockages.iter().filter_map(|b| intersection(b, area)).collect();
    occupied += union_area(&clipped);
    occupied <= area.area()
}

/// The area covered by at least one rectangle.
///
/// 🔑 Upstream reaches for `boost::polygon`'s 90-degree polygon set for this; the answer it wants
/// is only the area, so a sweep over the distinct x edges gives the same number. ⚠️ Summing the
/// rectangles instead would double-count every overlap and refuse designs that fit.
pub fn union_area(rects: &[Rect]) -> i64 {
    let mut xs: Vec<i64> = rects.iter().flat_map(|r| [r.x_min, r.x_max]).collect();
    xs.sort_unstable();
    xs.dedup();
    let mut total = 0i64;
    for w in xs.windows(2) {
        let (x0, x1) = (w[0], w[1]);
        let mut spans: Vec<(i64, i64)> = rects
            .iter()
            .filter(|r| r.x_min <= x0 && r.x_max >= x1 && r.y_max > r.y_min)
            .map(|r| (r.y_min, r.y_max))
            .collect();
        spans.sort_unstable();
        let mut covered = 0i64;
        let mut open: Option<(i64, i64)> = None;
        for (a, b) in spans {
            match open {
                Some((lo, hi)) if a <= hi => open = Some((lo, hi.max(b))),
                Some((lo, hi)) => {
                    covered += hi - lo;
                    open = Some((a, b));
                }
                None => open = Some((a, b)),
            }
        }
        if let Some((lo, hi)) = open {
            covered += hi - lo;
        }
        total += covered * (x1 - x0);
    }
    total
}

/// The overlapping part of two rectangles, or `None` when they do not overlap.
fn intersection(a: &Rect, b: &Rect) -> Option<Rect> {
    let r = Rect {
        x_min: a.x_min.max(b.x_min),
        y_min: a.y_min.max(b.y_min),
        x_max: a.x_max.min(b.x_max),
        y_max: a.y_max.min(b.y_max),
    };
    (r.x_max > r.x_min && r.y_max > r.y_min).then_some(r)
}

/// Upstream's MPL-16 test: the macros' area **with halos** plus the standard-cell area against the
/// floorplan area.
///
/// ⚠️ **Upstream computes this in `float`, not in integers.** At database-unit areas the 24-bit
/// mantissa rounds, so a design near the boundary can pass or fail on the rounding alone —
/// reproduced rather than corrected, because the verdict is what we are matching.
pub fn instance_area_with_halos_fits(
    macro_with_halo_area: i64,
    std_cell_area: i64,
    floorplan_area: i64,
) -> bool {
    let inst_area_with_halos = macro_with_halo_area as f32 + std_cell_area as f32;
    inst_area_with_halos <= floorplan_area as f32
}

/// Upstream's MPL-6 test, applied per macro as `createHardMacros` builds it.
///
/// ⚠️ The comparison is against the **core area**, not the floorplan shape a global fence may have
/// narrowed — and the macro's dimensions are taken **with halo**.
pub fn macro_fits_in_core(width_with_halo: i64, height_with_halo: i64, core: &Rect) -> bool {
    width_with_halo <= core.x_max - core.x_min && height_with_halo <= core.y_max - core.y_min
}
