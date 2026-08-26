// SPDX-License-Identifier: Apache-2.0
//! Hierarchical macro placement — the stage that runs once every cluster has a shape.
//!
//! 🔑 **Where coarse shaping asked "what size should this cluster be", this stage asks "where does
//! it go".** The two share an annealer, but almost nothing else: shaping ran with wirelength,
//! guidance, fence and every soft weight at ZERO, so its cost was area and outline alone. Here the
//! defaults are area `0.1`, outline `100`, wirelength `100`, guidance `10` and soft blockage `50`
//! — five live terms instead of two, and the wirelength one needs a netlist model that shaping
//! never had to build.

/// Upstream `computeTinyClusterMaxNumberOfStdCells`.
///
/// ⚠️ **A thousandth of the block's instance count, TRUNCATED.** The product is computed in `f32`
/// and assigned to an `int`, so a block with fewer than 1000 instances gets a threshold of zero —
/// and a threshold of zero means no cluster is ever "tiny", because the test is a strict `<`.
///
/// ⚠️ It counts **every instance in the block**, not the standard cells among them, despite naming
/// standard cells. A macro-heavy block therefore gets a larger threshold than its cell count alone
/// would justify.
pub fn tiny_cluster_max_number_of_std_cells(block_instance_count: usize) -> i32 {
    const TINY_CLUSTER_RATIO: f32 = 0.001;
    (TINY_CLUSTER_RATIO * block_instance_count as f32) as i32
}

/// Upstream `adjustSoftBlockageWeight`.
///
/// 🔑 **Only a single-level tree is adjusted.** With one level there is no hierarchy to separate
/// clusters, so the soft-blockage term is raised to half the outline weight to do that work
/// instead. A deeper tree is left alone.
///
/// ⚠️ The division is by `2.0` — a `double` — so the result is computed in double and narrowed
/// back to the `float` the weight is held in.
pub fn adjusted_soft_blockage_weight(
    max_level: i32,
    outline_weight: f32,
    current_soft_blockage_weight: f32,
) -> f32 {
    if max_level == 1 {
        (outline_weight as f64 / 2.0) as f32
    } else {
        current_soft_blockage_weight
    }
}

// ---------------------------------------------------------------- the netlist model

use crate::halo::Boundary;

/// One weighted connection between two macros in the current placement problem.
///
/// ⚠️ **Undirected**, and upstream's equality compares the terminals ONLY — two nets between the
/// same pair with different weights are "equal". Nothing here relies on that, but a `dedup` would.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BundledNet {
    pub source: usize,
    pub target: usize,
    pub weight: f32,
}

/// The weight upstream gives a parent's virtual connections.
pub const VIRTUAL_CONNECTION_WEIGHT: f32 = 10.0;

/// Upstream `buildBundledNets` for soft macros.
///
/// 🔑 **Order is part of the answer.** The virtual connections come first, in the parent's own
/// order, then each child's connections in child order — and within a child, in cluster-id order,
/// because upstream walks a `std::map`. The wirelength is a float sum, so a different order is a
/// different number in the last bits.
///
/// ⚠️ **`>` , strictly.** A connection is emitted only when the child's id is GREATER than the
/// target's, which halves the undirected pairs — and silently drops a self-connection, where the
/// two ids are equal.
pub fn build_bundled_nets(
    virtual_connections: &[(usize, usize)],
    children: &[(usize, Vec<(usize, f32)>)],
    macro_id_of_cluster: &dyn Fn(usize) -> usize,
) -> Vec<BundledNet> {
    let mut nets = Vec::new();
    for &(a, b) in virtual_connections {
        nets.push(BundledNet {
            source: macro_id_of_cluster(a),
            target: macro_id_of_cluster(b),
            weight: VIRTUAL_CONNECTION_WEIGHT,
        });
    }
    for (child_id, connections) in children {
        let source = macro_id_of_cluster(*child_id);
        for &(target_cluster, weight) in connections {
            if *child_id > target_cluster {
                nets.push(BundledNet { source, target: macro_id_of_cluster(target_cluster), weight });
            }
        }
    }
    nets
}

/// A stretch of a die edge, as the placement stage sees it. ⚠️ Always a LINE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub boundary: Boundary,
}

/// Upstream `computeNearestPointInRegion`: the closest point of a boundary line to a target.
///
/// ⚠️ **The axis tested depends on the boundary.** A vertical edge (left or right) clamps in Y and
/// keeps the line's X; a horizontal one clamps in X. Using the wrong axis returns a point that is
/// on the line but at the wrong end of it.
///
/// ⚠️ The comparisons are `>=` and `<=`, so a target level with an endpoint snaps to that
/// endpoint rather than projecting onto the line.
pub fn nearest_point_in_region(region: &Region, target: (i32, i32)) -> (i32, i32) {
    let (tx, ty) = target;
    if matches!(region.boundary, Boundary::L | Boundary::R) {
        if ty >= region.y1 {
            return (region.x1, region.y1);
        }
        if ty <= region.y0 {
            return (region.x0, region.y0);
        }
        return (region.x0, ty);
    }
    if tx >= region.x1 {
        return (region.x1, region.y1);
    }
    if tx <= region.x0 {
        return (region.x0, region.y0);
    }
    (tx, region.y0)
}

