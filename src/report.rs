// SPDX-License-Identifier: Apache-2.0
//! Upstream `ClusteringEngine::reportDesignData`, reproduced line for line.
//!
//! 🔑 **This is the ONLY place a resolved halo becomes visible before the commit stage.**
//! `Area of macros with halos` is the sum of every *unfixed* macro's area taken with its halo, so
//! a halo rule that is wrong by one side changes this number and nothing else in the run. Without
//! it the halo code is exercised by a single MPL-65 verdict and scored by nothing.
//!
//! ⚠️ **`Area of macros` and `Area of macros with halos` count DIFFERENT SETS.** The first is
//! every macro; the second only the ones to be placed. A fixed macro makes the second *smaller*
//! than the first, which reads like an error and is not.

use crate::design::{Design, Rect};

/// The twelve numbers upstream reports, before formatting.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignReport {
    pub die: Rect,
    pub floorplan: Rect,
    pub num_std_cell: i32,
    pub std_cell_area: i64,
    pub num_macro: i32,
    pub macros_to_place: usize,
    pub macro_area: i64,
    pub base_halo: crate::options::Halo,
    pub macro_with_halo_area: i64,
    pub floorplan_area: i64,
    pub manufacturing_grid: i32,
}

impl DesignReport {
    /// ⚠️ Both utilizations are computed in `double` from database units and only then rounded,
    /// so rounding the inputs first gives a different last digit.
    pub fn design_utilization(&self) -> f64 {
        (self.std_cell_area + self.macro_area) as f64 / self.floorplan_area as f64
    }

    /// 🔑 The denominator is the floorplan area **minus the macro area** — the space left for
    /// standard cells — not the floorplan area.
    pub fn floorplan_utilization(&self) -> f64 {
        self.std_cell_area as f64 / (self.floorplan_area - self.macro_area) as f64
    }

    /// Upstream's exact text, so the two can be diffed rather than interpreted.
    pub fn render(&self, dbu: i32) -> String {
        let u = |v: i64| v as f64 / dbu as f64;
        let a = |v: i64| v as f64 / (dbu as f64 * dbu as f64);
        format!(
            "Die Area: ({:.2}, {:.2}) ({:.2}, {:.2}),  Floorplan Area: ({:.2}, {:.2}) ({:.2}, {:.2})\n\
             \tNumber of std cell instances: {}\n\
             \tArea of std cell instances: {:.2}\n\
             \tNumber of macros: {}\n\
             \tMacros to be placed: {}\n\
             \tArea of macros: {:.2}\n\
             \tBase halo (L, B, R, T): ({:.2}, {:.2}, {:.2}, {:.2})\n\
             \tArea of macros with halos: {:.2}\n\
             \tArea of std cell instances + Area of macros: {:.2}\n\
             \tFloorplan area: {:.2}\n\
             \tDesign Utilization: {:.2}\n\
             \tFloorplan Utilization: {:.2}\n\
             \tManufacturing Grid: {}\n",
            u(self.die.x_min), u(self.die.y_min), u(self.die.x_max), u(self.die.y_max),
            u(self.floorplan.x_min), u(self.floorplan.y_min),
            u(self.floorplan.x_max), u(self.floorplan.y_max),
            self.num_std_cell,
            a(self.std_cell_area),
            self.num_macro,
            self.macros_to_place,
            a(self.macro_area),
            u(self.base_halo.left), u(self.base_halo.bottom),
            u(self.base_halo.right), u(self.base_halo.top),
            a(self.macro_with_halo_area),
            a(self.std_cell_area + self.macro_area),
            a(self.floorplan_area),
            self.design_utilization(),
            self.floorplan_utilization(),
            self.manufacturing_grid,
        )
    }
}

/// Everything the report needs that the engine already computes.
pub fn design_report(
    design: &Design,
    floorplan: &Rect,
    metrics: &crate::design::ModuleMetrics,
    base_halo: crate::options::Halo,
    macro_with_halo_area: i64,
    macros_to_place: usize,
    manufacturing_grid: i32,
) -> DesignReport {
    DesignReport {
        die: design.die_area,
        floorplan: *floorplan,
        num_std_cell: metrics.num_std_cell,
        std_cell_area: metrics.std_cell_area,
        num_macro: metrics.num_macro,
        macros_to_place,
        macro_area: metrics.macro_area,
        base_halo,
        macro_with_halo_area,
        floorplan_area: floorplan.area(),
        manufacturing_grid,
    }
}
