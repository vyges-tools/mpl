// SPDX-License-Identifier: Apache-2.0
//! The feasibility checks run before any clustering happens.
//!
//! 🔑 **At the f11b4276 pin (#12140, #12258) they are upstream's `FeasibilityChecker`**, run by
//! `MacroPlacer::run` after `HardMacroFactory::create` and before `HierRTLMP::run`: no unfixed
//! macros (MPL-13 skip) → MPL-6 per macro → MPL-65 → target utilization (MPL-14) → virtual
//! utilization (MPL-7 / MPL-9) → MPL-48 if any failed. All before the module metrics, so a design
//! that fails never produces a tree. (Before the pin: MPL-6 → MPL-65 → metrics → MPL-17 → MPL-16,
//! each an immediate error.)

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

/// What [`FeasibilityChecker`] needs of one hard macro (every block, ignorable ones included,
/// as `HardMacroFactory::create` makes them): its size and box WITH halo, and its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardMacroView {
    pub inst: usize,
    pub is_fixed: bool,
    /// Fixed and outside the placement area: no halo, and skipped by the checks below.
    pub is_ignorable: bool,
    pub width: i64,
    pub height: i64,
    /// `HardMacro::getBBox`: anchored at the instance box less the left/bottom halo.
    pub bbox: Rect,
}

impl HardMacroView {
    fn area(&self) -> i64 {
        self.width * self.height
    }
}

/// What the checker ended with.
#[derive(Debug, Clone, PartialEq)]
pub struct Feasibility {
    /// The warnings it logged, in order.
    pub messages: Vec<crate::messages::Message>,
    /// `foundMacrosToPlace`: some hard macro is not fixed. False skips the placement (MPL-13).
    pub found_macros_to_place: bool,
    /// False ends the run with MPL-48 after every warning.
    pub feasible: bool,
}

/// Upstream `FeasibilityChecker` (OpenROAD #12258, in the f11b4276 pin), run by
/// `MacroPlacer::run` between the hard macro factory and the placer — the call sequence:
///
/// 1. `checkUnfixedMacros`: no unfixed hard macro returns at once (the caller logs MPL-13 and
///    skips; nothing below runs, so an all-fixed design is never judged).
/// 2. `checkMacrosFitInCore`: MPL-6 WARNING per non-ignorable macro wider or taller (with halo)
///    than the CORE, with its dimensions table, in instance-id order.
/// 3. `checkMovableCellsFit`: MPL-65 warning ([`movable_cells_fit`]).
/// 4. a target utilization outside (0, 1] warns MPL-14; otherwise `checkVirtualUtilization`:
///    inflated std cells + unfixed macros (with halo) + fixed macros (with halo, clipped) +
///    blockages (clipped, unioned) at or over the floorplan warns MPL-7 with an areas table;
///    under it but above 0.9 (and some std cell area) warns MPL-9.
/// 5. any failed check: MPL-48 error — every check warns first, the run fails at the end.
///
/// ⚠️ This replaces the old `ClusteringEngine::init` errors: MPL-6 and MPL-65 were errors at
/// the first failure, MPL-17 "No unfixed macros." came after MPL-6, and the halo-area test
/// MPL-16 is gone (the virtual utilization stands in for it).
pub struct FeasibilityChecker<'a> {
    pub design: &'a Design,
    pub hard_macros: &'a [HardMacroView],
    pub floorplan_shape: Rect,
    pub blockages: &'a [Rect],
    pub target_utilization: f32,
    pub dbu_per_micron: i32,
}