/// Upstream `computeDistToNearestRegion`.
///
/// ⚠️ **Minimised on the SQUARED distance and square-rooted once at the end**, then truncated to
/// an integer. Taking the root inside the loop would round every candidate and could pick a
/// different region.
///
/// ⛔ Returns `None` for an empty list; upstream raises MPL-47 instead, which only the
/// unconstrained-pin path can trigger.
pub fn dist_to_nearest_region(source: (i32, i32), regions: &[Region]) -> Option<i64> {
    let mut smallest = i64::MAX;
    for region in regions {
        let (nx, ny) = nearest_point_in_region(region, source);
        let (dx, dy) = ((nx - source.0) as i64, (ny - source.1) as i64);
        let squared = dx * dx + dy * dy;
        if squared < smallest {
            smallest = squared;
        }
    }
    if regions.is_empty() {
        return None;
    }
    Some((smallest as f64).sqrt() as i64)
}

/// Upstream `SoftMacro::getPinX` / `getPinY`: the macro's CENTRE.
///
/// ⚠️ **`x + 0.5 * width` computed in floating point and truncated on return.** An odd width
/// therefore rounds the centre down, and computing `x + width / 2` in integers happens to agree —
/// but only because both truncate the same way for non-negative coordinates.
pub fn pin_center(origin: i32, extent: i32) -> i32 {
    (origin as f64 + 0.5 * extent as f64) as i32
}

/// A macro, as the wirelength model needs to see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WirelengthMacro {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// ⚠️ Only the net's TARGET is ever tested for this — the model is asymmetric.
    pub is_cluster_of_unplaced_io_pins: bool,
    pub is_unconstrained_io_cluster: bool,
}

impl WirelengthMacro {
    pub fn pin(&self) -> (i32, i32) {
        (pin_center(self.x, self.width), pin_center(self.y, self.height))
    }
}

/// Upstream `isOutsideTheOutline`.
///
/// ⚠️ **The macro's PIN is compared against the outline's DIMENSIONS**, not against its far edge —
/// the coordinates are already relative to the outline's origin. And it is `>`, so a pin exactly
/// on the boundary counts as inside.
pub fn is_outside_the_outline(macro_: &WirelengthMacro, outline: (i32, i32)) -> bool {
    let (px, py) = macro_.pin();
    px > outline.0 || py > outline.1
}

/// Upstream `computeWLForClusterOfUnplacedIOPins`.
///
/// 🔑 **A macro outside the outline is charged the whole die, so the annealer is pushed hard to
/// bring it back in** before any pin-proximity refinement matters.
///
/// ⚠️ The result is `float * int64` **truncated to an integer**, and then added to a running
/// `f32` sum — two different precisions in one expression.
pub fn wirelength_for_unplaced_io_pins(
    macro_: &WirelengthMacro,
    target: &WirelengthMacro,
    net_weight: f32,
    outline: (i32, i32),
    die_span: i64,
    available_regions: &[Region],
    constraint_region: Option<Region>,
) -> i64 {
    if is_outside_the_outline(macro_, outline) {
        return (net_weight as f64 * die_span as f64) as i64;
    }
    let smallest = if target.is_unconstrained_io_cluster {
        // ⛔ Upstream raises MPL-47 when there is no region at all.
        dist_to_nearest_region(macro_.pin(), available_regions).unwrap_or(0)
    } else {
        match constraint_region {
            Some(region) => dist_to_nearest_region(macro_.pin(), &[region]).unwrap_or(0),
            None => 0,
        }
    };
    (net_weight as f64 * smallest as f64) as i64
}

/// Upstream `computeNetsWireLength`: weighted half-perimeter, normalised.
///
/// 🔑 **Normalised twice** — by the total net weight and by the outline's semi-perimeter — so the
/// result is a dimensionless fraction rather than a distance, and comparable across outlines.
///
/// ⛔ **`weight_sum` is summed over the CORE'S OWN net list, not over the `nets` argument.**
/// Upstream writes `for (const auto& net : nets_)` for the sum and `for (const auto& net : nets)`
/// for the length, one character apart. Its only caller passes `nets_`, so the two agree today;
/// they are kept as separate parameters here so the difference stays visible rather than being
/// quietly unified.
///
/// ⚠️ **Only the TARGET is tested for being a cluster of unplaced IO pins.** A net whose SOURCE is
/// such a cluster takes the ordinary half-perimeter path.
///
/// ⚠️ A zero weight sum returns zero rather than dividing by it.
#[allow(clippy::too_many_arguments)]
pub fn compute_nets_wire_length(
    nets: &[BundledNet],
    weight_sum_nets: &[BundledNet],
    macros: &[WirelengthMacro],
    outline: (i32, i32),
    die_span: i64,
    available_regions: &[Region],
    constraint_region_of: &dyn Fn(usize) -> Option<Region>,
) -> f32 {
    let weight_sum: f32 = weight_sum_nets.iter().fold(0.0f32, |a, n| a + n.weight);
    if weight_sum == 0.0 {
        return 0.0;
    }

    let mut total = 0.0f32;
    for net in nets {
        let source = &macros[net.source];
        let target = &macros[net.target];
        if target.is_cluster_of_unplaced_io_pins {
            total += wirelength_for_unplaced_io_pins(
                source,
                target,
                net.weight,
                outline,
                die_span,
                available_regions,
                constraint_region_of(net.target),
            ) as f32;
        } else {
            let (x1, y1) = source.pin();
            let (x2, y2) = target.pin();
            total += net.weight * ((x2 - x1).abs() + (y2 - y1).abs()) as f32;
        }
    }

    total / weight_sum / (outline.0 + outline.1) as f32
}
