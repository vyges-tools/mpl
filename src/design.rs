// SPDX-License-Identifier: Apache-2.0
//! Reading the design out of the database, and the rules that classify what is read.
//!
//! 🔑 **The database is read once into plain data, then every rule runs on that data.** The
//! classification rules below decide which instances count, which are ignored, and what a module
//! is worth — and each of them is a place a placer can go quietly wrong. Keeping them pure means
//! they are tested with values rather than with a design, so a wrong answer shows up as a failing
//! assertion instead of as macros in the wrong place.

/// A rectangle in database units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x_min: i64,
    pub y_min: i64,
    pub x_max: i64,
    pub y_max: i64,
}

impl Rect {
    pub fn area(&self) -> i64 {
        (self.x_max - self.x_min).max(0) * (self.y_max - self.y_min).max(0)
    }

    /// ⚠️ Touching edges do NOT overlap. A macro abutting the placement area is outside it.
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x_min < other.x_max
            && other.x_min < self.x_max
            && self.y_min < other.y_max
            && other.y_min < self.y_max
    }

    pub fn intersection(&self, other: &Rect) -> Rect {
        let r = Rect {
            x_min: self.x_min.max(other.x_min),
            y_min: self.y_min.max(other.y_min),
            x_max: self.x_max.min(other.x_max),
            y_max: self.y_max.min(other.y_max),
        };
        if r.x_max <= r.x_min || r.y_max <= r.y_min {
            Rect::default()
        } else {
            r
        }
    }
}

/// What the master of an instance is, to the extent the classification rules care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MasterKind {
    pub is_pad: bool,
    pub is_cover: bool,
    pub is_end_cap: bool,
}

/// One placed or placeable instance.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub name: String,
    /// A macro. Upstream's `isBlock()`.
    pub is_block: bool,
    pub is_fixed: bool,
    pub bbox: Rect,
    pub master: MasterKind,
    /// Set for a **fixed macro that does not overlap the placement area**, which is dropped from
    /// consideration entirely. Upstream's `ignorable_macros_`.
    pub is_ignorable_macro: bool,
}

/// A node of the logical module hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: String,
    pub hierarchical_name: String,
    /// Instances owned directly by this module.
    pub insts: Vec<usize>,
    /// Child modules, by index.
    pub children: Vec<usize>,
}

/// The design, read once.
///
/// ⚠️ **Two different scopes live here, and confusing them is a real bug.**
///
/// - [`instances`](Self::instances) is **every** instance in the block, including physical-only
///   cells — tapcells, decaps, end-caps — inserted by a physical tool.
/// - [`modules`](Self::modules) is the **logical netlist hierarchy**, and it does **not** own
///   those physical cells. A module-walk therefore reaches fewer instances than the block holds.
///
/// 🔑 Measured on a real design: 229 instances in the block, **47** owned by the top module, and
/// the other 182 all `physical_only`. Both scopes are correct and both are needed — the block
/// scope finds macros and checks occupancy, the module scope computes metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct Design {
    /// Every instance in the block. Includes physical-only cells owned by no module.
    pub instances: Vec<Instance>,
    /// The logical netlist hierarchy. Does not own physical-only cells.
    pub modules: Vec<Module>,
    pub top: usize,
    pub core_area: Rect,
    pub die_area: Rect,
}

impl Design {
    /// Instances reachable by walking the module hierarchy from the top.
    ///
    /// Use this when asking what the NETLIST contains; use `instances` when asking what is
    /// physically in the block.
    pub fn module_owned_instances(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![self.top];
        while let Some(m) = stack.pop() {
            out.extend_from_slice(&self.modules[m].insts);
            stack.extend_from_slice(&self.modules[m].children);
        }
        out
    }
}

/// Should this instance be left out of clustering entirely?
///
/// 🔑 Upstream `isIgnoredInst`: a **pad, cover or end-cap** master is ignored, and so is a macro
/// already marked ignorable (fixed and outside the placement area).
///
/// ⚠️ These are physical-only cells. Counting them would inflate a cluster's occupancy with
/// instances no placer is entitled to move.
pub fn is_ignored_inst(inst: &Instance) -> bool {
    if inst.is_block && inst.is_ignorable_macro {
        return true;
    }
    inst.master.is_pad || inst.master.is_cover || inst.master.is_end_cap
}

/// What a module contains, counted recursively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModuleMetrics {
    pub num_std_cell: i32,
    pub num_macro: i32,
    pub std_cell_area: i64,
    pub macro_area: i64,
}

/// A fixed non-macro instance sitting inside the placement area — upstream errors on this
/// (`MPL-50`), because a placer cannot honour it and cannot move it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedInstanceInArea {
    pub name: String,
}

/// Upstream `computeModuleMetrics`, recursive over the module hierarchy.
///
/// ⚠️ **The branch ORDER is the rule**, and it is not the order one would write from scratch:
///
/// 1. `is_block` → counted as a macro **without consulting `is_ignored_inst`**, so an *ignorable*
///    macro still contributes to `num_macro` and `macro_area`.
/// 2. else fixed, non-cover, and overlapping the placement area → **error**.
/// 3. else not ignored → counted as a standard cell.
///
/// 🔑 An ignored standard cell (a pad, cover or end-cap) therefore falls through all three and is
/// counted as **neither**. Reordering these changes what every threshold downstream compares.
pub fn compute_module_metrics(
    design: &Design,
    module: usize,
    placement_area: &Rect,
    errors: &mut Vec<FixedInstanceInArea>,
) -> ModuleMetrics {
    let mut m = ModuleMetrics::default();

    for &i in &design.modules[module].insts {
        let inst = &design.instances[i];
        if inst.is_block {
            m.num_macro += 1;
            m.macro_area += inst.bbox.area();
        } else if inst.is_fixed
            && !inst.master.is_cover
            && inst.bbox.overlaps(placement_area)
        {
            errors.push(FixedInstanceInArea { name: inst.name.clone() });
        } else if !is_ignored_inst(inst) {
            m.num_std_cell += 1;
            m.std_cell_area += inst.bbox.area();
        }
    }

    for &child in &design.modules[module].children {
        let c = compute_module_metrics(design, child, placement_area, errors);
        m.num_std_cell += c.num_std_cell;
        m.num_macro += c.num_macro;
        m.std_cell_area += c.std_cell_area;
        m.macro_area += c.macro_area;
    }

    m
}

/// Macros the placer may move: a macro whose placement status is not fixed.
pub fn unfixed_macros(design: &Design) -> Vec<usize> {
    design
        .instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.is_block && !i.is_fixed)
        .map(|(idx, _)| idx)
        .collect()
}

/// The area macro placement may use: the core, clipped to any global fence the user set.
///
/// ⚠️ A fence that misses the core entirely leaves nothing to place into — upstream errors
/// (`MPL-68`) rather than silently placing into the core.
pub fn floorplan_shape(core_area: &Rect, global_fence: Option<&Rect>) -> Option<Rect> {
    let shape = match global_fence {
        Some(f) => core_area.intersection(f),
        None => *core_area,
    };
    if shape.area() == 0 {
        None
    } else {
        Some(shape)
    }
}