impl FeasibilityChecker<'_> {
    pub fn run(&self) -> Feasibility {
        use crate::messages::Message;
        let mut out = Feasibility { messages: Vec::new(), found_macros_to_place: false, feasible: true };
        out.found_macros_to_place = self.hard_macros.iter().any(|m| !m.is_fixed);
        if !out.found_macros_to_place {
            return out;
        }
        self.check_macros_fit_in_core(&mut out);
        let area = |i: usize| self.hard_macros.iter().find(|m| m.inst == i).map_or(0, |m| m.area());
        if !movable_cells_fit(self.design, &self.floorplan_shape, &area, self.blockages) {
            out.messages.push(Message::Warn(65, "The movable cells do not fit in the macro placement area.".into()));
            out.feasible = false;
        }
        if self.target_utilization > 0.0 && self.target_utilization <= 1.0 {
            self.check_virtual_utilization(&mut out);
        } else {
            out.messages.push(Message::Warn(14, format!("The target utilization {:.2} is outside (0, 1].", self.target_utilization)));
            out.feasible = false;
        }
        out
    }

    fn microns(&self, v: i64) -> f64 {
        v as f64 / self.dbu_per_micron as f64
    }

    fn check_macros_fit_in_core(&self, out: &mut Feasibility) {
        let core = &self.design.core_area;
        let (cw, ch) = (core.x_max - core.x_min, core.y_max - core.y_min);
        for m in self.hard_macros.iter().filter(|m| !m.is_ignorable) {
            if m.width > cw || m.height > ch {
                let table = format!(
                    "\n          |   Macro + Halos   |   Core   \n-----------------------------------------\n   Width  | {:>17.2} | {:>8.2}\n  Height  | {:>17.2} | {:>8.2}\n",
                    self.microns(m.width),
                    self.microns(cw),
                    self.microns(m.height),
                    self.microns(ch)
                );
                out.messages.push(crate::messages::Message::Warn(
                    6,
                    format!("Found macro that does not fit in the core.\nName: {}\n{table}", self.design.instances[m.inst].name),
                ));
                out.feasible = false;
            }
        }
    }

    fn blockages_area(&self) -> i64 {
        let clipped: Vec<Rect> = self.blockages.iter().filter_map(|b| intersection(b, &self.floorplan_shape)).collect();
        union_area(&clipped)
    }

    fn check_virtual_utilization(&self, out: &mut Feasibility) {
        let std_cell_area: i64 = self
            .design
            .instances
            .iter()
            .filter(|i| !(i.is_block || i.master.is_pad || i.master.is_cover || i.master.is_end_cap))
            .map(|i| i.bbox.area())
            .sum();
        // `static_cast<int64_t>(std_cell_area / static_cast<double>(target_utilization_))`
        let inflated = (std_cell_area as f64 / self.target_utilization as f64) as i64;
        let unfixed: i64 = self.hard_macros.iter().filter(|m| !m.is_fixed).map(|m| m.area()).sum();
        let fixed: i64 = self
            .hard_macros
            .iter()
            .filter(|m| m.is_fixed && !m.is_ignorable)
            .filter_map(|m| intersection(&m.bbox, &self.floorplan_shape))
            .map(|r| r.area())
            .sum();
        let blockages = self.blockages_area();
        let total = inflated + unfixed + fixed + blockages;
        let floorplan = self.floorplan_shape.area();
        let virtual_utilization = total as f64 / floorplan as f64;
        if total >= floorplan {
            let um2 = |a: i64| a as f64 / (self.dbu_per_micron as f64 * self.dbu_per_micron as f64);
            let mut table = format!("\n  {:<30} | {:>12}\n-----------------------------------------------", "", "Area (um^2)");
            for (label, a) in [
                (format!("Std cells at utilization {:.2}", self.target_utilization), inflated),
                ("Macros with halos".to_string(), unfixed),
                ("Fixed macros".to_string(), fixed),
                ("Blockages".to_string(), blockages),
                ("Total".to_string(), total),
                ("Floorplan".to_string(), floorplan),
            ] {
                table += &format!("\n  {label:<30} | {:>12.2}", um2(a));
            }
            table.push('\n');
            out.messages.push(crate::messages::Message::Warn(
                7,
                format!("The virtual utilization is {virtual_utilization:.2}: the total area exceeds the floorplan area.\n{table}"),
            ));
            out.feasible = false;
        } else if std_cell_area > 0 && virtual_utilization > VIRTUAL_UTIL_WARN_THRESHOLD as f64 {
            out.messages.push(crate::messages::Message::Warn(
                9,
                format!(
                    "The virtual utilization {virtual_utilization:.2} is above {:.1}. The annealer will probably struggle to find a valid result and may increase the std cell density to converge.",
                    VIRTUAL_UTIL_WARN_THRESHOLD
                ),
            ));
        }
    }
}

/// `kVirtualUtilWarnThreshold`: the annealer may struggle to find a valid result above this.
const VIRTUAL_UTIL_WARN_THRESHOLD: f32 = 0.9;

/// Upstream's MPL-16 test (REMOVED by #12258 at the f11b4276 pin; kept for the record and its
/// tests): the macros' area **with halos** plus the standard-cell area against the
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
