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
    /// The HARD macro's pin offset, when this is one.
    ///
    /// ⛔ **The two cores read different pins.** `SoftMacro::getPinX` is the macro's CENTRE;
    /// `HardMacro::getPinX` is `x_ + pin_x_`, where `pin_x_` is the centre of the master's SIGNAL
    /// pin bounding box plus half the total halo. They coincide only when the pins happen to sit
    /// in the middle, so using the centre in a macro run measures from the wrong point.
    pub pin_offset: Option<(i32, i32)>,
}

impl WirelengthMacro {
    pub fn pin(&self) -> (i32, i32) {
        match self.pin_offset {
            // `HardMacro::getPinX` — the offset is already master-relative and halo-adjusted.
            Some((dx, dy)) => (self.x + dx, self.y + dy),
            None => (pin_center(self.x, self.width), pin_center(self.y, self.height)),
        }
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
    die_margin: i64,
    available_regions: &[Region],
    constraint_region: Option<Region>,
) -> i64 {
    // ⛔ **`int64_t` narrowed to `int`.** `Rect::margin()` returns `2*dx + 2*dy` as an `int64_t`
    // and upstream assigns the halved value to a `const int`. On a die wider than 2^31 units the
    // narrowing wraps; reproduced rather than widened.
    let max_dist = (die_margin / 2) as i32;
    if is_outside_the_outline(macro_, outline) {
        // ⛔ **`float * int`, so the product is formed in `f32`** — not `f64`. A die distance runs
        // to millions of database units and an `f32` carries about seven significant digits, so
        // this result is deliberately COARSE. Computing it in `f64` gives a more accurate number
        // and a different search.
        return (net_weight * max_dist as f32) as i64;
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
    // ⛔ `float * int64_t` is also formed in `f32`; see above.
    (net_weight * smallest as f32) as i64
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
    die_margin: i64,
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
                die_margin,
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

// ---------------------------------------------------------------- the placement penalties

/// A macro as the soft-blockage term sees it: a box, a macro count, and its cluster's areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockageMacro {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub num_macro: i32,
    /// The cluster's macro area and total area — their ratio is how much the term weighs it.
    pub cluster_macro_area: i64,
    pub cluster_area: i64,
}

/// The overlap of two boxes, or `None` when they miss.
///
/// ⚠️ **`< 0`, not `<= 0`.** A zero-area touching overlap is kept and contributes nothing — but
/// two boxes that miss diagonally have BOTH dimensions negative and so a POSITIVE product, which
/// is exactly what this guard exists to reject.
fn overlap_area(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> Option<i64> {
    let (x0, y0) = (a.0.max(b.0), a.1.max(b.1));
    let (x1, y1) = (a.2.min(b.2), a.3.min(b.3));
    let (dx, dy) = (x1 - x0, y1 - y0);
    if dx < 0 || dy < 0 {
        return None;
    }
    Some(dx as i64 * dy as i64)
}

/// Upstream `SACoreSoftMacro::calSoftBlockagePenalty`.
///
/// 🔑 **Weighted by how macro-dominated each cluster is.** A cluster that is mostly standard cells
/// overlapping a soft blockage costs little; one that is nearly all macros costs the full overlap.
/// That is the whole point of the term — a soft blockage is somewhere macros should not go, not
/// somewhere nothing may go.
///
/// ⚠️ **Blockages OUTER, macros INNER.** Floating-point addition is not associative, so swapping
/// the loops changes the sum's last bits.
///
/// ⚠️ A macro with no macros in it, or a cluster of zero area, is skipped entirely — the second
/// guard also keeps the dominance ratio from dividing by zero.
///
/// ⚠️ Normalised by the TOTAL macro count across the sequence, not by the number of blockages.
pub fn soft_blockage_penalty(
    macros: &[BlockageMacro],
    order: &[usize],
    blockages: &[(i32, i32, i32, i32)],
    weight: f32,
) -> f32 {
    if blockages.is_empty() || weight <= 0.0 {
        return 0.0;
    }
    let total_macros: i32 = order.iter().map(|&i| macros[i].num_macro).sum();
    if total_macros <= 0 {
        return 0.0;
    }

    let mut penalty = 0.0f32;
    for blockage in blockages {
        for &i in order {
            let m = &macros[i];
            if m.num_macro > 0 && m.cluster_area > 0 {
                let box_ = (m.x, m.y, m.x + m.width, m.y + m.height);
                let Some(area) = overlap_area(*blockage, box_) else { continue };
                let dominance = m.cluster_macro_area as f32 / m.cluster_area as f32;
                penalty += (area * m.num_macro as i64) as f32 * dominance;
            }
        }
    }
    penalty / total_macros as f32
}

/// Upstream `calGuidancePenalty`.
///
/// 🔑 **It measures the SHORTFALL from the best possible overlap, not the overlap.** The penalty
/// starts at the largest area the macro could share with its guide — `min` of the two widths times
/// `min` of the two heights — and the actual overlap is subtracted. A macro sitting wholly inside
/// its guide therefore scores ZERO, and one entirely outside scores the full best-possible area.
///
/// ⚠️ **The subtraction is guarded by `> 0` on BOTH dimensions**, so a degenerate touching overlap
/// subtracts nothing. It would have subtracted zero anyway; the guard matters because a diagonal
/// miss has a positive area.
///
/// ⚠️ Converted to microns per guide, then averaged over the number of guides.
pub fn guidance_penalty(
    guides: &[(usize, (i32, i32, i32, i32))],
    macros: &[WirelengthMacro],
    weight: f32,
    dbu_per_micron: i32,
) -> f32 {
    if weight <= 0.0 || guides.is_empty() {
        return 0.0;
    }
    let mut penalty = 0.0f32;
    for &(id, guide) in guides {
        let m = &macros[id];
        let (guide_dx, guide_dy) = (guide.2 - guide.0, guide.3 - guide.1);
        // The largest area this macro could possibly share with the guide.
        let mut best = m.width.min(guide_dx) as i64 * m.height.min(guide_dy) as i64;

        let box_ = (m.x, m.y, m.x + m.width, m.y + m.height);
        let (x0, y0) = (box_.0.max(guide.0), box_.1.max(guide.1));
        let (x1, y1) = (box_.2.min(guide.2), box_.3.min(guide.3));
        if x1 - x0 > 0 && y1 - y0 > 0 {
            best -= (x1 - x0) as i64 * (y1 - y0) as i64;
        }
        // ⛔ `float += double`: upstream adds the `dbuAreaToMicrons` result in `f64` and rounds
        // once. See [`crate::anneal::plus_double`].
        penalty = crate::anneal::plus_double(penalty, area_to_microns_f64(best, dbu_per_micron));
    }
    penalty / guides.len() as f32
}

/// Upstream `calFencePenalty`.
///
/// 🔑 **It measures how far a macro is from having no fence violation at all**, not how far it is
/// from the fence's centre. A macro anywhere inside its fence scores zero; one outside scores the
/// overshoot, as a fraction of the outline, squared on each axis and summed.
///
/// ⚠️ **A macro with zero area is skipped**, and so is one that simply does not FIT its fence — a
/// fence smaller than the macro it constrains is treated as unsatisfiable rather than as
/// infinitely violated.
///
/// ⛔ **The zero-area test WRAPS.** It is the one place upstream multiplies two `int` dimensions
/// directly instead of going through `Rect::area()`, so a macro whose width times height is a
/// multiple of 2^32 reads as zero-area and is skipped — see the comment at the site.
///
/// ⚠️ **Both skips still count towards the divisor.** The mean is over every fence declared, not
/// over the ones that scored, so adding an unsatisfiable fence dilutes the whole term.
///
/// ⚠️ **`<=`, so a macro exactly at the limit of its slack scores zero.** And every centre and
/// half-extent is an integer division, so an odd extent loses its half unit before the comparison.
/// ℹ️ Writing `<` instead would be EQUIVALENT, not wrong — at equality the other branch evaluates
/// to zero as well. Kept as `<=` because that is the reference's spelling; do not add a mutation
/// for it, because none can fail.
///
/// ⚠️ The ratios are formed against the OUTLINE's extents, so the term is dimensionless and
/// comparable across outlines — and it is the ratio that is squared, not the distance.
pub fn fence_penalty(
    fences: &[(usize, (i32, i32, i32, i32))],
    macros: &[WirelengthMacro],
    outline: (i32, i32),
    weight: f32,
) -> f32 {
    if weight <= 0.0 || fences.is_empty() {
        return 0.0;
    }

    let mut penalty = 0.0f32;
    for &(id, fence) in fences {
        let m = &macros[id];
        let (lx, ly) = (m.x, m.y);
        let (ux, uy) = (lx + m.width, ly + m.height);

        // ⛔ **`int * int`, NOT `Rect::area()`** — upstream multiplies the two `int` dimensions
        // directly here, where everywhere else it goes through `area()`, which widens to
        // `int64_t` first. So the product WRAPS at 2^32 and a macro whose width times height is a
        // multiple of 2^32 is treated as zero-area and skipped. 65536 x 65536 database units is
        // 32.8 µm square at 2000 units per micron — an ordinary macro, not a pathological one.
        // ⚠️ Signed overflow is undefined in C++; this reproduces what the reference BUILD does at
        // the pin, which is to wrap. Candidate for an upstream report.
        if m.width.wrapping_mul(m.height) == 0 {
            continue;
        }
        let (fence_dx, fence_dy) = (fence.2 - fence.0, fence.3 - fence.1);
        if m.width > fence_dx || m.height > fence_dy {
            continue;
        }

        // How far the macro's centre may stray before any part of it leaves the fence.
        let max_x_dist = (fence_dx - (ux - lx)) / 2;
        let max_y_dist = (fence_dy - (uy - ly)) / 2;
        let x_dist = (((fence.0 + fence.2) / 2) - ((lx + ux) / 2)).abs();
        let y_dist = (((fence.1 + fence.3) / 2) - ((ly + uy) / 2)).abs();

        let width = if x_dist <= max_x_dist { 0 } else { x_dist - max_x_dist };
        let height = if y_dist <= max_y_dist { 0 } else { y_dist - max_y_dist };
        let width_ratio = width as f32 / outline.0 as f32;
        let height_ratio = height as f32 / outline.1 as f32;
        penalty += (width_ratio * width_ratio) + (height_ratio * height_ratio);
    }

    penalty / fences.len() as f32
}

/// `dbuAreaToMicrons`: an area over the square of the units per micron, in `f64` — the type the
/// database returns and the type every caller must add in. See [`crate::anneal::plus_double`].
fn area_to_microns_f64(dbu_area: i64, dbu_per_micron: i32) -> f64 {
    let d = dbu_per_micron as f64;
    dbu_area as f64 / (d * d)
}

/// `dbuToMicrons`: a LENGTH, in `f64`, exactly as the database returns it.
fn length_to_microns(dbu: i32, dbu_per_micron: i32) -> f64 {
    dbu as f64 / dbu_per_micron as f64
}

/// A macro, as the boundary term sees it: a box, whether it may move, and how many hard macros
/// it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryMacro {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub fixed: bool,
    /// ⚠️ The count of hard macros in the cluster, **not** a flag. A standard-cell cluster is
    /// zero and a hard-macro cluster is however many it holds — the term is a macro-weighted
    /// average, so a cluster of ten pulls ten times as hard as a cluster of one.
    pub num_macro: i32,
}

/// The root cluster's own box, which the boundary term measures against.
///
/// ⚠️ **Its `width` and `height` are used as if they were the far-edge COORDINATES.** That is
/// upstream's arithmetic and it is consistent, because the macro's position is first rebased onto
/// the root's origin — but it only reads as correct once that rebasing is in view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Root {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Upstream `SACoreSoftMacro::calBoundaryPenalty`.
///
/// 🔑 **It rewards macros for hugging the ROOT's edges, not this cluster's outline.** Each
/// movable macro is rebased from outline-relative coordinates into root-relative ones, and then
/// charged its distance to the nearer vertical edge plus its distance to the nearer horizontal
/// one. A macro already on a die edge costs nothing; one in the middle costs the most.
///
/// ⛔ **The two sides are NOT symmetric.** The left/bottom distance is the raw coordinate, while
/// the right/top distance is wrapped in `abs`. So a macro pushed out past the root's LEFT edge
/// scores a negative distance and *reduces* the penalty, whereas one pushed out past the RIGHT
/// edge scores a positive one and increases it. Making both `abs` would be more sensible and
/// would not reproduce the reference.
///
/// ⚠️ **Two passes over the sequence, and the denominators differ in principle.** The first pass
/// sums `num_macro` over every non-fixed macro; the second accumulates only where `num_macro > 0`.
/// They agree only because a macro with none contributes zero to both.
///
/// ⚠️ **The two distances are summed in DATABASE UNITS and converted once**, then multiplied by
/// the macro count in `f64` and added to an `f32` accumulator — so each step rounds through the
/// wider type before narrowing. Converting each distance separately, or accumulating in `f32`,
/// gives a different number in the last bits.
///
/// ⚠️ A zero weight, or a sequence with no movable macros in it, returns zero without measuring
/// anything.
pub fn boundary_penalty(
    macros: &[BoundaryMacro],
    order: &[usize],
    outline_origin: (i32, i32),
    root: &Root,
    weight: f32,
    dbu_per_micron: i32,
) -> f32 {
    if weight <= 0.0 {
        return 0.0;
    }

    let mut number_of_movable_macros = 0i32;
    for &i in order {
        let m = &macros[i];
        if m.fixed {
            continue;
        }
        number_of_movable_macros += m.num_macro;
    }
    if number_of_movable_macros == 0 {
        return 0.0;
    }

    let mut penalty = 0.0f32;
    for &i in order {
        let m = &macros[i];
        if m.fixed {
            continue;
        }
        if m.num_macro > 0 {
            let global_lx = m.x + outline_origin.0 - root.x;
            let global_ly = m.y + outline_origin.1 - root.y;
            let global_ux = global_lx + m.width;
            let global_uy = global_ly + m.height;

            // ⛔ The left/bottom term is unsigned only by accident of position; see above.
            let x_dist_from_root = global_lx.min((root.width - global_ux).abs());
            let y_dist_from_root = global_ly.min((root.height - global_uy).abs());

            let microns = length_to_microns(x_dist_from_root + y_dist_from_root, dbu_per_micron);
            penalty = (penalty as f64 + microns * m.num_macro as f64) as f32;
        }
    }

    penalty / number_of_movable_macros as f32
}

// ---------------------------------------------------------------- notches

/// A macro, as the notch term sees it: a box and whether it obstructs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotchMacro {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// ⚠️ **`None` means NO CLUSTER behind the soft macro** — a fixed terminal, or a blockage
    /// proxy. Upstream reads `cluster_ == nullptr` and both `isMacroCluster` and `isMixedCluster`
    /// answer false, so it obstructs nothing.
    ///
    /// ⛔ This used to be a bare `AreaKind` with `None` folded onto `FixedMacro`, which was safe
    /// only while a fixed macro obstructed nothing. It does obstruct, so the fold silently turned
    /// every cluster-less macro into an obstruction. A fallback is only ever as correct as the
    /// case it is folded onto.
    pub kind: Option<AreaKind>,
}

impl NotchMacro {
    /// ⛔ **Only a hard-macro cluster, a mixed cluster or a FIXED macro obstructs.** A
    /// standard-cell cluster does not, and neither does a blockage or an IO cluster.
    ///
    /// ⛔ **CORRECTED 2026-08-27 — a FIXED macro DOES obstruct.** The comment here used to say the
    /// opposite, on the grounds that the soft macro built from a fixed hard macro "never sets"
    /// `cluster_`. It does: `SoftMacro(logger, hard_macro, outline)` ends with
    /// `cluster_ = hard_macro->getCluster()`, and that cluster is a `HardMacroCluster`, so
    /// `isMacroCluster()` holds. With it excluded the space beside a fixed macro was scanned as
    /// empty and the notch term read zero on every design with one.
    ///
    /// ⚠️ The same wrong claim was written in two places — here and in the handoff — and fixing
    /// the `is_macro_cluster` flag on the soft macro did NOT fix this path, because the notch view
    /// keys off `AreaKind` instead. A belief that is recorded twice has to be corrected twice.
    fn obstructs(&self) -> bool {
        matches!(
            self.kind,
            Some(AreaKind::HardMacroCluster)
                | Some(AreaKind::MixedCluster)
                | Some(AreaKind::FixedMacro)
        )
    }
}

/// Upstream `fillCoordsLists`: the grid lines the notch scan works on.
///
/// 🔑 **Every obstructing macro's two edges on each axis become grid lines**, plus the outline's
/// own two — so the grid is exactly fine enough to describe this arrangement and no finer.
///
/// ⚠️ **Near-coincident lines are COALESCED**, at a tolerance of a hundredth of the outline's
/// extent on that axis — the x tolerance from the width, the y from the height. Without it a
/// design whose macros are a database unit apart would be scanned on a grid of hairline columns.
///
/// ⛔ **The survivor of a coalesced group is the LARGEST, not the smallest.** The list is sorted
/// ascending, walked BACKWARDS from the top keeping anything more than a tolerance below the last
/// survivor, and reversed at the end. Upstream says why at the site: [`segment_index`] uses
/// `lower_bound`, which needs the bigger value of a group to put a macro's far edge on the right
/// side of it. Walking forwards and keeping the smallest is the natural way to write this and is
/// a different grid.
///
/// ⚠️ **Strictly greater than the tolerance**, so a gap of exactly one tolerance is coalesced.
///
/// ⚠️ The tolerances are INTEGER divisions, so any outline narrower than 100 units gets a
/// tolerance of zero and no coalescing at all.
pub fn notch_grid(macros: &[NotchMacro], outline: (i32, i32)) -> (Vec<i32>, Vec<i32>) {
    let mut x_point = Vec::new();
    let mut y_point = Vec::new();
    for m in macros {
        if !m.obstructs() {
            continue;
        }
        x_point.push(m.x);
        x_point.push(m.x + m.width);
        y_point.push(m.y);
        y_point.push(m.y + m.height);
    }
    x_point.push(0);
    y_point.push(0);
    x_point.push(outline.0);
    y_point.push(outline.1);

    x_point.sort_unstable();
    y_point.sort_unstable();

    (coalesce_downwards(&x_point, outline.0 / 100), coalesce_downwards(&y_point, outline.1 / 100))
}

/// The backwards walk that keeps the largest of each near-coincident group. See [`notch_grid`].
fn coalesce_downwards(sorted: &[i32], epsilon: i32) -> Vec<i32> {
    let mut coords = vec![*sorted.last().expect("the outline's own corners are always present")];
    for i in (0..sorted.len() - 1).rev() {
        if coords[coords.len() - 1] - sorted[i] > epsilon {
            coords.push(sorted[i]);
        }
    }
    coords.reverse();
    coords
}

/// How enclosed a candidate notch is on each of its four sides.
///
/// ⚠️ **Every side starts TRUE and is only ever cleared**, and a side facing the edge of the grid
/// is never tested at all — so the outline's own boundary counts as enclosing. A gap running the
/// full width of the outline is therefore "enclosed left and right", which is what lets a shallow
/// full-width strip be recognised as a notch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotchVicinity {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl Default for NotchVicinity {
    fn default() -> Self {
        Self { top: true, bottom: true, left: true, right: true }
    }
}

impl NotchVicinity {
    /// ⚠️ **Upstream sums four `bool`s as integers**, so this is a count from 0 to 4 and two very
    /// different vicinities can tie.
    pub fn total(&self) -> i32 {
        self.top as i32 + self.bottom as i32 + self.left as i32 + self.right as i32
    }
}

/// Upstream `checkNotchVicinity`: is every cell adjacent to this region occupied, on each side?
///
/// ⚠️ **A side is cleared by the FIRST empty neighbour** — it asks whether the region is walled
/// in, not how much of the wall exists.
pub fn check_notch_vicinity(
    grid: &[Vec<bool>],
    start_row: usize,
    start_col: usize,
    end_row: usize,
    end_col: usize,
) -> NotchVicinity {
    let num_y = grid.len();
    let num_x = grid[0].len();

    let mut vicinity = NotchVicinity::default();
    if start_row > 0 {
        for i in start_col..=end_col {
            if !grid[start_row - 1][i] {
                vicinity.bottom = false;
                break;
            }
        }
    }
    if end_row < num_y - 1 {
        for i in start_col..=end_col {
            if !grid[end_row + 1][i] {
                vicinity.top = false;
                break;
            }
        }
    }
    if start_col > 0 {
        for i in start_row..=end_row {
            if !grid[i][start_col - 1] {
                vicinity.left = false;
                break;
            }
        }
    }
    if end_col < num_x - 1 {
        for i in start_row..=end_row {
            if !grid[i][end_col + 1] {
                vicinity.right = false;
                break;
            }
        }
    }
    vicinity
}

fn is_row_empty(grid: &[Vec<bool>], row: usize, start_col: usize, end_col: usize) -> bool {
    (start_col..=end_col).all(|col| !grid[row][col])
}

fn is_col_empty(grid: &[Vec<bool>], col: usize, start_row: usize, end_row: usize) -> bool {
    (start_row..=end_row).all(|row| !grid[row][col])
}

/// Upstream `calSingleNotchPenalty`: the notch's area as a fraction of the outline's, rooted.
///
/// ⚠️ **The root is what makes it a length-like quantity**, so two notches of half the area each
/// cost more together than one notch of the whole — the term prefers one big gap to two small.
///
/// ⚠️ Computed in `f64` and narrowed once on return. An outline of zero area gives a NaN here,
/// exactly as upstream does; nothing guards it on either side.
pub fn single_notch_penalty(width: i32, height: i32, outline_area: i64) -> f32 {
    ((width as f64 * height as f64) / outline_area as f64).sqrt() as f32
}

/// Upstream `SACoreSoftMacro::calNotchPenalty`.
///
/// 🔑 **A notch is an empty region walled in on both sides of an axis and too thin to be useful.**
/// The scan grids the outline along the obstructing macros' edges, finds every maximal empty
/// rectangle, and charges the ones that are boxed in top-and-bottom while being shallow, or
/// boxed in left-and-right while being narrow.
///
/// ⛔ **The two thresholds are RECOMPUTED here from the outline and the constructor's values are
/// discarded.** Whatever was passed in — `10.0` by default, and no command exposes it — is
/// overwritten by a tenth of the outline's extent before the first comparison.
///
/// ⛔ **And they are CROSSED relative to their names.** The `h` threshold comes from the outline's
/// HEIGHT and is compared against a candidate's HEIGHT; the `v` threshold comes from the WIDTH and
/// is compared against the WIDTH. Reading the names as "horizontal extent" gets both backwards.
///
/// ⚠️ **An invalid floorplan is treated as one huge notch** covering at least the whole outline,
/// so a packing that does not fit is charged roughly `1.0` here rather than being scanned. That is
/// how the term stays meaningful during the early, sprawling part of the search.
///
/// ⚠️ A zero weight returns without measuring — which is why coarse shaping never sees this.
pub fn notch_penalty(
    macros: &[NotchMacro],
    outline: (i32, i32),
    packing: (i32, i32),
    valid: bool,
    weight: f32,
) -> f32 {
    if weight <= 0.0 {
        return 0.0;
    }

    let outline_area = outline.0 as i64 * outline.1 as i64;

    // ⛔ Both thresholds, overwritten from the outline; see above for why they read crossed.
    let notch_h_th = outline.1 / 10;
    let notch_v_th = outline.0 / 10;

    if !valid {
        let width = packing.0.max(outline.0);
        let height = packing.1.max(outline.1);
        return single_notch_penalty(width, height, outline_area);
    }

    let (x_coords, y_coords) = notch_grid(macros, outline);
    let num_x = x_coords.len() - 1;
    let num_y = y_coords.len() - 1;
    // ⛔ **A guard the reference does not have** — divergence class B. An outline with a zero
    // extent coalesces its whole axis to a single grid line, and upstream then builds a grid with
    // no cells and reads `grid.front()` out of it. Its scan loops never run, so the value it
    // would return is this one; the difference is only that ours is defined. Nothing reaches it:
    // a zero-extent outline is refused long before placement.
    if num_x == 0 || num_y == 0 {
        return 0.0;
    }

    let mut grid = vec![vec![false; num_x]; num_y];
    for m in macros {
        if !m.obstructs() {
            continue;
        }
        let x_start = segment_index(m.x, &x_coords);
        let x_end = segment_index(m.x + m.width, &x_coords);
        let y_start = segment_index(m.y, &y_coords);
        let y_end = segment_index(m.y + m.height, &y_coords);
        for row in y_start..y_end {
            for col in x_start..x_end {
                grid[row][col] = true;
            }
        }
    }

    let mut visited = vec![vec![false; num_x]; num_y];
    let mut penalty = 0.0f32;

    for start_row in 0..num_y {
        for start_col in 0..num_x {
            if grid[start_row][start_col] || visited[start_row][start_col] {
                continue;
            }

            let mut end_row = start_row;
            let mut end_col = start_col;

            let mut current = check_notch_vicinity(&grid, start_row, start_col, end_row, end_col);
            let mut expand_rows = true;
            let mut expand_cols = true;

            // ⚠️ **Rows are tried before columns in every pass**, and the column test then runs
            // against the row range this same pass just changed. Swapping the two grows a
            // different rectangle out of the same seed.
            while expand_rows || expand_cols {
                if expand_rows {
                    end_row += 1;
                    if end_row < num_y && is_row_empty(&grid, end_row, start_col, end_col) {
                        let expanded =
                            check_notch_vicinity(&grid, start_row, start_col, end_row, end_col);
                        // ⚠️ Equal-but-different is REJECTED: only a strictly better total, or
                        // the very same four flags, is accepted.
                        if expanded.total() > current.total() || expanded == current {
                            current = expanded;
                        } else {
                            expand_rows = false;
                            end_row -= 1;
                        }
                    } else {
                        expand_rows = false;
                        end_row -= 1;
                    }
                }

                if expand_cols {
                    end_col += 1;
                    if end_col < num_x && is_col_empty(&grid, end_col, start_row, end_row) {
                        let expanded =
                            check_notch_vicinity(&grid, start_row, start_col, end_row, end_col);
                        if expanded.total() > current.total() || expanded == current {
                            current = expanded;
                        } else {
                            expand_cols = false;
                            end_col -= 1;
                        }
                    } else {
                        expand_cols = false;
                        end_col -= 1;
                    }
                }
            }

            for row in visited.iter_mut().take(end_row + 1).skip(start_row) {
                for cell in row.iter_mut().take(end_col + 1).skip(start_col) {
                    *cell = true;
                }
            }

            let width = x_coords[end_col + 1] - x_coords[start_col];
            let height = y_coords[end_row + 1] - y_coords[start_row];

            let mut is_notch = false;
            if current.top && current.bottom && height < notch_h_th {
                is_notch = true;
            }
            if current.left && current.right && width < notch_v_th {
                is_notch = true;
            }

            if is_notch {
                penalty += single_notch_penalty(width, height, outline_area);
            }
        }
    }

    penalty
}

// ---------------------------------------------------------------- blockages into the outline

/// Upstream `findOffsetIntersections`: the parts of each blockage that fall inside the outline,
/// re-expressed relative to the outline's corner.
///
/// 🔑 **Everything inside a placement problem is outline-relative.** The parent's outline becomes
/// the origin, so a blockage at absolute `(500, 500)` inside an outline starting at `(400, 400)`
/// arrives as `(100, 100)`.
///
/// ⚠️ **A zero-AREA intersection is dropped, not kept as a degenerate box.** A blockage touching
/// the outline edge-on contributes nothing, so the test is on the area rather than on the
/// individual dimensions — which also rejects the inverted rectangle a complete miss produces.
pub fn find_offset_intersections(
    candidates: &[(i32, i32, i32, i32)],
    outline: (i32, i32, i32, i32),
) -> Vec<(i32, i32, i32, i32)> {
    let mut out = Vec::new();
    for &(cx0, cy0, cx1, cy1) in candidates {
        let x0 = cx0.max(outline.0);
        let y0 = cy0.max(outline.1);
        let x1 = cx1.min(outline.2);
        let y1 = cy1.min(outline.3);
        // `isInverted()`, then a zero area — a miss gives one or both, a touch gives the second.
        if x0 > x1 || y0 > y1 {
            continue;
        }
        if (x1 - x0) as i64 * (y1 - y0) as i64 == 0 {
            continue;
        }
        out.push((x0 - outline.0, y0 - outline.1, x1 - outline.0, y1 - outline.1));
    }
    out
}

/// Why a blockage set could not be reduced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedsPolygonUnion;

/// Upstream `eliminateOverlaps`: merge the blockages and re-cut them into disjoint rectangles.
///
/// ⛔ **Upstream does this with `boost::polygon`'s `polygon_90_set_data`, and the rectangles that
/// come back are that library's DECOMPOSITION, not a canonical answer.** How a merged region is
/// sliced — how many rectangles, where the cuts fall, what order they arrive in — is boost's
/// choice. Reimplementing it by eye would produce a plausible set that is not the same set, which
/// is the one thing this programme never does.
///
/// 🔑 **Zero or one rectangle needs no library.** A set built from a single rectangle decomposes
/// back to that rectangle, so those cases are exact. Every design in the reference suite that has
/// a placement blockage at all has exactly one, so this covers the corpus.
///
/// ⛔ Two or more is REFUSED by name. Note that it is refused even when they do not overlap:
/// boost merges rectangles that merely touch, so "no overlap" is not the same as "no work".
pub fn eliminate_overlaps(
    blockages: &[(i32, i32, i32, i32)],
) -> Result<Vec<(i32, i32, i32, i32)>, NeedsPolygonUnion> {
    match blockages.len() {
        0 => Ok(Vec::new()),
        1 => Ok(blockages.to_vec()),
        _ => Err(NeedsPolygonUnion),
    }
}

/// Upstream `createSoftMacrosForBlockages`: each blockage becomes a fixed, zero-cluster macro.
///
/// ⚠️ **They are appended BEFORE any cluster**, so blockage macros occupy the lowest ids and every
/// cluster's id is offset by their count. The sequence pair indexes into this list.
pub fn soft_macros_for_blockages(blockages: &[(i32, i32, i32, i32)]) -> Vec<crate::anneal::SoftMacro> {
    blockages
        .iter()
        .map(|&(x0, y0, x1, y1)| crate::anneal::SoftMacro {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
            fixed: true,
            area: (x1 - x0) as i64 * (y1 - y0) as i64,
            is_macro_cluster: false,
        })
        .collect()
}

// ---------------------------------------------------------------- fixed terminals

/// A cluster, as the fixed-terminal builder needs to see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCluster {
    pub center: (i32, i32),
    pub origin: (i32, i32),
    pub width: i32,
    pub height: i32,
    /// ⚠️ Decides which of two quite different things this becomes.
    pub is_cluster_of_unplaced_io_pins: bool,
}

/// Upstream `createFixedTerminal`.
///
/// 🔑 **Two shapes in one function.** An ordinary terminal is a POINT at the cluster's CENTRE with
/// no size and no cluster attached — it exists only to pull wirelength toward where that cluster
/// sits. A cluster of unplaced IO pins is different: it keeps its real ORIGIN, its real extent and
/// its cluster identity, because the wirelength model measures distance to the region it covers.
///
/// ⚠️ **Centre for one, origin for the other** — not a uniform anchor.
///
/// ⚠️ **The area is ZERO either way**, even for the sized one. Upstream's comment says so
/// explicitly: the area is what distinguishes a terminal from a placeable macro inside the
/// annealer, so giving the IO region its real area would make it placeable.
pub fn fixed_terminal(cluster: &TerminalCluster, outline_origin: (i32, i32)) -> crate::anneal::SoftMacro {
    let (location, width, height) = if cluster.is_cluster_of_unplaced_io_pins {
        (cluster.origin, cluster.width, cluster.height)
    } else {
        (cluster.center, 0, 0)
    };
    crate::anneal::SoftMacro {
        x: location.0 - outline_origin.0,
        y: location.1 - outline_origin.1,
        width,
        height,
        fixed: true,
        // ⚠️ Zero on purpose — see above.
        area: 0,
        is_macro_cluster: false,
    }
}

/// Upstream `createFixedTerminals`' walk: every SIBLING at every level above the cluster.
///
/// 🔑 **It climbs the tree, gathering aunts and uncles.** Placing a cluster's children means
/// knowing where everything else in the design already sits, so at each level it takes the
/// current node's siblings and then steps up. The node itself is always excluded.
///
/// ⛔ **It stops when the grandparent runs out, not the parent** — so the ROOT's own children are
/// never added as terminals, and a cluster whose parent is the root gets its siblings and nothing
/// more.
///
/// ⚠️ Returns nothing at all for a cluster with no parent.
pub fn fixed_terminal_walk(
    start: usize,
    parent_of: &dyn Fn(usize) -> Option<usize>,
    children_of: &dyn Fn(usize) -> Vec<usize>,
) -> Vec<usize> {
    let mut out = Vec::new();
    if parent_of(start).is_none() {
        return out;
    }
    let mut frontier = std::collections::VecDeque::new();
    frontier.push_back(start);

    while let Some(node) = frontier.pop_front() {
        let Some(grandparent) = parent_of(node) else { continue };
        for sibling in children_of(grandparent) {
            if sibling != node {
                out.push(sibling);
            }
        }
        // ⛔ The GRANDparent's own parent is the test — climbing stops one level below the root.
        if let Some(parent) = parent_of(node) {
            if parent_of(parent).is_some() {
                frontier.push_back(parent);
            }
        }
    }
    out
}

// ---------------------------------------------------------------- nets, fences and guides

/// Upstream `mergeNets`: collapse duplicate nets, summing their weights.
///
/// ⛔ **The match is DIRECTED, despite the nets being documented as undirected.** Upstream's
/// `operator==` compares `first` to `first` and `second` to `second`, so `(a, b)` and `(b, a)`
/// are NOT duplicates and both survive as separate nets, each with its own weight. Treating the
/// pair as unordered here would merge nets the reference keeps apart, halving their count and
/// doubling a weight.
///
/// ⚠️ **The survivor keeps the FIRST occurrence's position**, and the weights are added in index
/// order — a float sum, so the order is part of the answer.
pub fn merge_nets(nets: &[BundledNet]) -> Vec<BundledNet> {
    let mut class = vec![usize::MAX; nets.len()];
    for i in 0..nets.len() {
        if class[i] == usize::MAX {
            for j in (i + 1)..nets.len() {
                // ⛔ Directed: both terminals in the same roles.
                if nets[i].source == nets[j].source && nets[i].target == nets[j].target {
                    class[j] = i;
                }
            }
        }
    }

    let mut merged: Vec<BundledNet> = nets.to_vec();
    for i in 0..class.len() {
        if class[i] != usize::MAX {
            let weight = merged[i].weight;
            merged[class[i]].weight += weight;
        }
    }
    (0..class.len()).filter(|&i| class[i] == usize::MAX).map(|i| merged[i]).collect()
}

/// The merge of every fence or guide belonging to a cluster's hard macros, clipped to the outline.
///
/// 🔑 **A cluster inherits the UNION of its macros' regions, not each one separately.** Once
/// macros are grouped into a cluster the annealer places the cluster, so the constraint has to be
/// one box — and merging boxes that were far apart yields a region much larger than either.
///
/// ⚠️ **Dropped when the clipped area is zero**, so a region falling outside the outline
/// constrains nothing rather than constraining everything.
///
/// ⚠️ Returned relative to the outline's corner, like every other coordinate in the stage.
pub fn merged_region(
    regions: &[(i32, i32, i32, i32)],
    outline: (i32, i32, i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    let mut merged: Option<(i32, i32, i32, i32)> = None;
    for &r in regions {
        merged = Some(match merged {
            None => r,
            Some(m) => (m.0.min(r.0), m.1.min(r.1), m.2.max(r.2), m.3.max(r.3)),
        });
    }
    let m = merged?;
    let clipped = (m.0.max(outline.0), m.1.max(outline.1), m.2.min(outline.2), m.3.min(outline.3));
    if clipped.0 > clipped.2 || clipped.1 > clipped.3 {
        return None;
    }
    if (clipped.2 - clipped.0) as i64 * (clipped.3 - clipped.1) as i64 <= 0 {
        return None;
    }
    Some((
        clipped.0 - outline.0,
        clipped.1 - outline.1,
        clipped.2 - outline.0,
        clipped.3 - outline.1,
    ))
}

// ---------------------------------------------------------------- utilization

/// What each soft macro contributes to the utilization sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaKind {
    /// A blockage — no cluster behind it. ⚠️ Counted by its PHYSICAL area.
    Blockage,
    /// ⚠️ Skipped entirely.
    IoCluster,
    /// ⚠️ Counted by the SOFT MACRO's area, not the cluster's, so a fixed macro only partly inside
    /// the outline contributes only the part that is inside.
    FixedMacro,
    StdCellCluster,
    HardMacroCluster,
    MixedCluster,
}

/// One entry in the utilization sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreaContribution {
    pub kind: AreaKind,
    /// The soft macro's own area — used for blockages and fixed macros.
    pub soft_macro_area: i64,
    pub cluster_std_cell_area: i64,
    /// ⚠️ The FIRST tiling's area, for the two kinds that have tilings.
    pub first_tiling_area: i64,
}

/// Upstream `validUtilization`: does the soft area still fit once the hard area is taken out?
///
/// 🔑 **"Hard" and "soft" here are about compressibility, not about macros.** Blockages, macro
/// clusters and a mixed cluster's macro half occupy space that cannot be squeezed; standard cells
/// can be packed tighter, which is what the utilization figure expresses.
///
/// ⚠️ **A fixed macro is measured by its SOFT MACRO's area**, which is the clipped one — so a
/// fixed macro hanging half outside the outline only charges for the half inside. Every other
/// kind is measured from the cluster.
///
/// ⚠️ **`soft_area / utilization` is an int64 divided by a float and truncated back to an
/// integer**, and the comparison is a strict `<`.
pub fn valid_utilization(
    contributions: &[AreaContribution],
    outline_area: i64,
    utilization: f32,
) -> bool {
    let mut blocked = 0i64;
    let mut std_cell = 0i64;
    let mut mixed_std_cell = 0i64;
    let mut macro_cluster = 0i64;
    let mut mixed_macro = 0i64;

    for c in contributions {
        match c.kind {
            AreaKind::Blockage => blocked += c.soft_macro_area,
            AreaKind::IoCluster => continue,
            AreaKind::FixedMacro => macro_cluster += c.soft_macro_area,
            AreaKind::StdCellCluster => std_cell += c.cluster_std_cell_area,
            AreaKind::HardMacroCluster => macro_cluster += c.first_tiling_area,
            AreaKind::MixedCluster => {
                mixed_macro += c.first_tiling_area;
                mixed_std_cell += c.cluster_std_cell_area;
            }
        }
    }

    let hard_area = blocked + macro_cluster + mixed_macro;
    let available_area = outline_area - hard_area;
    let soft_area = std_cell + mixed_std_cell;
    // ⚠️ Truncated on the way back to an integer.
    let inflated_soft_area = (soft_area as f32 / utilization) as i64;
    inflated_soft_area < available_area
}

/// Upstream `setMacroClustersShapes`: give each movable macro cluster its tilings as shapes.
///
/// ⚠️ **This is `setShapes` WITHOUT the force flag**, so it declines an empty tiling list — unlike
/// the shaping stage, which forces the same call. A macro cluster that was never shaped therefore
/// arrives at placement with no shape curve and cannot be resized.
///
/// ⚠️ **A FIXED macro cluster is skipped**, because its shape is its position.
///
/// ⛔ The reference's `setShapes` APPENDS to the interval lists rather than clearing them, so
/// calling it twice on one macro doubles its shapes. Each call site builds its macros fresh.
pub fn set_macro_cluster_shapes(
    is_macro_cluster: bool,
    is_fixed: bool,
    tilings: &[(i32, i32)],
) -> Option<(crate::anneal::ShapeCurve, i32, i32, i64)> {
    if !is_macro_cluster || is_fixed || tilings.is_empty() {
        return None;
    }
    Some(crate::anneal::shape_curve_from_tilings(tilings))
}

// ---------------------------------------------------------------- fine shaping

/// Upstream `singleArraySingleStdCellCluster`.
///
/// 🔑 **Exactly one macro array and exactly one standard-cell cluster, and nothing else.** That
/// shape of design gets special treatment below: the cell cluster is shrunk to nothing so the
/// array can use the whole outline.
///
/// ⛔ **Any mixed cluster fails it outright**, before anything is counted.
/// ⛔ A macro cluster that is not an ARRAY of interconnected macros also fails it.
pub fn single_array_single_std_cell_cluster(
    entries: &[(Option<crate::cluster::ClusterType>, bool)],
) -> bool {
    use crate::cluster::ClusterType;
    let mut arrays = 0;
    let mut std_clusters = 0;
    for &(cluster_type, is_array_of_interconnected_macros) in entries {
        match cluster_type {
            // ⛔ `isMixedCluster()` — and it is TRUE for an IO cluster, whose `type_` is the
            // member's `MixedCluster` default. So this is the line an IO cluster leaves by, not
            // the `isIOCluster()` skip below it.
            Some(ClusterType::Mixed) => return false,
            // ⛔ `!cluster` — a blockage, and a conventional fixed terminal, are built with a null
            // cluster. ⚠️ The reference's `|| cluster->isIOCluster()` half of this test is DEAD:
            // an IO cluster has already returned above.
            None => continue,
            Some(ClusterType::HardMacro) => {
                if !is_array_of_interconnected_macros {
                    return false;
                }
                arrays += 1;
            }
            Some(ClusterType::StdCell) => std_clusters += 1,
        }
        if arrays > 1 || std_clusters > 1 {
            return false;
        }
    }
    arrays != 0 && std_clusters != 0
}

/// The shape a standard-cell cluster is given at fine shaping.
///
/// 🔑 **A cluster small enough to be "tiny" is collapsed to ONE DATABASE UNIT SQUARE.** Not
/// shrunk — erased. The same happens to the lone cell cluster of a single-array design. It still
/// exists, so the netlist still pulls on it, but it occupies nothing and the macros place as if
/// it were not there.
///
/// ⚠️ Otherwise the area is inflated by the utilization and the width comes from
/// `sqrt(area / min_ar)`, which is the widest the cluster may be at its aspect-ratio limit. The
/// interval then runs from `area / width` up to that width.
///
/// ⚠️ Every division truncates on the way back to an integer, and the square root truncates too.
pub fn std_cell_cluster_shape(
    cluster_area: i64,
    num_std_cell: i32,
    tiny_threshold: i32,
    single_array_single_std_cell: bool,
    utilization: f32,
    min_ar: f32,
) -> (crate::anneal::Interval, i64) {
    if num_std_cell <= tiny_threshold || single_array_single_std_cell {
        // ⚠️ One unit square, not zero — a zero area would make it a fixed terminal.
        const NEGLIGIBLE_WIDTH: i32 = 1;
        return (
            crate::anneal::Interval { min: NEGLIGIBLE_WIDTH, max: NEGLIGIBLE_WIDTH },
            NEGLIGIBLE_WIDTH as i64 * NEGLIGIBLE_WIDTH as i64,
        );
    }
    let area = (cluster_area as f32 / utilization) as i64;
    let width = (area as f32 / min_ar).sqrt() as i32;
    let minimum_width = if width != 0 { (area / width as i64) as i32 } else { 0 };
    (crate::anneal::Interval { min: minimum_width, max: width }, area)
}

/// The shape curve a mixed cluster is given at fine shaping.
///
/// ⛔ **The macro area comes from the LAST tiling, not the first.** The tilings are ordered by
/// area, so the last is the LARGEST — the inflation is sized against the worst case the cluster
/// might take, and using `front()` would under-inflate every mixed cluster.
///
/// 🔑 **Only the standard-cell half is inflated.** Macros do not compress, so the utilization is
/// applied to the cell area alone and the macro area is added back at full size.
///
/// ⚠️ One interval per tiling: from that tiling's own width up to whatever width the inflated area
/// allows at that tiling's height. So a tall thin tiling permits a wide range and a short wide one
/// permits almost none.
pub fn mixed_cluster_shape(
    tilings: &[(i32, i32)],
    cluster_std_cell_area: i64,
    utilization: f32,
) -> Option<(Vec<crate::anneal::Interval>, i64)> {
    let macro_area = tilings.last()?.0 as i64 * tilings.last()?.1 as i64;
    let inflated_area =
        (macro_area as f32 + cluster_std_cell_area as f32 / utilization) as i64;
    let intervals = tilings
        .iter()
        .map(|&(width, height)| crate::anneal::Interval {
            min: width,
            max: if height != 0 { (inflated_area / height as i64) as i32 } else { 0 },
        })
        .collect();
    Some((intervals, inflated_area))
}

// ---------------------------------------------------------------- dead space

/// Upstream `getSegmentIndex`: where a coordinate falls in the grid's edge list.
///
/// ⚠️ **`lower_bound`, so a coordinate that IS an edge returns that edge's own index**, not the
/// cell before it. That is what makes a macro's `[start, end)` span cover exactly its own cells.
pub fn segment_index(coordinate: i32, coords: &[i32]) -> usize {
    coords.partition_point(|&c| c < coordinate)
}

/// A macro as the dead-space filler sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadSpaceMacro {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub area: i64,
    pub is_mixed_cluster: bool,
    pub is_std_cell_cluster: bool,
}

/// The grid edges the filler cuts the outline along.
///
/// 🔑 **Every macro edge becomes a grid line**, plus the outline's own two corners — so the grid is
/// exactly fine enough to describe any arrangement of these macros and no finer.
///
/// ⚠️ **A zero-AREA macro contributes no edges.** Fixed terminals and IO clusters carry zero area
/// by construction, so they neither cut the grid nor occupy it — the filler expands straight
/// through them.
pub fn dead_space_grid(macros: &[DeadSpaceMacro], outline: (i32, i32)) -> (Vec<i32>, Vec<i32>) {
    let mut xs = std::collections::BTreeSet::new();
    let mut ys = std::collections::BTreeSet::new();
    for m in macros {
        if m.area == 0 {
            continue;
        }
        xs.insert(m.x);
        xs.insert(m.x + m.width);
        ys.insert(m.y);
        ys.insert(m.y + m.height);
    }
    xs.insert(0);
    ys.insert(0);
    xs.insert(outline.0);
    ys.insert(outline.1);
    (xs.into_iter().collect(), ys.into_iter().collect())
}

/// Upstream `fillDeadSpace`: grow the soft clusters into whatever space nobody claimed.
///
/// 🔑 **Mixed clusters first, then standard-cell clusters — two full passes.** The order is the
/// algorithm: whatever a mixed cluster takes in the first pass is no longer available to a cell
/// cluster in the second, so swapping them redistributes the empty space differently.
///
/// ⛔ **Within one cluster the directions are LEFT, TOP, RIGHT, then DOWN, and each uses the
/// bounds the previous one just widened.** Growing left first means the top pass sweeps a wider
/// span, which can block on something the original span would have cleared. The order compounds;
/// it is not four independent expansions.
///
/// ⚠️ Expansion stops at the FIRST occupied column or row — it does not skip over an obstacle to
/// take free space beyond it.
///
/// ⚠️ The final shape is assigned with the forcing setters, so it ignores the cluster's shape
/// curve entirely: a cluster can end up a shape its own curve does not offer.
pub fn fill_dead_space(macros: &mut [DeadSpaceMacro], outline: (i32, i32)) {
    let (x_grid, y_grid) = dead_space_grid(macros, outline);
    let (num_x, num_y) = (x_grid.len() - 1, y_grid.len() - 1);
    if num_x == 0 || num_y == 0 {
        return;
    }

    // -1 means unclaimed.
    let mut grid = vec![vec![-1i64; num_x]; num_y];
    let span = |m: &DeadSpaceMacro| {
        (
            segment_index(m.x, &x_grid),
            segment_index(m.x + m.width, &x_grid),
            segment_index(m.y, &y_grid),
            segment_index(m.y + m.height, &y_grid),
        )
    };

    for (id, m) in macros.iter().enumerate() {
        if m.area == 0 {
            continue;
        }
        let (x0, x1, y0, y1) = span(m);
        for row in grid.iter_mut().take(y1).skip(y0) {
            for cell in row.iter_mut().take(x1).skip(x0) {
                *cell = id as i64;
            }
        }
    }

    for order in 0..=1 {
        for id in 0..macros.len() {
            if macros[id].area == 0 {
                continue;
            }
            let wanted = if order == 0 {
                macros[id].is_mixed_cluster
            } else {
                macros[id].is_std_cell_cluster
            };
            if !wanted {
                continue;
            }

            let (mut x_start, mut x_end, mut y_start, mut y_end) = span(&macros[id]);
            let me = id as i64;

            // ⛔ Left first, and the widened `x_start` is what the top pass then sweeps.
            for i in (0..x_start).rev() {
                if (y_start..y_end).any(|j| grid[j][i] != -1) {
                    break;
                }
                x_start = i;
                for j in y_start..y_end {
                    grid[j][i] = me;
                }
            }
            // Top second.
            for j in y_end..num_y {
                if (x_start..x_end).any(|i| grid[j][i] != -1) {
                    break;
                }
                y_end = j + 1;
                for i in x_start..x_end {
                    grid[j][i] = me;
                }
            }
            // Right third.
            for i in x_end..num_x {
                if (y_start..y_end).any(|j| grid[j][i] != -1) {
                    break;
                }
                x_end = i + 1;
                for j in y_start..y_end {
                    grid[j][i] = me;
                }
            }
            // Down last.
            for j in (0..y_start).rev() {
                if (x_start..x_end).any(|i| grid[j][i] != -1) {
                    break;
                }
                y_start = j;
                for i in x_start..x_end {
                    grid[j][i] = me;
                }
            }

            // ⚠️ `setLocationF` then `setShapeF` — and **`setShapeF` RECOMPUTES THE AREA**
            // (`width * static_cast<int64_t>(height)`, widened before the multiply, so unlike the
            // fence term's zero test this one cannot wrap). Leaving `area` at its pre-fill value
            // is invisible inside this function — it is read only by the zero-area skip, which a
            // grown cluster passes either way — and wrong for every consumer downstream, starting
            // with the Area row of the placement summary.
            macros[id].x = x_grid[x_start];
            macros[id].y = y_grid[y_start];
            macros[id].width = x_grid[x_end] - x_grid[x_start];
            macros[id].height = y_grid[y_end] - y_grid[y_start];
            macros[id].area = macros[id].width as i64 * macros[id].height as i64;
        }
    }
}

/// Upstream `best_sa->fillDeadSpace()`, as `hier_rtlmp` calls it on the WINNING run.
///
/// ⛔ **The validity guard is upstream's FIRST line and belongs here, not in the filler.**
/// `fillDeadSpace` returns immediately on an invalid floorplan, so a solution that overflows its
/// outline — or that overlaps a fixed macro — keeps the geometry the annealer left it with.
/// [`fill_dead_space`] is pure geometry and cannot ask; the caller must.
///
/// 🔑 **Only MIXED and STANDARD-CELL clusters grow.** Everything else — hard-macro clusters, fixed
/// macros, blockage proxies, IO clusters — is scenery the two passes read and never move.
///
/// ℹ️ **`setShapeF`'s `if (fixed_) return;` is not modelled**, deliberately: the passes touch only
/// mixed and cell clusters, and a soft macro that is either of those is never fixed. ⚠️ Note the
/// asymmetry it guards, in case a later stage does reach it — `setLocationF` has NO such test, so
/// upstream would MOVE a fixed macro while refusing to RESIZE it.
pub fn fill_dead_space_on_solution(
    macros: &mut [crate::anneal::SoftMacro],
    kinds: &[Option<AreaKind>],
    outline: (i32, i32),
    is_valid: bool,
) {
    if !is_valid {
        return;
    }
    let mut cells: Vec<DeadSpaceMacro> = macros
        .iter()
        .enumerate()
        .map(|(i, m)| DeadSpaceMacro {
            x: m.x,
            y: m.y,
            width: m.width,
            height: m.height,
            area: m.area,
            is_mixed_cluster: kinds.get(i).copied().flatten() == Some(AreaKind::MixedCluster),
            is_std_cell_cluster: kinds.get(i).copied().flatten() == Some(AreaKind::StdCellCluster),
        })
        .collect();
    fill_dead_space(&mut cells, outline);
    for (m, c) in macros.iter_mut().zip(&cells) {
        m.x = c.x;
        m.y = c.y;
        m.width = c.width;
        m.height = c.height;
        m.area = c.area;
    }
}

// ---------------------------------------------------------------- the post-anneal enhancements

/// What the two post-anneal enhancements need from the annealer they run on.
///
/// ⚠️ **A structural divergence, class E.** Upstream reads these straight off `SACoreSoftMacro`'s
/// members; here they are a seam so the two enhancements can be transcribed and pinned before the
/// placement core exists. Nothing about the control flow below differs — but the coupling the
/// members hid is now spelled out, and [`Enhancements::notch_thresholds`] is the one that matters:
/// the alignment step reads thresholds that [`notch_penalty`] silently rewrote.
pub trait Enhancements {
    fn macros(&self) -> &[crate::anneal::SoftMacro];
    fn macros_mut(&mut self) -> &mut [crate::anneal::SoftMacro];
    /// The sequence pair's positive order — ⚠️ **not every macro.** The IO clusters and fixed
    /// terminals appended after the clusters are outside it, so nothing below ever moves them.
    fn order(&self) -> &[usize];
    fn outline(&self) -> (i32, i32);
    /// The current packing's `(width, height)`.
    fn packing(&self) -> (i32, i32);
    fn outline_penalty(&self) -> f32;
    fn is_valid(&self) -> bool;
    /// ⛔ **The thresholds `calNotchPenalty` leaves behind**, which is not the same as the ones the
    /// constructor was given — see [`notch_penalty`]. A run with the notch term switched off
    /// reaches the alignment step with the constructor's values still in place.
    fn notch_thresholds(&self) -> (i32, i32);
    fn cal_penalty(&mut self);
    fn norm_cost(&self) -> f32;
}

/// Upstream `getClustersLocations`.
///
/// ⚠️ **Indexed by MACRO ID, not by position in the sequence.** The sequence is a permutation of
/// `0..n`, so the two are the same set of indices — but writing it positionally would scramble
/// every location the moment the annealer swapped anything.
pub fn cluster_locations(macros: &[crate::anneal::SoftMacro], order: &[usize]) -> Vec<(i32, i32)> {
    let mut locations = vec![(0, 0); order.len()];
    for &id in order {
        locations[id] = (macros[id].x, macros[id].y);
    }
    locations
}

/// A `setClustersLocations` call whose list does not match the sequence pair — upstream MPL-52.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterCountMismatch;

/// Upstream `setClustersLocations`.
pub fn set_cluster_locations(
    macros: &mut [crate::anneal::SoftMacro],
    order: &[usize],
    locations: &[(i32, i32)],
) -> Result<(), ClusterCountMismatch> {
    if locations.len() != order.len() {
        return Err(ClusterCountMismatch);
    }
    for &id in order {
        // ⚠️ Through the setters, like upstream — a fixed macro was never moved, so putting it
        // back is a no-op either way, but the guard belongs where the reference has it.
        macros[id].set_x(locations[id].0);
        macros[id].set_y(locations[id].1);
    }
    Ok(())
}

/// Upstream `moveFloorplan`: shift every macro in the sequence pair by one offset.
///
/// ⛔ **CORRECTED — a FIXED macro does NOT move.** This said the opposite, on the grounds that
/// `moveFloorplan` has no `isFixed` test. It does not need one: it assigns through `setX`/`setY`,
/// and the guard is in those. So centralizing shifts everything the packer could place and leaves
/// the fixed macros and blockage proxies exactly where they were.
///
/// ⚠️ Getting this wrong moved a FIXED macro during the anneal — its position is an input, so the
/// placement was scored against a floorplan the design does not have.
pub fn move_floorplan(
    macros: &mut [crate::anneal::SoftMacro],
    order: &[usize],
    offset: (i32, i32),
) {
    for &id in order {
        let (x, y) = (macros[id].x + offset.0, macros[id].y + offset.1);
        macros[id].set_x(x);
        macros[id].set_y(y);
    }
}

/// Upstream `attemptCentralization`'s offset: half the slack on each axis.
///
/// ⚠️ **Integer division, truncating** — an odd amount of slack leaves the extra unit at the top
/// and right.
pub fn centralization_offset(outline: (i32, i32), packing: (i32, i32)) -> (i32, i32) {
    ((outline.0 - packing.0) / 2, (outline.1 - packing.1) / 2)
}

/// Upstream `SACoreSoftMacro::attemptCentralization`. Returns whether it was **reverted**.
///
/// 🔑 **The return value is the gate on the next enhancement.** Upstream records it in
/// `centralization_was_reverted_` and runs [`attempt_macro_cluster_alignment`] only when it is
/// set — so alignment is what happens *because* centralizing made the cost worse, never as a
/// second improvement on top of a centralization that stuck.
///
/// ⛔ **An early return is NOT a revert.** A floorplan that overflows its outline returns without
/// setting the flag, so it gets neither enhancement.
///
/// ⚠️ **`> pre_cost`, strictly** — an exactly equal cost keeps the centralized floorplan.
///
/// ⚠️ **Forcing skips the revert but still runs the second `calPenalty`-free path**: with
/// `force` set the new penalties stand whatever they cost.
pub fn attempt_centralization<S: Enhancements + ?Sized>(
    state: &mut S,
    pre_cost: f32,
    force: bool,
) -> bool {
    if state.outline_penalty() > 0.0 {
        return false;
    }

    // Cached rather than recomputed: reverting by re-packing would re-derive the coordinates
    // through the dead-space grid's floating point, and upstream says so at the site.
    let saved = cluster_locations(state.macros(), state.order());

    let offset = centralization_offset(state.outline(), state.packing());
    let order = state.order().to_vec();
    move_floorplan(state.macros_mut(), &order, offset);
    state.cal_penalty();

    if state.norm_cost() > pre_cost && !force {
        let _ = set_cluster_locations(state.macros_mut(), &order, &saved);
        state.cal_penalty();
        return true;
    }
    false
}

/// Upstream `attemptMacroClusterAlignment`'s two thresholds.
///
/// ⛔ **Crossed relative to their names, exactly as in [`notch_penalty`].** The `h` threshold is
/// the one an X coordinate is tested against, and it is floored by a tenth of the outline's
/// HEIGHT; the `v` threshold governs Y and is floored by a tenth of the WIDTH.
///
/// 🔑 **Also floored by the smallest macro cluster on the board** — its width for `h`, its height
/// for `v`. A design with one thin macro cluster therefore aligns nothing else either.
///
/// ⚠️ **The starting values come from the notch thresholds**, which [`notch_penalty`] overwrites
/// from the outline whenever it runs. With the notch term dark they are still the constructor's,
/// which is `10` database units — small enough that nothing aligns at all.
pub fn alignment_thresholds<'a>(
    macro_clusters: impl Iterator<Item = &'a crate::anneal::SoftMacro>,
    outline: (i32, i32),
    notch_thresholds: (i32, i32),
) -> (i32, i32) {
    let (mut h, mut v) = notch_thresholds;
    for m in macro_clusters {
        h = h.min(m.width);
        v = v.min(m.height);
    }
    const RATIO: i32 = 10;
    (h.min(outline.1 / RATIO), v.min(outline.0 / RATIO))
}

/// The snap itself: every macro cluster within a threshold of an edge is pushed onto it.
///
/// ⚠️ **`else if`, so the left test wins over the right one.** A macro cluster wider than the
/// outline satisfies both and is snapped LEFT.
///
/// ⚠️ **Strictly less than the threshold**, on both the near coordinate and the far gap.
pub fn align_macro_clusters(
    macros: &mut [crate::anneal::SoftMacro],
    order: &[usize],
    outline: (i32, i32),
    thresholds: (i32, i32),
) {
    let (h_th, v_th) = thresholds;
    for &id in order {
        if !macros[id].is_macro_cluster {
            continue;
        }
        let (lx, ly) = (macros[id].x, macros[id].y);
        let (ux, uy) = (lx + macros[id].width, ly + macros[id].height);
        // ⛔ X is governed by the `h` threshold, Y by the `v` one. See `alignment_thresholds`.
        if lx < h_th {
            macros[id].set_x(0);
        } else if outline.0 - ux < h_th {
            let x = outline.0 - macros[id].width;
            macros[id].set_x(x);
        }
        if ly < v_th {
            macros[id].set_y(0);
        } else if outline.1 - uy < v_th {
            let y = outline.1 - macros[id].height;
            macros[id].set_y(y);
        }
    }
}

/// Upstream `SACoreSoftMacro::attemptMacroClusterAlignment`. Returns whether it was **reverted**.
///
/// ⛔ **No force override here**, unlike centralization: an alignment that costs more is always
/// undone.
///
/// ⚠️ **An invalid floorplan is left alone** — this runs only on a solution worth polishing.
pub fn attempt_macro_cluster_alignment<S: Enhancements + ?Sized>(state: &mut S) -> bool {
    if !state.is_valid() {
        return false;
    }

    let pre_cost = state.norm_cost();
    let order = state.order().to_vec();
    let saved = cluster_locations(state.macros(), &order);

    let outline = state.outline();
    let thresholds = alignment_thresholds(
        order.iter().map(|&id| &state.macros()[id]).filter(|m| m.is_macro_cluster),
        outline,
        state.notch_thresholds(),
    );

    align_macro_clusters(state.macros_mut(), &order, outline, thresholds);
    state.cal_penalty();

    if state.norm_cost() > pre_cost {
        let _ = set_cluster_locations(state.macros_mut(), &order, &saved);
        state.cal_penalty();
        return true;
    }
    false
}

/// Upstream `SACoreSoftMacro::run`'s tail, once `fastSA` has finished.
///
/// 🔑 **Alignment is the consolation prize.** It runs only when centralization was tried and
/// reverted — never after a centralization that stuck, and never when centralization declined to
/// run at all.
pub fn run_enhancements<S: Enhancements + ?Sized>(state: &mut S, force_centralization: bool) {
    let pre_cost = state.norm_cost();
    if attempt_centralization(state, pre_cost, force_centralization) {
        attempt_macro_cluster_alignment(state);
    }
}

// ---------------------------------------------------------------- the placement cost's inputs

/// What a soft macro's cluster contributes to the placement-only cost terms.
///
/// 🔑 **Fixed for the whole search.** The annealer moves and resizes macros, but nothing here
/// changes — so this is taken once, alongside the macros, and the per-term views below are rebuilt
/// from it and the current geometry on every scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MacroAttributes {
    pub kind: Option<AreaKind>,
    /// The number of hard macros the cluster holds. ⚠️ Zero for a standard-cell cluster.
    pub num_macro: i32,
    pub cluster_macro_area: i64,
    pub cluster_area: i64,
    pub is_cluster_of_unplaced_io_pins: bool,
    pub is_unconstrained_io_cluster: bool,
    /// Set only in a MACRO run — see [`WirelengthMacro::pin_offset`].
    pub pin_offset: Option<(i32, i32)>,
}

/// Everything the six placement-only cost terms read that the annealer's own state does not hold.
///
/// ⚠️ **A structural divergence, class E.** Upstream keeps every one of these as a member of
/// `SACoreSoftMacro` and reads them term by term. Gathering them into one value is what lets the
/// tiling search share the same core without carrying any of it.
#[derive(Debug, Clone, Default)]
pub struct PlacementInputs {
    pub attributes: Vec<MacroAttributes>,
    pub nets: Vec<BundledNet>,
    pub guides: Vec<(usize, (i32, i32, i32, i32))>,
    pub fences: Vec<(usize, (i32, i32, i32, i32))>,
    pub soft_blockages: Vec<(i32, i32, i32, i32)>,
    /// The parent outline's lower-left corner in the die's coordinates.
    pub outline_origin: (i32, i32),
    pub root: Root,
    /// Upstream `Rect::margin()` on the die: `2 * dx + 2 * dy`, as an `int64_t`. ⚠️ **The full
    /// margin, not the half** — the halving and its narrowing to `int` happen at the point of use,
    /// which is where upstream does them.
    pub die_margin: i64,
    /// The die edges an unconstrained IO cluster's pins may land on.
    pub available_regions: Vec<Region>,
    /// A constrained IO cluster's own region, by macro id.
    pub constraint_regions: Vec<(usize, Region)>,
    pub weights: crate::anneal::SoftWeights,
}

impl PlacementInputs {
    fn wirelength_view(&self, macros: &[crate::anneal::SoftMacro]) -> Vec<WirelengthMacro> {
        macros
            .iter()
            .zip(&self.attributes)
            .map(|(m, a)| WirelengthMacro {
                x: m.x,
                y: m.y,
                width: m.width,
                height: m.height,
                is_cluster_of_unplaced_io_pins: a.is_cluster_of_unplaced_io_pins,
                is_unconstrained_io_cluster: a.is_unconstrained_io_cluster,
                pin_offset: a.pin_offset,
            })
            .collect()
    }

    /// ⚠️ **Both net lists are the same one here.** They are separate parameters so the reference's
    /// one-character difference between the weight sum's list and the length's stays visible; its
    /// only caller passes the same list to both, and so does this.
    pub fn wirelength(
        &self,
        macros: &[crate::anneal::SoftMacro],
        outline: (i32, i32),
    ) -> f32 {
        if self.weights.wirelength <= 0.0 {
            return 0.0;
        }
        let view = self.wirelength_view(macros);
        let constraint_of = |id: usize| {
            self.constraint_regions.iter().find(|(i, _)| *i == id).map(|(_, r)| r.clone())
        };
        compute_nets_wire_length(
            &self.nets,
            &self.nets,
            &view,
            outline,
            self.die_margin,
            &self.available_regions,
            &constraint_of,
        )
    }

    pub fn guidance(&self, macros: &[crate::anneal::SoftMacro], dbu_per_micron: i32) -> f32 {
        let view = self.wirelength_view(macros);
        guidance_penalty(&self.guides, &view, self.weights.guidance, dbu_per_micron)
    }

    pub fn fence(&self, macros: &[crate::anneal::SoftMacro], outline: (i32, i32)) -> f32 {
        let view = self.wirelength_view(macros);
        fence_penalty(&self.fences, &view, outline, self.weights.fence)
    }

    pub fn boundary(
        &self,
        macros: &[crate::anneal::SoftMacro],
        sp: &crate::anneal::SequencePair,
        dbu_per_micron: i32,
    ) -> f32 {
        let view: Vec<BoundaryMacro> = macros
            .iter()
            .zip(&self.attributes)
            .map(|(m, a)| BoundaryMacro {
                x: m.x,
                y: m.y,
                width: m.width,
                height: m.height,
                fixed: m.fixed,
                num_macro: a.num_macro,
            })
            .collect();
        boundary_penalty(
            &view,
            &sp.pos,
            self.outline_origin,
            &self.root,
            self.weights.boundary,
            dbu_per_micron,
        )
    }

    pub fn soft_blockage(
        &self,
        macros: &[crate::anneal::SoftMacro],
        sp: &crate::anneal::SequencePair,
    ) -> f32 {
        let view: Vec<BlockageMacro> = macros
            .iter()
            .zip(&self.attributes)
            .map(|(m, a)| BlockageMacro {
                x: m.x,
                y: m.y,
                width: m.width,
                height: m.height,
                num_macro: a.num_macro,
                cluster_macro_area: a.cluster_macro_area,
                cluster_area: a.cluster_area,
            })
            .collect();
        soft_blockage_penalty(&view, &sp.pos, &self.soft_blockages, self.weights.soft_blockage)
    }

    /// ⚠️ **A macro with no cluster behind it obstructs nothing**, whatever its geometry — see
    /// [`NotchMacro::obstructs`]. `None` is upstream's null cluster pointer.
    pub fn notch(
        &self,
        macros: &[crate::anneal::SoftMacro],
        outline: (i32, i32),
        packing: (i32, i32),
        valid: bool,
    ) -> f32 {
        let view: Vec<NotchMacro> = macros
            .iter()
            .zip(&self.attributes)
            .map(|(m, a)| NotchMacro {
                x: m.x,
                y: m.y,
                width: m.width,
                height: m.height,
                kind: a.kind,
            })
            .collect();
        notch_penalty(&view, outline, packing, valid, self.weights.notch)
    }
}

// ---------------------------------------------------------------- choosing a run

/// Upstream `computeUtilizationList`.
///
/// 🔑 **Ten utilizations, ramped GEOMETRICALLY from the target up to exactly 1.0.** The first is
/// the utilization the user asked for; each later one packs the clusters tighter, so the annealer
/// gets progressively more room to find something that fits. The last is the degenerate case where
/// the clusters are shrunk to their bare area.
///
/// ⚠️ **The ratio is `pow(1 / target, 1 / (runs - 1))`**, computed with the division in `f32` and
/// the power in `f64`, then narrowed — and each entry is `target * pow(ratio, i)`, `f64` again and
/// narrowed again. Doing the whole thing in one precision gives different last bits, and these
/// values feed `applyUtilization`, which sizes every macro.
///
/// ⛔ **Which operand is divided in `f32` cannot be established from the default target.** At
/// `0.25` — and at most plausible targets — an `f32` division and an `f64` one give the same ten
/// values. Roughly one target in thirty separates them; `0.32` is one, and the reference's own
/// `fine_shaping` output at `0.32` agrees with the `f32` division. Pinned in
/// `tests/placement_runs.rs`, which carries both captures.
///
/// ⚠️ Upstream's parameter is a `float` and its loop counter an `int`, compared against it. At ten
/// runs both are exact; the shape is kept so the oddity stays visible.
///
/// ⛔ **A target of zero divides by zero** and every entry comes back infinite. Upstream does not
/// guard it and neither does this — `-target_util 0` is the user's to get wrong.
pub fn utilization_list(target_utilization: f32, total_number_of_runs: i32) -> Vec<f32> {
    let maximum_utilization = 1.0f32;
    // ⚠️ The division is in `f32`; only the exponent and the power are wider.
    let base = (maximum_utilization / target_utilization) as f64;
    let exponential_ratio = base.powf(1.0 / (total_number_of_runs as f64 - 1.0)) as f32;

    (0..total_number_of_runs)
        .map(|i| (target_utilization as f64 * (exponential_ratio as f64).powf(i as f64)) as f32)
        .collect()
}

/// The run `placeChildren` settled on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectedRun {
    pub index: usize,
    pub utilization: f32,
    /// ⚠️ **Set when the chosen run is not the first**, which is upstream's MPL-55 warning:
    /// "Couldn't find a solution for the specified utilization. The utilization was adjusted."
    /// It is a warning, not an error — the run still counts.
    pub utilization_was_adjusted: bool,
}

/// Nobody produced a valid solution. Upstream raises MPL-40 at the root and MPL-8 below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoValidSolution;

/// Upstream `placeChildren`'s run loop: try utilizations in order until one anneals to a valid
/// solution.
///
/// 🔑 **The FIRST valid run wins, in index order — not the cheapest.** The batch exists only to
/// spread the work across threads; the scan that picks a winner walks the batch in the order it
/// was built, which is index order, and breaks immediately.
///
/// 🔑 **So the answer does not depend on the thread count.** With one thread the runs are tried
/// one at a time and the first valid one ends the loop; with ten, all ten run and the first valid
/// one is chosen. Same winner, different amount of wasted work. That is what makes running them
/// sequentially here a faithful transcription rather than an approximation.
///
/// ⛔ **A run skipped for an invalid utilization still CONSUMES its slot.** `run_id` advances
/// before the test, and the batch size is subtracted from the remaining count whether or not an
/// annealer was built — so a design with three unusable utilizations gets seven attempts, not ten.
///
/// ⚠️ **`valid_utilization` is asked before any annealing**, and `run` is asked for every attempt
/// in a batch before any result is examined. Examining as you go would stop earlier and is a
/// different amount of work, though not a different winner.
pub fn select_run(
    utilizations: &[f32],
    num_threads: usize,
    valid_utilization: &mut dyn FnMut(f32) -> bool,
    run: &mut dyn FnMut(usize, f32) -> bool,
) -> Result<SelectedRun, NoValidSolution> {
    let mut remaining_runs = utilizations.len();
    let mut run_id = 0usize;

    while remaining_runs > 0 {
        let number_of_attempts = remaining_runs.min(num_threads.max(1));

        let mut batch: Vec<usize> = Vec::new();
        for _ in 0..number_of_attempts {
            let index = run_id;
            run_id += 1;
            // ⛔ The slot is spent either way; see above.
            if !valid_utilization(utilizations[index]) {
                continue;
            }
            batch.push(index);
        }

        // The whole batch anneals before any of it is judged.
        let results: Vec<bool> = batch.iter().map(|&i| run(i, utilizations[i])).collect();

        remaining_runs -= number_of_attempts;

        for (position, &index) in batch.iter().enumerate() {
            if results[position] {
                return Ok(SelectedRun {
                    index,
                    utilization: utilizations[index],
                    utilization_was_adjusted: index != 0,
                });
            }
        }
    }

    Err(NoValidSolution)
}

/// The error upstream raises when no run produced a valid solution.
///
/// ⚠️ **Two different codes for the same condition.** At the root it is MPL-40 and blames the
/// core utilization, which the user can act on; anywhere below it is MPL-8 and asks for a bug
/// report, because a child outline that cannot be filled is upstream's own doing.
pub fn no_valid_solution_error(
    is_root: bool,
    cluster_id: i32,
    cluster_name: &str,
) -> crate::options::MplError {
    if is_root {
        crate::options::MplError::new(
            40,
            "Annealing engine failed to find a valid solution. Core utilization is probably too \
             high. Please, reduce it and try again.",
        )
    } else {
        crate::options::MplError::new(
            8,
            &format!(
                "Annealing engine failed to find a valid solution. Please, report this internal \
                 error.\nFailed at cluster ({cluster_id}): {cluster_name}"
            ),
        )
    }
}

/// Upstream `updateChildrenRealLocation`: move every child out of the parent's coordinates and
/// into the die's.
///
/// ⛔ **The offsets are `float` upstream while the coordinates are `int`**, so every coordinate
/// makes an `int` → `float` → `int` round trip. Above 2^24 database units — 8.4 mm at 2000 units
/// per micron, which a real die reaches — a `float` cannot hold every integer, so the result can
/// be a unit or two off. The narrowing then truncates toward zero. Reproduced, not tidied: doing
/// the addition in integers is more correct and is a different program.
pub fn to_real_locations(children: &mut [(i32, i32)], offset: (i32, i32)) {
    for (x, y) in children.iter_mut() {
        *x = (*x as f32 + offset.0 as f32) as i32;
        *y = (*y as f32 + offset.1 as f32) as i32;
    }
}

// ---------------------------------------------------------------- assembling one parent's problem

/// One child of the parent being placed, as the assembler needs to see it.
///
/// ⚠️ **The soft macro is built by the caller**, because how it is built depends on what the child
/// is — a fixed macro clips its hard macro to the outline, an IO cluster is a point with a name,
/// and everything else comes from the cluster. What the assembler decides is the ORDER and the ids.
#[derive(Debug, Clone)]
pub struct AssemblyChild {
    pub name: String,
    pub kind: AreaKind,
    pub macro_: crate::anneal::SoftMacro,
    /// The fence merged over this cluster's hard macros, in the DIE's coordinates and unclipped.
    /// `None` when the cluster declares none.
    pub fence: Option<(i32, i32, i32, i32)>,
    pub guide: Option<(i32, i32, i32, i32)>,
}

/// The placement problem for one parent, in the reference's own order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Assembly {
    pub macros: Vec<crate::anneal::SoftMacro>,
    /// Name to macro id, in insertion order. ⚠️ A `Vec`, not a map: the ORDER is part of the
    /// answer, and upstream's `std::map` is only ever read by name.
    pub id_of: Vec<(String, usize)>,
    /// ⛔ Captured BEFORE the IO clusters and fixed terminals are appended, so those are in the
    /// macro list but outside the sequence pair — the annealer never moves them.
    pub number_of_sequence_pair_macros: usize,
    pub fences: Vec<(usize, (i32, i32, i32, i32))>,
    pub guides: Vec<(usize, (i32, i32, i32, i32))>,
}

/// Upstream `placeChildren`'s macro-list construction.
///
/// 🔑 **The id order IS the answer.** Every net, fence, guide and blockage indexes into this list,
/// and the sequence pair is `0..number_of_sequence_pair_macros`. Getting the order wrong does not
/// produce a worse placement, it produces a different problem.
///
/// The order, and nothing may move between the groups:
/// 1. **blockages**, so they occupy the lowest ids and offset every cluster;
/// 2. **the parent's children**, in child order, with IO clusters SKIPPED;
/// 3. — the sequence-pair count is taken here —
/// 4. **the IO clusters**, in the order they were skipped;
/// 5. **the fixed terminals**, from the walk up the tree.
///
/// ⛔ **An IO cluster is deferred, not dropped.** Upstream says why at the site: holding them back
/// until the list is populated with the clusters actually being placed is what lets the SA moves
/// treat everything past the sequence-pair count as immovable.
///
/// ⚠️ **A fixed macro cluster still takes an id**, because the name is recorded before the branch
/// that skips building a cluster-backed soft macro for it.
///
/// ⛔ **A standard-cell cluster gets NO fence and NO guide**, whatever was declared for it —
/// upstream `continue`s before the merge. It has no hard macros to merge over, so the merge would
/// be empty anyway; the early exit is reproduced because it is what the reference does, not
/// because the two are provably the same for every input. A **fixed macro cluster** takes the same
/// exit, one branch earlier.
///
/// ⚠️ **Blockages get ids but no NAMES.** `createSoftMacrosForBlockages` never touches the id map,
/// so a blockage is addressable only by position — which is all the sequence pair and the
/// blockage list need.
pub fn assemble(
    blockages: &[crate::anneal::SoftMacro],
    children: &[AssemblyChild],
    outline: (i32, i32, i32, i32),
    terminals: &[(String, crate::anneal::SoftMacro)],
) -> Assembly {
    let mut out = Assembly { macros: blockages.to_vec(), ..Default::default() };

    let mut deferred_io: Vec<&AssemblyChild> = Vec::new();
    for child in children {
        if child.kind == AreaKind::IoCluster {
            deferred_io.push(child);
            continue;
        }

        let id = out.macros.len();
        out.id_of.push((child.name.clone(), id));
        out.macros.push(child.macro_);

        if child.kind == AreaKind::FixedMacro || child.kind == AreaKind::StdCellCluster {
            continue;
        }

        if let Some(fence) = child.fence {
            if let Some(clipped) = merged_region(&[fence], outline) {
                out.fences.push((id, clipped));
            }
        }
        if let Some(guide) = child.guide {
            if let Some(clipped) = merged_region(&[guide], outline) {
                out.guides.push((id, clipped));
            }
        }
    }

    // ⛔ Taken HERE — everything appended below is outside the sequence pair.
    out.number_of_sequence_pair_macros = out.macros.len();

    for child in deferred_io {
        out.id_of.push((child.name.clone(), out.macros.len()));
        out.macros.push(child.macro_);
    }

    for (name, terminal) in terminals {
        out.id_of.push((name.clone(), out.macros.len()));
        out.macros.push(*terminal);
    }

    out
}

impl Assembly {
    /// The id a name was given, or `None`.
    ///
    /// ⚠️ **The LAST binding wins**, because upstream assigns into a `std::map` and `map[k] = v`
    /// overwrites. ℹ️ Whether a name can actually repeat is NOT established — cluster names look
    /// unique across the tree, and no design in the suite repeats one. This reproduces the map's
    /// rule rather than asserting that the situation arises.
    pub fn id(&self, name: &str) -> Option<usize> {
        self.id_of.iter().rev().find(|(n, _)| n == name).map(|(_, id)| *id)
    }
}

// ---------------------------------------------------------------- closing out a parent

/// What `placeChildren` does when handed a cluster.
///
/// ⚠️ **The macro-cluster test comes FIRST**, before the leaf test — so a hard-macro cluster that
/// is also a leaf goes to macro placement, not to the leaf return. Reordering the two would place
/// nothing at all for the commonest case there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementAction {
    /// A hard-macro cluster: hand it to macro placement.
    PlaceMacros,
    /// ⛔ **Also reached by a FIXED macro cluster**, whose type is `HardMacroCluster` — it takes
    /// this branch and is then refused by macro placement's own first line. Two guards, one
    /// outcome, and neither is redundant: the type test does not know about fixedness.
    PlaceMacrosButRefused,
    /// A leaf: nothing to place. Upstream's comment names what lands here — IO clusters, leaf
    /// standard-cell clusters and fixed macros.
    Nothing,
    /// Place this cluster's children, then recurse into each of them.
    PlaceChildren,
}

/// Upstream `placeChildren`'s two opening guards, and `placeMacros`' first line.
pub fn placement_action(kind: AreaKind, is_fixed_macro: bool, is_leaf: bool) -> PlacementAction {
    if kind == AreaKind::HardMacroCluster || is_fixed_macro {
        // ⚠️ A fixed macro cluster's TYPE is `HardMacroCluster`, so it arrives here either way.
        return if is_fixed_macro {
            PlacementAction::PlaceMacrosButRefused
        } else {
            PlacementAction::PlaceMacros
        };
    }
    if is_leaf {
        return PlacementAction::Nothing;
    }
    PlacementAction::PlaceChildren
}

/// A child named in the tree but not in the assembled problem — upstream's `std::map::at` throws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownChild(pub String);

/// Upstream `updateChildrenShapesAndLocations`: write each annealed macro back onto its cluster.
///
/// ⛔ **An IO cluster is SKIPPED, and that is not an optimisation.** Its soft macro was built at
/// CLUSTERING time by `setAsIOBundle` / `setAsIOPadCluster` / `setAsClusterOfUnplacedIOPins`, and
/// it is the authoritative one — the edge slice or region the pins actually occupy. The annealer's
/// copy is a zero-area stand-in that exists only to be a net terminal. Overwriting the real one
/// with it would lose the region.
///
/// 🔑 **The same is NOT true of a fixed macro cluster, and that asymmetry is the point.** Its soft
/// macro is also built at clustering time (`setAsFixedMacro`), in ABSOLUTE, unclipped coordinates
/// — and here it IS overwritten, by the annealer's outline-relative, clipped copy. So the two
/// clustering-time soft macros are treated in opposite ways, a few lines apart.
///
/// ⚠️ **The whole macro is assigned, not just its position** — shape included, which is how a
/// mixed cluster keeps the dimensions the annealer chose for it.
///
/// ⛔ Upstream indexes with `std::map::at`, so a child missing from the id map THROWS rather than
/// being skipped. Returned as a typed error here.
pub fn update_children_shapes_and_locations(
    children: &[(String, AreaKind)],
    shaped: &[crate::anneal::SoftMacro],
    assembly: &Assembly,
) -> Result<Vec<(String, crate::anneal::SoftMacro)>, UnknownChild> {
    let mut out = Vec::new();
    for (name, kind) in children {
        if *kind == AreaKind::IoCluster {
            continue;
        }
        let Some(id) = assembly.id(name) else {
            return Err(UnknownChild(name.clone()));
        };
        out.push((name.clone(), shaped[id]));
    }
    Ok(out)
}

// ---------------------------------------------------------------- configuring a macro-placement run

/// The four move probabilities a hard-macro annealer is built with.
///
/// ⛔ **There is no RESIZE.** A hard macro has one shape; the fifth action the soft annealer has
/// does not exist here, and it is absent from the normalising sum as well.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HardActionProbabilities {
    pub pos_swap: f32,
    pub neg_swap: f32,
    pub double_swap: f32,
    pub exchange: f32,
}

/// Upstream `placeMacros`' probability setup.
///
/// 🔑 **Exchange is scaled by how much MASTER SHARING there is.** Upstream says why at the site:
/// exchanging two macros is only useful when they might be interchangeable, so the raw exchange
/// probability is multiplied by `5 * (1 - masters/macros)`. Every macro having its own master
/// makes that factor exactly **zero** and switches exchange off entirely; every macro sharing one
/// master drives it to nearly `5`.
///
/// ⛔ **The single-sequence swaps are scaled by TEN and the double swap is NOT.** So relative to
/// cluster placement, a hard-macro run is pushed hard towards single swaps, and the double swap —
/// which the soft annealer weights equally with the others — becomes rare.
///
/// ⚠️ Every step is `f32`: `exchange * 5` is `float * int`, and `masters / macros` is
/// `size_t / (float)size_t`, which C++ evaluates in `float`.
///
/// ⚠️ **The sum is formed from the SCALED values**, so the four results are a normalised
/// distribution even though the inputs are not.
pub fn macro_placement_probabilities(
    pos_swap: f32,
    neg_swap: f32,
    double_swap: f32,
    exchange: f32,
    master_count: usize,
    macro_count: usize,
) -> HardActionProbabilities {
    // ⚠️ `masters.size() / (float) hard_macros.size()` — the division is in `f32`.
    let sharing = 1.0 - (master_count as f32 / macro_count as f32);
    let exchange = exchange * 5.0 * sharing;

    // ⛔ Ten on the single swaps, nothing on the double swap.
    let action_sum = pos_swap * 10.0 + neg_swap * 10.0 + double_swap + exchange;

    HardActionProbabilities {
        pos_swap: pos_swap * 10.0 / action_sum,
        neg_swap: neg_swap * 10.0 / action_sum,
        double_swap: double_swap / action_sum,
        exchange: exchange / action_sum,
    }
}

/// Upstream `placeChildren`' perturbation count — the CLUSTER placement rule.
///
/// ⛔ **Not the coarse-shaping rule and not the macro rule.** All three sites derive their own:
/// shaping takes `max(macros, num/10)`, this one takes `max(macros, num)` — the FULL configured
/// count as a floor, ten times shaping's — and `macro_perturbations_per_step` has a third.
/// Upstream states the intent at the site: a step should have more perturbations than there are
/// macros.
///
/// 🔑 **So a small cluster is perturbed `num_perturb_per_step` times here**, where coarse shaping
/// would perturb it fifty. Sharing one derivation between the two silently gives one of them the
/// other's count, and the annealer's whole trajectory changes with it.
pub fn cluster_perturbations_per_step(num_perturb_per_step: i32, macro_count: i32) -> i32 {
    macro_count.max(num_perturb_per_step)
}

/// Upstream `placeMacros`' perturbation count.
///
/// ⛔ **The floor is a TENTH of the configured count**, by integer division — `500 / 10` is `50`.
/// A cluster with fewer macros than that still gets the floor, so a two-macro cluster is perturbed
/// fifty times a step.
///
/// ⚠️ **A "large" cluster is one with MORE macros than the floor**, and it is perturbed once per
/// macro — so past the floor the count tracks the problem size rather than a constant.
///
/// 🔑 **A large macro ARRAY is the exception and gets the FULL count**, not a tenth. Upstream says
/// why: large arrays need more steps to converge.
pub fn macro_perturbations_per_step(
    num_perturb_per_step: i32,
    macro_count: i32,
    is_macro_array: bool,
) -> i32 {
    let minimum = num_perturb_per_step / 10;
    let large = macro_count > minimum;
    if is_macro_array && large {
        return num_perturb_per_step;
    }
    if large {
        macro_count
    } else {
        minimum
    }
}

/// What a macro ARRAY does to the run's configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroArraySetup {
    pub probabilities: HardActionProbabilities,
    /// ⛔ **The ONLY place `disallowInvalidStates` is called in the whole engine.** Every other
    /// annealer — coarse shaping and cluster placement included — leaves invalid states allowed.
    pub invalid_states_allowed: bool,
}

/// Upstream `placeMacros`' macro-array branch.
///
/// 🔑 **An array with no empty space does not need to explore shapes at all**, so every swap
/// probability is zeroed and exchange is set to `1.0` — the run does nothing but swap macros
/// around a fixed arrangement, looking for the best wirelength.
///
/// ⛔ **An array WITH empty space instead disallows invalid states**, which is the one place in the
/// engine that flag is ever set. It leaves the probabilities alone.
///
/// ⚠️ A cluster that is not an array keeps its probabilities and allows invalid states.
pub fn macro_array_setup(
    probabilities: HardActionProbabilities,
    is_macro_array: bool,
    array_has_empty_space: bool,
) -> MacroArraySetup {
    if !is_macro_array {
        return MacroArraySetup { probabilities, invalid_states_allowed: true };
    }
    if array_has_empty_space {
        return MacroArraySetup { probabilities, invalid_states_allowed: false };
    }
    MacroArraySetup {
        probabilities: HardActionProbabilities {
            pos_swap: 0.0,
            neg_swap: 0.0,
            double_swap: 0.0,
            exchange: 1.0,
        },
        invalid_states_allowed: true,
    }
}

/// Upstream `placeMacros`' per-run weights.
///
/// 🔑 **Each run is a HARDER version of the last.** The outline weight is multiplied by
/// `(run_id + 1) * 10` and the wirelength weight divided by `(run_id + 1)`, so the first run is
/// free to spread out while the tenth is squeezed into the outline almost regardless of wire
/// length. Ten runs is not ten samples of one problem; it is a ramp.
///
/// ⛔ **These weights are RESET before the runs are compared** — see [`best_macro_run`]. The
/// escalation shapes the search and then has no say in which search won.
///
/// ⚠️ `float *= int` and `float /= int`, so both stay in `f32`.
pub fn macro_run_weights(base: crate::anneal::SoftWeights, run_id: i32) -> crate::anneal::SoftWeights {
    let mut w = base;
    w.outline *= ((run_id + 1) * 10) as f32;
    w.wirelength /= (run_id + 1) as f32;
    w
}

/// Upstream `placeMacros`' seed for one run.
///
/// ⛔ **Each run gets a DIFFERENT seed**, unlike the tiling search — where every run in a batch
/// shares one seed and differs only in its outline. Here the seed and the weights both move.
pub fn macro_run_seed(random_seed: u32, run_id: i32) -> u32 {
    random_seed.wrapping_add(run_id as u32)
}

/// Upstream `placeMacros`' run selection.
///
/// ⛔ **The BEST cost wins, not the first valid one** — the opposite of cluster placement, which
/// takes the first valid run in index order and stops. Here every run is annealed and then scored
/// on the COMMON weighting, because each ran under its own escalated one.
///
/// ⚠️ **`<`, strictly**, so a tie goes to the LOWEST run id — the run that was least squeezed.
///
/// ⚠️ An invalid run is never a candidate, whatever it cost.
pub fn best_macro_run(costs: &[(bool, f32)]) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (run_id, &(is_valid, cost)) in costs.iter().enumerate() {
        if !is_valid {
            continue;
        }
        match best {
            Some((_, best_cost)) if cost >= best_cost => {}
            _ => best = Some((run_id, cost)),
        }
    }
    best.map(|(id, _)| id)
}

// ---------------------------------------------------------------- per-macro-cluster inputs

/// Upstream `Rect::intersects`: ⚠️ **inclusive on every edge**, so two rectangles that merely touch
/// along a line intersect, and the intersection is a degenerate rect rather than nothing.
fn rects_intersect(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    b.2 >= a.0 && b.0 <= a.2 && b.3 >= a.1 && b.1 <= a.3
}

/// Upstream `computeFencesAndGuides`: one macro's fence or guide, clipped to the outline and
/// rebased onto it.
///
/// ⛔ **There is NO area test here, and that is the difference from the cluster path.**
/// `placeChildren` records a fence only when the clipped rect has positive area; this records it
/// unconditionally. A fence that misses the outline entirely becomes upstream's zero rect and is
/// then shifted to `(-xMin, -yMin)` — a degenerate box at a NEGATIVE position.
///
/// 🔑 **That degenerate entry still counts.** `calFencePenalty` skips it, because a zero-extent
/// fence cannot fit the macro — but it skips it AFTER the divisor is taken, so it dilutes the mean
/// for every other fence. A macro whose fence misses the outline silently weakens the fence term
/// for the whole cluster.
///
/// ⚠️ **Clipping is INCLUSIVE**: a fence touching the outline along a line survives as a zero-width
/// box at a real position, which is a different thing from the miss case above.
pub fn clip_region_to_outline(
    region: (i32, i32, i32, i32),
    outline: (i32, i32, i32, i32),
) -> (i32, i32, i32, i32) {
    let clipped = if rects_intersect(region, outline) {
        (
            region.0.max(outline.0),
            region.1.max(outline.1),
            region.2.min(outline.2),
            region.3.min(outline.3),
        )
    } else {
        // ⛔ Upstream's `intersection` writes a ZERO rect on a miss, not the empty set.
        (0, 0, 0, 0)
    };
    (clipped.0 - outline.0, clipped.1 - outline.1, clipped.2 - outline.0, clipped.3 - outline.1)
}

/// Upstream `computeFencesAndGuides` over a macro cluster's hard macros.
///
/// ⚠️ **Keyed by the macro's INDEX in the cluster's hard-macro list**, which is the same as its id
/// in the annealer because `createTempMacroClusters` builds the SA macros from that list in order.
///
/// ⚠️ **Fences are looked up by macro NAME and guides by database instance** — two different keys
/// for the two maps, which is why they are separate arguments here.
pub fn macro_fences_and_guides(
    fence_of: &dyn Fn(usize) -> Option<(i32, i32, i32, i32)>,
    guide_of: &dyn Fn(usize) -> Option<(i32, i32, i32, i32)>,
    macro_count: usize,
    outline: (i32, i32, i32, i32),
) -> (Vec<(usize, (i32, i32, i32, i32))>, Vec<(usize, (i32, i32, i32, i32))>) {
    let mut fences = Vec::new();
    let mut guides = Vec::new();
    for i in 0..macro_count {
        if let Some(fence) = fence_of(i) {
            fences.push((i, clip_region_to_outline(fence, outline)));
        }
        if let Some(guide) = guide_of(i) {
            guides.push((i, clip_region_to_outline(guide, outline)));
        }
    }
    (fences, guides)
}

/// The starting arrangement for a macro ARRAY, and whether the grid has gaps.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArraySequencePair {
    pub pos: Vec<usize>,
    pub neg: Vec<usize>,
    /// ⚠️ Upstream's out-parameter, which is only ever set TRUE — the caller initialises it.
    pub has_empty_space: bool,
}

/// Upstream `computeArraySequencePair`: the sequence pair that encodes a regular grid.
///
/// 🔑 **The positive sequence is just `0..n`, and the negative one walks the grid COLUMN BY
/// COLUMN, downwards within each column.** That pairing is what a sequence pair means by "these
/// macros are laid out as a grid" — it is a starting point the annealer then permutes, not a
/// constraint.
///
/// ⛔ **`std::round` on an INTEGER division, which has already truncated.** Both `getWidth()`s are
/// `int`, so `cluster_width / macro_width` is integer division and the rounding is decorative: a
/// cluster 2.9 macros wide gives **2** columns, not 3. Reading it as "the ratio, rounded" gives a
/// different grid for every cluster whose width is not an exact multiple.
///
/// ⚠️ **A grid position past the last macro sets `has_empty_space`** rather than being skipped
/// quietly, and that flag is what makes the caller disallow invalid states.
///
/// ⛔ **The flag reports a SLOT WITH NO MACRO, never a MACRO WITH NO SLOT.** A grid too SMALL for
/// the macros it holds reports no empty space at all, and leaves a negative sequence shorter than
/// the positive one — so the two are not permutations of each other, which every consumer of a
/// sequence pair assumes they are.
/// ℹ️ Not known to be reachable: a macro array's cluster dimensions come from its own tilings, so
/// the grid should always be large enough. Written down because the flag's name suggests it covers
/// both directions and it covers only one.
pub fn array_sequence_pair(
    macro_count: usize,
    cluster_width: i32,
    cluster_height: i32,
    macro_width: i32,
    macro_height: i32,
) -> ArraySequencePair {
    let mut out = ArraySequencePair { pos: (0..macro_count).collect(), ..Default::default() };

    // ⛔ Integer division; the `round` upstream applies to an already-integral value.
    let columns = if macro_width != 0 { cluster_width / macro_width } else { 0 };
    let rows = if macro_height != 0 { cluster_height / macro_height } else { 0 };

    for i in 1..=columns {
        for j in 1..=rows {
            let macro_id = (rows * i) - j;
            if (macro_id as usize) < macro_count {
                out.neg.push(macro_id as usize);
            } else {
                out.has_empty_space = true;
            }
        }
    }
    out
}

// ---------------------------------------------------------------- the hard-macro annealer

/// Upstream `SACoreHardMacro::calNormCost`: the FIVE terms the base core owns.
///
/// 🔑 **Boundary, soft blockage, fixed macros and notch do not exist here.** They are
/// `SACoreSoftMacro`'s own members, not the base class's — so a hard-macro run has no notion of
/// them at all, rather than weighting them at zero.
///
/// ℹ️ Delegating to [`crate::anneal::norm_cost`] with those four penalties forced to zero is
/// **bit-identical** to writing the five terms out: they are the first five in the same order, and
/// each of the other four then contributes an exact `0.0`, which leaves an `f32` sum unchanged.
/// Written this way so the relationship between the two cores is visible rather than duplicated.
pub fn hard_norm_cost(
    p: &crate::anneal::Penalties,
    w: &crate::anneal::SoftWeights,
    n: &crate::anneal::Normalization,
) -> f32 {
    crate::anneal::norm_cost(
        &crate::anneal::Penalties {
            boundary: 0.0,
            soft_blockage: 0.0,
            fixed_macros: 0.0,
            notch: 0.0,
            ..*p
        },
        w,
        n,
    )
}

impl HardActionProbabilities {
    /// Upstream `SACoreHardMacro::perturb`'s action choice.
    ///
    /// ⛔ **THREE thresholds for FOUR actions**, so exchange is the `else` and takes everything
    /// left over — including whatever slack the normalisation left behind. The soft core has four
    /// thresholds and gives the remainder to resize instead; there is no resize here, because a
    /// hard macro has one shape.
    ///
    /// ⚠️ `<=` at every threshold, matching the reference.
    pub fn action_for(&self, draw: f32) -> crate::anneal::Action {
        let one = self.pos_swap;
        let two = one + self.neg_swap;
        let three = two + self.double_swap;
        if draw <= one {
            crate::anneal::Action::SwapPositive
        } else if draw <= two {
            crate::anneal::Action::SwapNegative
        } else if draw <= three {
            crate::anneal::Action::SwapBoth
        } else {
            crate::anneal::Action::Exchange
        }
    }
}

/// Upstream `calAverage`'s result after the `<= 1e-4` floor.
///
/// ⚠️ **Not a clamp to something small — it becomes exactly `1.0`.** A penalty that is almost
/// always zero therefore reaches the cost undamped on the rare step where it is not.
///
/// ⚠️ `<=`, so a factor of exactly `1e-4` is floored too.
pub fn norm_floor(value: f32) -> f32 {
    if value <= 1e-4 {
        1.0
    } else {
        value
    }
}

/// Upstream's initial temperature, identical in both cores.
///
/// 🔑 **It comes from the mean ABSOLUTE step-to-step CHANGE in cost, not from the spread.** Two
/// runs with the same range of costs but different orderings get different temperatures.
///
/// ⚠️ **Fewer than two samples, or no change at all, gives exactly `1.0`** rather than dividing by
/// zero — and a sweep that recorded nothing lands here.
pub fn init_temperature(costs: &[f32], init_prob: f32) -> f32 {
    let mut delta_cost = 0.0f32;
    for i in 1..costs.len() {
        delta_cost += (costs[i] - costs[i - 1]).abs();
    }
    if costs.len() > 1 && delta_cost > 0.0 {
        -(delta_cost / (costs.len() - 1) as f32) / init_prob.ln()
    } else {
        1.0
    }
}

/// Upstream's `std::vector<float> width_list` in the HARD core's `initialize`.
///
/// ⛔ **The soft core stores its widths as `int`; the hard core stores them as `float`.** The
/// replay then assigns `width_ = width_list[i]`, narrowing back to `int` — so every sampled width
/// makes an `int → float → int` round trip that the soft core does not.
///
/// ⚠️ Above 2^24 database units — 8.4 mm at 2000 units per micron, which a real die reaches — an
/// `f32` cannot hold every integer, so the replayed width can differ from the sampled one. It
/// feeds `getAreaPenalty()`, so it moves the replayed cost, the mean delta, and the initial
/// temperature with it.
pub fn hard_sampled_extent(extent: i32) -> i32 {
    extent as f32 as i32
}

// ---------------------------------------------------------------- the hard-macro netlist

/// Upstream `createFixedTerminals`' terminal set, for macro placement.
///
/// 🔑 **Ascending CLUSTER-ID order, not connection order.** Upstream gathers the ids into a
/// `std::set<int>`, which both deduplicates and sorts — so the terminal ids depend only on which
/// clusters are connected, never on the order the connections were discovered. Iterating the
/// connections directly would give a different id for every terminal.
///
/// ⚠️ **A cluster already in the macro map is skipped**, because it is one of the macros being
/// placed rather than a terminal.
pub fn hard_terminal_cluster_ids(
    connected_ids: &[i32],
    is_already_a_macro: &dyn Fn(i32) -> bool,
) -> Vec<i32> {
    let unique: std::collections::BTreeSet<i32> = connected_ids.iter().copied().collect();
    unique.into_iter().filter(|id| !is_already_a_macro(*id)).collect()
}

/// A cluster missing from the macro map — upstream's `std::map::at` throws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmappedCluster(pub i32);

/// Upstream `buildBundledNets` for MACRO placement — the second overload.
///
/// ⛔ **It has NO virtual connections.** The soft overload emits the parent's virtual connections
/// first, each at weight `10.0`; this one does not emit them at all.
///
/// ⛔ **And it has NO `>` id filter, so every connection is emitted TWICE** — once from each end.
/// The soft overload halves them with `child->getId() > target->getId()` and says why at the site:
/// connections are undirected and exist in both directions. That guard is simply absent here.
///
/// 🔑 **Doubling is not always a no-op.** The wirelength is normalised by the total weight, so a
/// symmetric pair cancels out — but [`compute_nets_wire_length`] tests only the TARGET for being a
/// cluster of unplaced IO pins, so `(macro, io)` and `(io, macro)` take DIFFERENT paths. Where a
/// terminal is an unplaced-IO cluster, emitting both directions changes the answer rather than
/// scaling it.
///
/// ⚠️ **A self-connection survives**, since nothing compares the two ids.
pub fn build_bundled_nets_for_macros(
    clusters: &[(i32, Vec<(i32, f32)>)],
    macro_of: &dyn Fn(i32) -> Option<usize>,
) -> Result<Vec<BundledNet>, UnmappedCluster> {
    let mut nets = Vec::new();
    for (cluster_id, connections) in clusters {
        let Some(source) = macro_of(*cluster_id) else {
            return Err(UnmappedCluster(*cluster_id));
        };
        for &(target_id, weight) in connections {
            let Some(target) = macro_of(target_id) else {
                return Err(UnmappedCluster(target_id));
            };
            nets.push(BundledNet { source, target, weight });
        }
    }
    Ok(nets)
}

// ---------------------------------------------------------------- pushing to the core boundaries

/// Upstream `Pusher::fetchMacroClusters`: the macro clusters the push will consider.
///
/// ⚠️ **It descends into MIXED clusters only.** A standard-cell cluster is not entered, so a macro
/// cluster underneath one would never be fetched. Nothing builds that shape today; the restriction
/// is the reference's and is reproduced rather than generalised.
///
/// 🔑 **Depth-first in child order**, and each cluster's hard macros are flattened into one list as
/// it is visited — that flat list is what the overlap tests later scan, so its order is part of the
/// answer.
pub fn fetch_macro_clusters(
    root: usize,
    type_of: &dyn Fn(usize) -> crate::cluster::ClusterType,
    children_of: &dyn Fn(usize) -> Vec<usize>,
) -> Vec<usize> {
    use crate::cluster::ClusterType;
    let mut out = Vec::new();
    for child in children_of(root) {
        match type_of(child) {
            ClusterType::HardMacro => out.push(child),
            ClusterType::Mixed => {
                out.extend(fetch_macro_clusters(child, type_of, children_of));
            }
            ClusterType::StdCell => {}
        }
    }
    out
}

/// Upstream `Pusher::designHasSingleCentralizedMacroArray`.
///
/// 🔑 **This is the mirror of `singleArraySingleStdCellCluster`.** That function shrinks the
/// standard-cell cluster to nothing so a lone macro array can use the whole outline; this one
/// detects exactly that arrangement afterwards, and declines to push — the array is already where
/// it was meant to be.
///
/// ⛔ **A standard-cell cluster is judged by its SOFT MACRO's area, not the cluster's.** Upstream
/// says why at the site: `Cluster::getArea()` returns the real standard-cell area, which is never
/// zero, while the soft macro's is the abstraction — and only the abstraction records that fine
/// shaping shrank it away. Using the cluster's area here would make this always return false.
///
/// ⚠️ **Any MIXED cluster fails it immediately**, before anything is counted.
///
/// ⛔ **AN IO CLUSTER IS A MIXED CLUSTER, so it fails this outright.** `setAsIOBundle`,
/// `setAsIOPadCluster` and `setAsClusterOfUnplacedIOPins` set their own flags and a soft macro and
/// **never touch `type_`**, which defaults to `MixedCluster` — so `isIOCluster()` and
/// `getClusterType()` are answering different questions and only the second one is asked here.
/// ⚠️ This is why the guard is far rarer than it reads: any design with a single unplaced IO pin,
/// one IO pad or one IO bundle has a Mixed child of the root and is always pushed. Treating an IO
/// cluster as its own kind and skipping it silently declined the push on 27 of 34 designs.
///
/// ⛔ **A FIXED macro cluster is a HardMacroCluster and IS COUNTED.** `setAsFixedMacro` likewise
/// only sets a flag; `clusterMacros` types both the movable and the fixed macro clusters
/// `HardMacroCluster` a few lines apart. So a design with one movable and one fixed macro cluster
/// has a count of two and fails the guard.
///
/// ⚠️ **The count test is INSIDE the loop**, so a second macro cluster fails it at once rather
/// than after the whole scan — which matters only if a later child would also have failed it, but
/// it is the reference's shape.
///
/// ℹ️ **A root with no children returns TRUE**, vacuously — zero arrays counts as "a single
/// centralized macro array" and the push is skipped. Nothing reaches it: a design with no children
/// under the root has already been refused.
pub fn has_single_centralized_macro_array(children: &[(crate::cluster::ClusterType, i64)]) -> bool {
    use crate::cluster::ClusterType;
    let mut macro_cluster_count = 0;
    for &(cluster_type, soft_macro_area) in children {
        match cluster_type {
            ClusterType::Mixed => return false,
            ClusterType::HardMacro => macro_cluster_count += 1,
            ClusterType::StdCell => {
                if soft_macro_area != 0 {
                    return false;
                }
            }
        }
        if macro_cluster_count > 1 {
            return false;
        }
    }
    true
}

/// Upstream `SoftMacro::getArea`.
///
/// ⛔ **`area_ > 1 ? area_ : 0`, not `area_`.** A one-DBU² area reports as zero, which is what
/// `singleArraySingleStdCellCluster`'s shrunk standard-cell cluster relies on — and the only
/// reader that cares is `designHasSingleCentralizedMacroArray`, where the difference is whether the
/// whole boundary push runs.
pub fn soft_macro_area(area: i64) -> i64 {
    if area > 1 {
        area
    } else {
        0
    }
}

/// Why the boundary push declined to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoPush {
    /// ⚠️ A design that is nothing but macros — the root itself is a macro cluster.
    DesignIsAllMacros,
    /// The arrangement `singleArraySingleStdCellCluster` produced; already where it should be.
    SingleCentralizedMacroArray,
}

/// Upstream `Pusher::pushMacrosToCoreBoundaries`' two opening guards.
pub fn push_decision(
    root_type: crate::cluster::ClusterType,
    root_children: &[(crate::cluster::ClusterType, i64)],
) -> Result<(), NoPush> {
    if root_type == crate::cluster::ClusterType::HardMacro {
        return Err(NoPush::DesignIsAllMacros);
    }
    if has_single_centralized_macro_array(root_children) {
        return Err(NoPush::SingleCentralizedMacroArray);
    }
    Ok(())
}

/// Upstream `Pusher::getDistanceToCloseBoundaries`.
///
/// 🔑 **At most ONE horizontal and ONE vertical boundary**, each the nearer of its pair — a cluster
/// is never pushed both left and right.
///
/// ⚠️ **The threshold is the MACRO's own dimension, not the cluster's.** A cluster further from the
/// boundary than one macro is wide is left alone entirely, which is what stops the push dragging
/// clusters across the die. Upstream takes it from `getHardMacros().front()` and says why: only
/// macros of the same size are grouped.
///
/// ⛔ **A TIE goes to RIGHT, and to TOP.** The tests are `left < right` and `bottom < top`, so a
/// cluster exactly centred is pushed towards the far edges, not the near ones.
///
/// ⚠️ **Both distances are `abs`**, so a cluster already OUTSIDE the core reads as close to the
/// boundary it has passed and is pushed further out rather than back in.
///
/// ⚠️ **The result is a `std::map` keyed by boundary**, so it comes out in enum order — `B`, `L`,
/// `T`, `R` — not in the order the two were decided. The push then applies them in that order, and
/// the second is applied to a box the first may already have moved.
pub fn distance_to_close_boundaries(
    cluster_box: (i32, i32, i32, i32),
    core: (i32, i32, i32, i32),
    macro_width: i32,
    macro_height: i32,
) -> Vec<(crate::halo::Boundary, i32)> {
    use crate::halo::Boundary;
    let mut found: Vec<(Boundary, i32)> = Vec::new();

    let distance_to_left = (cluster_box.0 - core.0).abs();
    let distance_to_right = (cluster_box.2 - core.2).abs();
    let (hor_boundary, smaller_hor) = if distance_to_left < distance_to_right {
        (Boundary::L, distance_to_left)
    } else {
        // ⛔ A tie goes RIGHT.
        (Boundary::R, distance_to_right)
    };
    if smaller_hor < macro_width {
        found.push((hor_boundary, smaller_hor));
    }

    let distance_to_top = (cluster_box.3 - core.3).abs();
    let distance_to_bottom = (cluster_box.1 - core.1).abs();
    let (ver_boundary, smaller_ver) = if distance_to_bottom < distance_to_top {
        (Boundary::B, distance_to_bottom)
    } else {
        // ⛔ A tie goes TOP.
        (Boundary::T, distance_to_top)
    };
    if smaller_ver < macro_height {
        found.push((ver_boundary, smaller_ver));
    }

    // ⚠️ Upstream's container is a `std::map<Boundary, int>`, so it is read in enum order.
    found.sort_by_key(|(b, _)| *b);
    found
}

/// Upstream `Pusher::moveMacroClusterBox`: shift a box towards one boundary.
///
/// ⚠️ **`L` and `B` move NEGATIVE, `R` and `T` positive** — the distance is unsigned and the
/// direction comes entirely from the boundary.
pub fn move_towards_boundary(
    box_: (i32, i32, i32, i32),
    boundary: crate::halo::Boundary,
    distance: i32,
) -> (i32, i32, i32, i32) {
    use crate::halo::Boundary;
    let (dx, dy) = match boundary {
        Boundary::L => (-distance, 0),
        Boundary::R => (distance, 0),
        Boundary::T => (0, distance),
        Boundary::B => (0, -distance),
    };
    (box_.0 + dx, box_.1 + dy, box_.2 + dx, box_.3 + dy)
}

/// One attempted push, as the reference's `boundary_push` trace records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushAttempt {
    pub boundary: crate::halo::Boundary,
    pub distance: i32,
    /// ⚠️ **False when the move was reverted.** The reference's trace line is printed BEFORE the
    /// overlap test, so its log says "Moved X" for attempts that were undone — scoring against
    /// that channel means scoring ATTEMPTS, not commits.
    pub committed: bool,
    /// What blocked it, when one did. ⚠️ Carried rather than recomputed: the reference names the
    /// obstacle in the same trace line that reports the revert, and `overlapsWithHardMacro` is the
    /// only thing that knows WHICH macro — asking a second time would be a second traversal of a
    /// list that earlier pushes mutate.
    pub obstacle: Option<PushObstacle>,
}

/// Upstream `Pusher::pushMacroClusterToCoreBoundaries`.
///
/// 🔑 **Each boundary is tried in turn against the box the previous one left behind.** A committed
/// push is not undone by a later failure, and a reverted one leaves the box exactly as it was — so
/// the two pushes compose.
///
/// ⚠️ **A distance of zero is skipped**, not attempted: the cluster is already on that boundary.
///
/// ⚠️ **Overlap is tested on the CLUSTER's box, not on each macro** — upstream says why: it avoids
/// iterating every hard macro. So a cluster whose bounding box clears an obstacle is committed even
/// if the arrangement inside it would not.
///
/// Returns the final box and what was attempted.
pub fn push_macro_cluster(
    mut cluster_box: (i32, i32, i32, i32),
    boundaries: &[(crate::halo::Boundary, i32)],
    obstacle_for: &dyn Fn((i32, i32, i32, i32)) -> Option<PushObstacle>,
) -> ((i32, i32, i32, i32), Vec<PushAttempt>) {
    let mut attempts = Vec::new();
    for &(boundary, distance) in boundaries {
        if distance == 0 {
            continue;
        }
        let moved = move_towards_boundary(cluster_box, boundary, distance);
        match obstacle_for(moved) {
            // ⚠️ Upstream moves the box back by the same distance rather than restoring a copy.
            Some(obstacle) => {
                attempts.push(PushAttempt { boundary, distance, committed: false, obstacle: Some(obstacle) })
            }
            None => {
                cluster_box = moved;
                attempts.push(PushAttempt { boundary, distance, committed: true, obstacle: None });
            }
        }
    }
    (cluster_box, attempts)
}

/// Upstream `Rect::overlaps`: ⚠️ **STRICT on every edge**, so two boxes that merely touch do NOT
/// overlap.
///
/// ⛔ **The opposite of [`rects_intersect`], on the same class.** `Rect::intersects` is inclusive
/// and `Rect::overlaps` is not, and `mpl` uses both — clipping a fence goes through the inclusive
/// one, testing a push for obstruction through the strict one. Using either in the other's place is
/// a difference only ever visible on an exact touch, which is precisely the arrangement a boundary
/// push produces.
pub fn boxes_overlap(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    b.2 > a.0 && b.0 < a.2 && b.3 > a.1 && b.1 < a.3
}

/// Upstream `Pusher::moveHardMacro`: one macro, one axis.
///
/// ⚠️ **Only X or only Y moves** — the boundary decides which, and the other coordinate is not
/// touched. The same mapping as [`move_towards_boundary`], applied to a macro instead of a box.
pub fn move_hard_macro(
    location: (i32, i32),
    boundary: crate::halo::Boundary,
    distance: i32,
) -> (i32, i32) {
    use crate::halo::Boundary;
    match boundary {
        Boundary::L => (location.0 - distance, location.1),
        Boundary::R => (location.0 + distance, location.1),
        Boundary::T => (location.0, location.1 + distance),
        Boundary::B => (location.0, location.1 - distance),
    }
}

/// What stopped a push.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushObstacle {
    /// ⚠️ Carries the macro's index in the pusher's flat list — the reference names it in the trace.
    HardMacro(usize),
    IoBlockage(usize),
}

/// Upstream `Pusher::overlapsWithHardMacro` followed by `overlapsWithIOBlockage`.
///
/// ⛔ **Hard macros are tested FIRST and the test short-circuits**, so a box overlapping both a
/// macro and an IO blockage reports only the macro — and the reference's trace prints only that
/// line. Scoring against `boundary_push` means matching that precedence.
///
/// 🔑 **A macro belonging to the cluster being pushed is skipped**, by cluster id — otherwise every
/// push would collide with itself.
///
/// ⚠️ **The flat macro list carries positions that EARLIER pushes have already moved**, so the
/// result depends on the order the clusters were pushed in. That is upstream's own sequencing, not
/// an artefact of gathering them into one list.
pub fn push_obstacle(
    cluster_box: (i32, i32, i32, i32),
    cluster_id: i32,
    hard_macros: &[(i32, (i32, i32, i32, i32))],
    io_blockages: &[(i32, i32, i32, i32)],
) -> Option<PushObstacle> {
    for (i, &(owner, bbox)) in hard_macros.iter().enumerate() {
        if owner == cluster_id {
            continue;
        }
        if boxes_overlap(cluster_box, bbox) {
            return Some(PushObstacle::HardMacro(i));
        }
    }
    for (i, &blockage) in io_blockages.iter().enumerate() {
        if boxes_overlap(cluster_box, blockage) {
            return Some(PushObstacle::IoBlockage(i));
        }
    }
    None
}

/// One macro cluster, as `Pusher` sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushCluster {
    pub id: i32,
    pub name: String,
    /// ⚠️ Fixed clusters are still GATHERED — `fetchMacroClusters` adds their macros to the flat
    /// obstacle list before the push loop ever tests this flag. So a fixed cluster obstructs other
    /// pushes while never being pushed itself.
    pub is_fixed_macro: bool,
    /// `Cluster::getBBox()` — the SOFT MACRO's box.
    ///
    /// ⛔ **This is CLUSTER placement's box, not macro placement's.** `placeMacros` writes the
    /// hard macros and never touches the cluster's soft macro, so the distances the pusher
    /// measures come from the level above. Recomputing the box from the placed macros is a
    /// different number on any cluster the macro search did not fill edge to edge.
    pub bbox: (i32, i32, i32, i32),
    /// Indices into the flat macro list, in `getHardMacros()` order.
    ///
    /// ⚠️ **`front()` is the one that sets the push threshold** — upstream says why at the site:
    /// only macros of the same size are grouped, so any of them would do. The order still has to
    /// be the reference's for a cluster where that assumption does not hold.
    pub macros: Vec<usize>,
}

/// One hard macro, as `Pusher` sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushMacro {
    pub name: String,
    /// The id of the cluster it belongs to — what `overlapsWithHardMacro` skips on.
    pub cluster_id: i32,
    /// The HALOED lower-left corner, absolute. ⚠️ `HardMacro`'s default coordinates include the
    /// halo; the real ones are a separate accessor and are not what the pusher moves.
    pub location: (i32, i32),
    pub width: i32,
    pub height: i32,
}

impl PushMacro {
    fn bbox(&self) -> (i32, i32, i32, i32) {
        (
            self.location.0,
            self.location.1,
            self.location.0 + self.width,
            self.location.1 + self.height,
        )
    }
}

/// Upstream `Rect`'s stream formatter, which the IO-blockage revert line interpolates.
///
/// ⚠️ **Spaces inside the parentheses**, and two of them between the pairs' brackets — the exact
/// shape is `( xMin yMin ) ( xMax yMax )`. This is a trace format, so it is transcribed from a
/// captured line rather than guessed.
fn format_rect(r: (i32, i32, i32, i32)) -> String {
    format!("( {} {} ) ( {} {} )", r.0, r.1, r.2, r.3)
}

const PUSH_DEBUG: &str = "[DEBUG MPL-boundary_push] ";

/// Upstream `Pusher::pushMacrosToCoreBoundaries`, composed — the trace it prints and the macro
/// positions it leaves behind.
///
/// 🔑 **The trace is the oracle**, so this returns the lines rather than a summary. Two of them are
/// `logger_->report` rather than `debugPrint` and therefore carry **no** `[DEBUG …]` prefix: the
/// `Distance to Close Boundaries:` header and its rows. Prefixing them would be a different log.
///
/// ⛔ **The header prints even when the map is EMPTY.** It sits inside the `debugCheck` block,
/// above the loop over the map, and `pushMacroClusterToCoreBoundaries` returns on an empty map
/// only afterwards — so a cluster too far from every boundary still prints its name and the
/// header, and nothing else. `centralization1` is that case, and a version that skipped the header
/// would match every other design in the suite.
///
/// ⛔ **`Moved …` is printed BEFORE the overlap test**, so it appears for reverted pushes too.
///
/// ⚠️ **`macros` is mutated in place and the obstacle list is read from it**, which is upstream's
/// aliasing: `hard_macros_` holds raw pointers, so a committed push is visible to every later
/// cluster's overlap test. Snapshotting the list up front would make the result order-independent
/// — and wrong.
pub fn run_boundary_push(
    root_type: crate::cluster::ClusterType,
    root_children: &[(crate::cluster::ClusterType, i64)],
    clusters: &[PushCluster],
    macros: &mut [PushMacro],
    core: (i32, i32, i32, i32),
    io_blockages: &[(i32, i32, i32, i32)],
) -> Vec<String> {
    let mut out = Vec::new();
    if push_decision(root_type, root_children).is_err() {
        return out;
    }

    for cluster in clusters {
        if cluster.is_fixed_macro {
            continue;
        }
        // ⚠️ Upstream dereferences `getHardMacros().front()` unconditionally; a macro cluster
        // without macros cannot exist. We decline rather than panic, and record nothing — the
        // trace would then be short, which the gate reports as a difference.
        let Some(&first) = cluster.macros.first() else { continue };

        out.push(format!("{PUSH_DEBUG}Macro Cluster {}", cluster.name));

        let distances = distance_to_close_boundaries(
            cluster.bbox,
            core,
            macros[first].width,
            macros[first].height,
        );
        out.push("Distance to Close Boundaries:".to_string());
        for (boundary, distance) in &distances {
            out.push(format!("{} {}", boundary.name(), distance));
        }

        let flat: Vec<(i32, (i32, i32, i32, i32))> =
            macros.iter().map(|m| (m.cluster_id, m.bbox())).collect();
        let (_, attempts) = push_macro_cluster(cluster.bbox, &distances, &|b| {
            push_obstacle(b, cluster.id, &flat, io_blockages)
        });

        for attempt in attempts {
            out.push(format!(
                "{PUSH_DEBUG}Moved {} in the direction of {}.",
                cluster.name,
                attempt.boundary.name()
            ));
            match attempt.obstacle {
                Some(PushObstacle::HardMacro(index)) => out.push(format!(
                    "{PUSH_DEBUG}\tFound overlap with HardMacro {}. Push will be reverted.",
                    macros[index].name
                )),
                Some(PushObstacle::IoBlockage(index)) => out.push(format!(
                    "{PUSH_DEBUG}\tFound overlap with IO blockage {}. Push will be reverted.",
                    format_rect(io_blockages[index])
                )),
                None => {
                    for &index in &cluster.macros {
                        macros[index].location = move_hard_macro(
                            macros[index].location,
                            attempt.boundary,
                            attempt.distance,
                        );
                    }
                }
            }
        }
    }

    out
}

// ---------------------------------------------------------------- orientation correction

/// How the orientation pass is allowed to flip macros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrientationStrategy {
    /// Every unfixed macro on its own.
    Single,
    /// Whole columns and whole rows of a macro cluster, never one macro alone.
    ByCluster,
}

/// Upstream `correctAllMacrosOrientation`.
///
/// ⛔ **The branch reads backwards.** It is `!use_full_halo_` that takes the RESTRICTED,
/// by-cluster path — pin-aware halos are the case that needs the restriction, because flipping a
/// single macro inside a cluster could leave a region of it unreachable. A full halo has no such
/// worry and each macro is flipped alone.
pub fn orientation_strategy(use_full_halo: bool) -> OrientationStrategy {
    if use_full_halo {
        OrientationStrategy::Single
    } else {
        OrientationStrategy::ByCluster
    }
}

/// Upstream's accept rule after a trial flip.
///
/// ⚠️ **`>`, strictly — so a TIE KEEPS THE FLIP.** The flip is performed first and undone only when
/// it made things strictly worse, which means equal wirelength leaves the macro in its new
/// orientation rather than its original one.
pub fn keep_flip(original_wirelength: f32, new_wirelength: f32) -> bool {
    !(new_wirelength > original_wirelength)
}

/// The order of flip passes, and what each one covers.
///
/// ⛔ **TWO FULL PASSES, not one pass of two flips per macro.** Every macro is tried vertically
/// first, and only then is every macro tried horizontally — so a macro's horizontal trial is
/// measured against a board on which every other macro's vertical decision has already been made.
/// Interleaving them per macro is the obvious reading and a different algorithm.
///
/// ⚠️ Vertical before horizontal, in both strategies.
pub const FLIP_PASSES: [bool; 2] = [true, false];

/// Upstream `correctMacroOrientationByCluster`'s grouping.
///
/// 🔑 **A macro belongs to BOTH a column and a row**, so it is flipped as part of one group in the
/// vertical pass and a different group in the horizontal one.
///
/// ⚠️ **Grouped by the macro's REAL coordinate** — the one without the halo — and the groups come
/// out in ascending coordinate order, because upstream's container is a `std::map`.
///
/// ⚠️ A cluster that is not a hard-macro cluster, or is fixed, is skipped entirely.
pub fn orientation_groups(macros: &[(usize, (i32, i32))]) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let mut cols: std::collections::BTreeMap<i32, Vec<usize>> = std::collections::BTreeMap::new();
    let mut rows: std::collections::BTreeMap<i32, Vec<usize>> = std::collections::BTreeMap::new();
    for &(id, (x, y)) in macros {
        cols.entry(x).or_default().push(id);
        rows.entry(y).or_default().push(id);
    }
    (cols.into_values().collect(), rows.into_values().collect())
}

/// The database's EIGHT orientations — `odb::dbOrientType`.
///
/// ⛔ **NOT `halo::Orient`, which has five variants and folds every rotation into `Other`.** That
/// enum is the halo logic's vocabulary and cannot express `R90`; using it for a geometric transform
/// would silently place a rotated instance's pins wrong. The two are different questions about the
/// same field, which is the confusion class recorded as D2 in the divergence register.
///
/// ⚠️ **A rotated instance IS present in the suite and IS read by this stage.** `io_pads1` places
/// `PAD_1` at `W` (R90), and `calculateRealMacroWirelength` walks EVERY terminal on a macro's net —
/// pads included — so folding rotations away would mis-position that pad on one of the fourteen
/// designs this model exists to score. Measured, not assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbOrient {
    R0,
    R90,
    R180,
    R270,
    MY,
    MYR90,
    MX,
    MXR90,
}

impl DbOrient {
    /// The DEF spelling. ⚠️ `FS` is `MX` and `FN` is `MY` — the "flipped" names describe the axis
    /// the cell was mirrored ACROSS, not the direction it faces.
    pub fn from_def(name: &str) -> Option<DbOrient> {
        Some(match name {
            "N" => DbOrient::R0,
            "W" => DbOrient::R90,
            "S" => DbOrient::R180,
            "E" => DbOrient::R270,
            "FN" => DbOrient::MY,
            "FW" => DbOrient::MXR90,
            "FS" => DbOrient::MX,
            "FE" => DbOrient::MYR90,
            _ => return None,
        })
    }
}

/// Upstream `dbTransform::apply(Point&)` — an orientation about the ORIGIN, then a translation.
///
/// ⛔ **The rotation is about the master's origin, not about the instance's centre or its box.** The
/// offset is `dbInst::getTransform()`'s, which is `(inst->x_, inst->y_)` — the instance's ORIGIN,
/// the same number `setLocation` writes. Rotating about anything else moves a flipped macro's pins
/// somewhere the database does not put them.
///
/// ⚠️ **The mirror comes BEFORE the rotation** in the two combined orientations: `MYR90` negates X
/// and then rotates, which is not the same as rotating and then negating.
///
/// Conventions transcribed from `geom.h`: `rotate90` is `(x, y) -> (-y, x)`, `rotate180` is
/// `(-x, -y)`, `rotate270` is `(y, -x)`.
pub fn transform_point(p: (i32, i32), orient: DbOrient, offset: (i32, i32)) -> (i32, i32) {
    let (x, y) = p;
    let (x, y) = match orient {
        DbOrient::R0 => (x, y),
        DbOrient::R90 => (-y, x),
        DbOrient::R180 => (-x, -y),
        DbOrient::R270 => (y, -x),
        DbOrient::MY => (-x, y),
        // ⚠️ Mirror FIRST, then rotate — `p.setX(-p.x())` precedes `p.rotate90()`.
        DbOrient::MYR90 => (-y, -x),
        DbOrient::MX => (x, -y),
        DbOrient::MXR90 => (y, x),
    };
    (x + offset.0, y + offset.1)
}

/// Upstream `dbTransform::apply(Rect&)`.
///
/// ⛔ **Both CORNERS are transformed independently and the result is RE-NORMALISED.** `Rect::init`
/// orders the coordinates, so a mirror that sends the lower-left above the upper-right still yields
/// a well-formed box. Transforming the corners and keeping them in place would give a box with
/// negative extent on every mirrored orientation.
pub fn transform_rect(
    r: (i32, i32, i32, i32),
    orient: DbOrient,
    offset: (i32, i32),
) -> (i32, i32, i32, i32) {
    let ll = transform_point((r.0, r.1), orient, offset);
    let ur = transform_point((r.2, r.3), orient, offset);
    (ll.0.min(ur.0), ll.1.min(ur.1), ll.0.max(ur.0), ll.1.max(ur.1))
}

/// Upstream `dbITerm::getAvgXY`.
///
/// ⛔ **It is NOT the centre of the pin's bounding box.** Every geometry box of every MPin
/// contributes BOTH its min and its max to a running sum, and the divisor is `2 x boxes` — so a
/// terminal whose geometry is split across several boxes is weighted by how many boxes it has, not
/// by area or by extent. Two boxes far apart and one box spanning them give different answers.
///
/// ⚠️ **The average is accumulated in `double` and truncated to `int` at the end** — `int(xx)`,
/// which truncates TOWARD ZERO rather than flooring. Negative coordinates round the other way.
///
/// ⚠️ **`None` when the terminal has no geometry at all**, which upstream reports as ODB-34 and
/// treats as "no position" — the terminal then contributes nothing to the net's box rather than
/// contributing the origin.
pub fn iterm_avg_xy(
    boxes: &[(i32, i32, i32, i32)],
    orient: DbOrient,
    offset: (i32, i32),
) -> Option<(i32, i32)> {
    if boxes.is_empty() {
        return None;
    }
    let (mut xx, mut yy, mut nn) = (0.0f64, 0.0f64, 0i64);
    for &b in boxes {
        let r = transform_rect(b, orient, offset);
        xx += r.0 as f64 + r.2 as f64;
        yy += r.1 as f64 + r.3 as f64;
        nn += 2;
    }
    Some(((xx / nn as f64) as i32, (yy / nn as f64) as i32))
}

/// The bounding box of a terminal's geometry, transformed — `dbITerm::getBBox()`.
///
/// ⚠️ **Distinct from [`iterm_avg_xy`], and the flip wirelength uses BOTH**: the average positions
/// the terminal on its net, while this box's CENTRE is the point an unplaced IO pin measures its
/// nearest region from. They coincide only for a single-box terminal.
pub fn iterm_bbox(
    boxes: &[(i32, i32, i32, i32)],
    orient: DbOrient,
    offset: (i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    let mut merged: Option<(i32, i32, i32, i32)> = None;
    for &b in boxes {
        let r = transform_rect(b, orient, offset);
        merged = Some(match merged {
            None => r,
            Some(m) => (m.0.min(r.0), m.1.min(r.1), m.2.max(r.2), m.3.max(r.3)),
        });
    }
    merged
}

/// Upstream `ClusteringEngine::mapMacroInCluster2HardMacro`, which is what `getHardMacros()`
/// returns for the rest of the run.
///
/// ⛔ **A cluster's hard macros are its own LEAF MACROS *plus* every macro reachable through the
/// `dbModule`s it holds**, walked recursively into child module instances. It is NOT the union over
/// child CLUSTERS, and it is not the leaf list alone.
///
/// 🔑 **This is why an all-macro ROOT has every macro.** The root holds the top module, so the
/// module walk finds all of them even though `leaf_macros` is empty and each macro lives in its own
/// child cluster. `placeMacros` then places them against the root's outline, and the orientation
/// pass groups them into the root's own columns and rows — which is exactly the four extra lines
/// per pass `macro_only` emits.
///
/// ⚠️ **A StdCellCluster returns nothing at all**, before either source is consulted.
///
/// ⚠️ Order matters and is upstream's: the leaf macros first, in their own order, then the module
/// walk. The FIRST entry is what the flip trace reports and what the push threshold reads.
pub fn cluster_hard_macros(
    cluster_type: crate::cluster::ClusterType,
    leaf_macros: &[usize],
    db_modules: &[usize],
    module_insts: &dyn Fn(usize) -> Vec<usize>,
    module_children: &dyn Fn(usize) -> Vec<usize>,
    is_macro: &dyn Fn(usize) -> bool,
) -> Vec<usize> {
    if cluster_type == crate::cluster::ClusterType::StdCell {
        return Vec::new();
    }
    let mut out: Vec<usize> = leaf_macros.to_vec();
    fn walk(
        module: usize,
        out: &mut Vec<usize>,
        module_insts: &dyn Fn(usize) -> Vec<usize>,
        module_children: &dyn Fn(usize) -> Vec<usize>,
        is_macro: &dyn Fn(usize) -> bool,
    ) {
        for inst in module_insts(module) {
            if is_macro(inst) {
                out.push(inst);
            }
        }
        for child in module_children(module) {
            walk(child, out, module_insts, module_children, is_macro);
        }
    }
    for &module in db_modules {
        walk(module, &mut out, module_insts, module_children, is_macro);
    }
    // ⚠️ Upstream cannot produce a duplicate here — a macro is either a leaf of the cluster or
    // inside one of its modules, never both — but the two sources are independent in our shape, so
    // the invariant is asserted rather than assumed.
    debug_assert!(
        {
            let mut seen = out.clone();
            seen.sort_unstable();
            seen.dedup();
            seen.len() == out.len()
        },
        "a macro reached the hard-macro list twice"
    );
    out
}

/// One hard macro, as `correctAllMacrosOrientation` sees it.
///
/// ⛔ **Both coordinate systems are carried, because ONE TRACE LINE USES BOTH.** The columns and
/// rows are keyed by `getRealX`/`getRealY` — the halo taken OFF — and the line then reports
/// `macros.front()->getX()`/`getY()`, with the halo ON. Carrying one and deriving the other by a
/// constant offset agrees on every design whose macros share a halo and disagrees exactly on the
/// ones where `set_macro_halo` named a single macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlipMacro {
    pub name: String,
    /// ⛔ **`HardMacro::getCluster()`'s name — the MACRO's own cluster, which is NOT necessarily the
    /// cluster whose group is being flipped.** The trace line prints
    /// `macros.front()->getCluster()->getName()`, so the ROOT's own column groups are reported
    /// under the LEAF cluster each macro belongs to, not under `root`.
    ///
    /// 🔑 **Which cluster that is comes from a call ORDER.** `mapMacroInCluster2HardMacro` runs over
    /// `id_to_cluster` — ascending id — and ends with `hard_macro->setCluster(cluster)`, so the
    /// HIGHEST-ID cluster holding a macro owns it. The root has the lowest id and always loses.
    pub cluster_name: String,
    /// The HALOED lower-left — `getX`/`getY`. What the trace prints.
    pub location: (i32, i32),
    /// The REAL lower-left — `getRealX`/`getRealY`. What the grouping keys on.
    pub real_location: (i32, i32),
}

/// One cluster the orientation pass visits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlipCluster {
    pub id: i32,
    pub name: String,
    /// Indices into the flat macro list, in `getHardMacros()` order.
    pub macros: Vec<usize>,
}

const FLIP_DEBUG: &str = "[DEBUG MPL-flipping] ";

/// Upstream `correctMacroOrientationByCluster`, composed — the trace it prints.
///
/// 🔑 **Every group is tried VERTICALLY first, and only then is every group tried horizontally.**
/// Two full passes over the columns and then the rows, not two flips per group. See [`FLIP_PASSES`].
///
/// ⛔ **The clusters arrive in ASCENDING ID**, because upstream walks `tree_->maps.id_to_cluster`,
/// a `std::map<int, Cluster*>`. ⚠️ That is NOT `fetchMacroClusters`' depth-first order, which the
/// boundary push uses — the two coincide on every design in the suite because ids are handed out in
/// creation order, and would not on a deeper tree. Measured, not assumed.
///
/// ⛔ **THE ROOT IS ITERATED TOO.** It is just another entry in `id_to_cluster`, and in an all-macro
/// design `runCoarseShaping` types it `HardMacroCluster` under MPL-27 — so it holds EVERY macro and
/// contributes its own column and row groups on top of the per-macro clusters. `macro_only` has ten
/// macros and emits **fourteen** lines per pass: four from the root's four columns, ten from the
/// singles. Any model that visits only the leaf clusters is short by exactly the root's groups.
///
/// ⚠️ **A flip does not move the macro.** `flipRealMacro` sets the orientation and then re-writes
/// the instance location from `getRealLocation()`, leaving `HardMacro::x_`/`y_` untouched — so the
/// reported coordinate is stable across both passes and the groups stay valid throughout.
///
/// ⚠️ **The wirelengths are `f32` printed with `{}`, and that is deliberate.**
/// `calculateRealMacroWirelength` returns a `float` and the reference logs it the same way, so an
/// integral value carries NO decimal point — every captured line reads `orig_WL 0`, not `0.0`.
/// Rust's `{}` on an `f32` already omits it, so no helper is needed; a formatter that forced a
/// precision would differ on all 135 lines.
///
/// The caller supplies the wirelength of a group in its current orientation; returning the same
/// value twice means the flip is kept, since [`keep_flip`] treats a tie as an improvement.
pub fn run_orientation_by_cluster(
    clusters: &[FlipCluster],
    macros: &[FlipMacro],
    wirelength_of: &mut dyn FnMut(&[usize], bool) -> (f32, f32),
) -> Vec<String> {
    let mut out = Vec::new();
    for cluster in clusters {
        let entries: Vec<(usize, (i32, i32))> =
            cluster.macros.iter().map(|&i| (i, macros[i].real_location)).collect();
        let (cols, rows) = orientation_groups(&entries);

        for &is_vertical in FLIP_PASSES.iter() {
            let groups = if is_vertical { &cols } else { &rows };
            for group in groups {
                // ⚠️ Upstream dereferences `macros.front()` unconditionally; a `std::map` never
                // holds an empty vector, so this cannot arise.
                let Some(&first) = group.first() else { continue };
                let (original, new) = wirelength_of(group, is_vertical);
                let at = if is_vertical {
                    macros[first].location.0
                } else {
                    macros[first].location.1
                };
                out.push(format!(
                    "{FLIP_DEBUG}Cluster {} {} flip at {} orig_WL {} new_WL {}",
                    // ⛔ The MACRO's cluster, not the one being iterated. See `FlipMacro`.
                    macros[first].cluster_name,
                    if is_vertical { "column-wise (V)" } else { "row-wise (H)" },
                    at,
                    original,
                    new
                ));
            }
        }
    }
    out
}

/// Upstream `correctMacroOrientationSingle`.
///
/// ⛔ **TWO FULL PASSES over EVERY macro**, not two flips per macro — the same shape as the
/// by-cluster path, with each macro its own group of one.
///
/// ⚠️ **The trace line is a DIFFERENT one**: `Inst {} flip {V|H} …`, naming the macro rather than a
/// cluster and carrying no coordinate. ℹ️ Only ONE design in the reference suite reaches this path —
/// `halos5`, the only case that sets `-use_full_halo` — so a gate that is green without it says
/// nothing about this function.
pub fn run_orientation_single(
    macros: &[FlipMacro],
    unfixed: &[usize],
    wirelength_of: &mut dyn FnMut(usize, bool) -> (f32, f32),
) -> Vec<String> {
    let mut out = Vec::new();
    for &is_vertical in FLIP_PASSES.iter() {
        for &index in unfixed {
            let (original, new) = wirelength_of(index, is_vertical);
            out.push(format!(
                "{FLIP_DEBUG}Inst {} flip {} orig_WL {} new_WL {}",
                macros[index].name,
                if is_vertical { "V" } else { "H" },
                original,
                new
            ));
        }
    }
    out
}

// ---------------------------------------------------------------- committing to the database

/// What `updateMacroOnDb` writes, in the order it writes it.
///
/// ⛔ **ORIENTATION BEFORE LOCATION, and upstream says why at the site**: setting the orientation
/// mirrors the macro about an axis, which moves its lower-left corner — so the location must be
/// written afterwards to put it back. Writing them the other way round leaves every flipped macro
/// misplaced by its own width or height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacroCommit {
    pub orientation_first: bool,
    pub location: (i32, i32),
    /// ⚠️ **`PLACED`, not `LOCKED`** — upstream says why: orientation improvement runs next and
    /// needs the macros still movable. The lock comes later, in the final commit.
    pub locked: bool,
}

/// Upstream `updateMacroOnDb`.
///
/// ⚠️ **A fixed instance is skipped entirely** — not written, not locked.
///
/// ⚠️ **The location is the macro's REAL one** — without the halo. The halo-inclusive box is what
/// the blockage uses, later and separately.
pub fn commit_macro(inst_is_fixed: bool, real_location: (i32, i32)) -> Option<MacroCommit> {
    if inst_is_fixed {
        return None;
    }
    Some(MacroCommit { orientation_first: true, location: real_location, locked: false })
}

/// The halo a macro carries, as the final commit sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaloKind {
    None,
    Soft,
    Hard,
}

/// Upstream `commitMacroPlacementToDb`'s blockage rule.
///
/// ⛔ **A SOFT halo gets NO blockage.** Upstream says why: other tools capable of placement are
/// already aware of soft halos, so a blockage would be redundant. A hard halo, or none at all, does
/// get one.
///
/// ⛔ **And the blockage is created for EVERY macro, FIXED OR NOT.** The `isFixed` test guards only
/// the snap and the lock; the blockage block sits outside it. So a fixed macro is never snapped and
/// never locked, and still casts a blockage.
pub fn needs_halo_blockage(halo: HaloKind) -> bool {
    !matches!(halo, HaloKind::Soft)
}

/// What the final commit does to one macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalCommit {
    pub snapped: bool,
    pub locked: bool,
    pub blockage: bool,
}

/// Upstream `commitMacroPlacementToDb`, per macro.
///
/// ⚠️ **The blockage is taken from the SNAPPED location**, because `setRealLocation` is called with
/// the instance's position after the snap and the box is read from the macro afterwards. Computing
/// it before the snap would place every blockage a fraction off its macro.
///
/// ⚠️ The box is the macro's halo-INCLUSIVE one — `HardMacro`'s default coordinates include the
/// halo — so the blockage covers the keep-out, not just the macro.
pub fn final_commit(inst_is_fixed: bool, halo: HaloKind) -> FinalCommit {
    FinalCommit {
        snapped: !inst_is_fixed,
        locked: !inst_is_fixed,
        // ⛔ Outside the `isFixed` guard; see `needs_halo_blockage`.
        blockage: needs_halo_blockage(halo),
    }
}

// ---------------------------------------------------------------- temporary macro clusters

/// One temporary cluster, as `placeMacros` needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempMacroCluster {
    /// ⚠️ Named after the MACRO, not after the parent cluster.
    pub name: String,
    /// From the clustering engine's running counter — see [`temp_macro_clusters`].
    pub cluster_id: i32,
    /// The macro's index in the annealer's list. ⚠️ Equal to its position in the input.
    pub macro_id: usize,
}

/// What `createTempMacroClusters` produces.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TempMacroClusters {
    pub clusters: Vec<TempMacroCluster>,
    /// ⚠️ The DISTINCT masters among the macros — this count is what scales the exchange
    /// probability in [`macro_placement_probabilities`], so it is part of the search, not
    /// bookkeeping.
    pub distinct_masters: usize,
    /// The counter's value after the run. ⛔ See below: it does NOT rewind.
    pub next_cluster_id: i32,
}

/// Upstream `ClusteringEngine::createTempMacroClusters`.
///
/// 🔑 **One temporary cluster per hard macro**, so macro placement can reuse the cluster-shaped
/// machinery — connections, terminals, nets — on individual macros. They exist only for the
/// duration of one `placeMacros` call.
///
/// ⛔ **The ids come from the ENGINE's shared counter and are never given back.** The clusters are
/// destroyed when macro placement finishes and `clearTempMacroClusterMapping` removes their raw
/// pointers from the id map — upstream says why: otherwise they would be deleted twice — but the
/// counter itself does not rewind. So every macro cluster placed permanently consumes as many ids
/// as it has macros, and the ids a later cluster's temporaries receive depend on how many macros
/// every earlier cluster held.
///
/// ⚠️ **`macro_id` is taken BEFORE the macro is appended**, so it is the index the macro will
/// occupy — which makes it equal to the position in the input list.
///
/// ⚠️ The masters set is a `PtrSet`, so it counts DISTINCT masters; the order macros are visited in
/// does not change its size.
pub fn temp_macro_clusters(macro_names: &[String], masters: &[usize], first_id: i32) -> TempMacroClusters {
    let mut out = TempMacroClusters { next_cluster_id: first_id, ..Default::default() };
    let mut distinct: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();

    for (index, name) in macro_names.iter().enumerate() {
        out.clusters.push(TempMacroCluster {
            name: name.clone(),
            cluster_id: out.next_cluster_id,
            // ⚠️ `sa_macros.size()` read before the push — the index this macro will take.
            macro_id: index,
        });
        if let Some(master) = masters.get(index) {
            distinct.insert(*master);
        }
        out.next_cluster_id += 1;
    }

    out.distinct_masters = distinct.len();
    out
}

// ---------------------------------------------------------------- snapping to the track grid

/// Which axis a snap pass constrains.
///
/// ⚠️ **A VERTICAL pass constrains X**, because vertical routing layers have vertical tracks whose
/// positions are X coordinates. Reading "vertical" as "moves the macro vertically" gets both passes
/// backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapAxis {
    /// Vertical layers, vertical tracks, X positions.
    Vertical,
    /// Horizontal layers, horizontal tracks, Y positions.
    Horizontal,
}

/// Upstream `Snapper::snapMacro`: ⚠️ **vertical first, then horizontal** — X settled before Y. The
/// two passes are independent, so the order is not load-bearing for the result, but it is what the
/// reference does and what its trace shows.
pub const SNAP_PASSES: [SnapAxis; 2] = [SnapAxis::Vertical, SnapAxis::Horizontal];

/// Upstream `Snapper::getPinOffset`: where a pin's centre sits relative to the macro's origin.
///
/// ⛔ **It mixes two different boxes.** The half-width comes from the PLACED pin's bounding box,
/// and the distance to the origin from the MASTER TERMINAL's — one is in die coordinates and one
/// is in the master's. They agree for an unrotated macro, which is why it works.
///
/// ⛔ **Only THREE of the eight orientations negate, and they differ per axis**: `MY` and `R180`
/// on the vertical pass, `MX` and `R180` on the horizontal one. **The four rotated orientations —
/// `R90`, `R270`, `MXR90`, `MYR90` — are not handled at all** and take the unnegated offset.
///
/// ⚠️ The half-width is an INTEGER division, so an odd pin width loses its half unit.
pub fn pin_offset(
    pin_width: i32,
    mterm_min: i32,
    orient: crate::halo::Orient,
    axis: SnapAxis,
) -> i32 {
    use crate::halo::Orient;
    let offset = mterm_min + (pin_width / 2);
    let negates = match axis {
        SnapAxis::Vertical => matches!(orient, Orient::My | Orient::R180),
        SnapAxis::Horizontal => matches!(orient, Orient::Mx | Orient::R180),
    };
    if negates {
        -offset
    } else {
        offset
    }
}

/// Upstream `Snapper::alignWithManufacturingGrid`.
///
/// ⚠️ **`std::round`, which rounds half AWAY FROM ZERO** — not to even, and not toward zero. The
/// division is in `f64` and the product is narrowed back to an `int`.
///
/// ⛔ **A grid of zero divides by zero.** Upstream does not guard it; a technology without a
/// manufacturing grid would produce a NaN and then an unspecified integer.
pub fn align_with_manufacturing_grid(origin: i32, manufacturing_grid: i32) -> i32 {
    ((origin as f64 / manufacturing_grid as f64).round() * manufacturing_grid as f64) as i32
}

/// Upstream `Snapper::snapPinToPosition`.
///
/// ⛔ **The manufacturing-grid alignment happens AFTER the track is chosen, and can undo it.** The
/// origin is placed so the pin lands exactly on a track, and is then rounded to the manufacturing
/// grid — which moves the pin off that track whenever the track pitch is not a multiple of the
/// grid. The snap targets the track; the grid has the last word.
pub fn snap_origin_to_position(
    position: i32,
    pin_offset: i32,
    manufacturing_grid: i32,
) -> i32 {
    align_with_manufacturing_grid(position - pin_offset, manufacturing_grid)
}

/// Upstream `snap`'s choice of track.
///
/// ⛔ **It takes the first track AT OR AFTER the pin centre, not the NEAREST one.** A pin sitting
/// just past a track is moved FORWARD to the next, never back — so the snap is biased in one
/// direction along the axis.
///
/// ⚠️ **A pin past the last track steps BACK to it**, which is the only case that ever moves
/// backwards.
///
/// ℹ️ `None` when there are no tracks at all; upstream's caller handles that case before reaching
/// here, by aligning to the manufacturing grid alone.
pub fn starting_position_index(positions: &[i32], pin_center: i32) -> Option<usize> {
    if positions.is_empty() {
        return None;
    }
    let index = positions.partition_point(|&p| p < pin_center);
    Some(if index == positions.len() { index - 1 } else { index })
}

/// Upstream `attemptSnapToExtraPatterns`' candidate order.
///
/// 🔑 **An alternating outward spiral from the starting index, POSITIVE first**: `0, +1, -1, +2,
/// -2, …`. So a track just after the pin is preferred to the equally-distant one just before it.
///
/// ⚠️ **101 attempts, not 100** — the loop is `i <= total_attempts`, so it reaches ±50.
pub fn spiral_step(i: i32) -> i32 {
    if i % 2 == 1 {
        (i + 1) / 2
    } else {
        -(i / 2)
    }
}

/// Upstream `Snapper::totalAlignedPins`, for one layer.
///
/// 🔑 **A two-pointer merge over two ASCENDING lists** — the pins sorted by centre in
/// `computeLayerDataList`, and the track positions from the grid. A pin that falls short of the
/// current track can never align with any later one, so it is dropped and the pin pointer advances;
/// a track that falls short of the current pin is skipped.
///
/// ⛔ **It depends on both lists being sorted**, and nothing here re-checks that. Feeding it
/// unsorted pins silently undercounts rather than failing.
///
/// ⛔ **A pin past the LAST track is never examined.** The loop ends as soon as the track pointer
/// runs out, so unaligned pins at the high end are silently skipped — including for the
/// `RightWayOnGridOnly` error, which is only ever raised from the falls-short branch.
pub fn aligned_pins_on_layer(pin_centers: &[i32], positions: &[i32]) -> usize {
    let mut aligned = 0;
    let (mut i, mut j) = (0usize, 0usize);
    while i < pin_centers.len() && j < positions.len() {
        match pin_centers[i].cmp(&positions[j]) {
            std::cmp::Ordering::Equal => {
                aligned += 1;
                i += 1;
            }
            // ⛔ This pin cannot align with any LATER track either, so it is dropped here — and
            // this is the only branch that ever raises the RightWayOnGridOnly error.
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    aligned
}

/// The result of the extra-pattern search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapSearch {
    pub best_index: usize,
    pub best_aligned: usize,
    /// ⚠️ Upstream warns MPL-2 when not every pin could be aligned, and re-runs the count with the
    /// error flag set so a `RightWayOnGridOnly` layer raises MPL-5 first.
    pub all_aligned: bool,
}

/// Upstream `Snapper::attemptSnapToExtraPatterns`.
///
/// ⛔ **`>`, strictly, from a starting best of ZERO.** So the starting index is not privileged: it
/// is evaluated like any other candidate at step 0, and a later candidate must strictly beat it.
/// But if NO candidate aligns a single pin, the search falls back to the starting index with a
/// score of zero rather than to whatever it tried last.
///
/// ⚠️ **An out-of-range candidate is SKIPPED, not a stopping condition** — the spiral keeps
/// stepping past one end of the track list and continues to explore the other.
///
/// ⚠️ **The macro must be re-snapped to the winner afterwards**, because the loop leaves it at the
/// last candidate tried rather than the best one. That is a real step, not bookkeeping.
pub fn search_extra_patterns(
    start_index: usize,
    position_count: usize,
    total_pins: usize,
    aligned_for: &mut dyn FnMut(usize) -> usize,
) -> SnapSearch {
    const TOTAL_ATTEMPTS: i32 = 100;
    let mut best_index = start_index;
    let mut best_aligned = 0usize;

    for i in 0..=TOTAL_ATTEMPTS {
        let current = start_index as i64 + spiral_step(i) as i64;
        if current < 0 || current >= position_count as i64 {
            continue;
        }
        let aligned = aligned_for(current as usize);
        if aligned > best_aligned {
            best_aligned = aligned;
            best_index = current as usize;
            if best_aligned == total_pins {
                break;
            }
        }
    }

    SnapSearch { best_index, best_aligned, all_aligned: best_aligned == total_pins }
}

/// One `(iterm, mpin)` pairing, as `computeLayerDataList` sees it before filtering.
///
/// ⛔ **One entry per MPIN, not per terminal.** A terminal with several master pins on matching
/// layers is pushed into the list once for EACH of them — so it appears more than once, inflating
/// the total pin count and counting more than once towards alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapPinCandidate {
    pub iterm: usize,
    /// ⚠️ Only `SIGNAL` terminals are considered — power and ground are skipped.
    pub is_signal: bool,
    /// ⚠️ The layer of the pin's FIRST geometry. A master pin with geometry on several layers is
    /// judged by whichever the iterator yields first, not by the lowest or the widest.
    pub layer: usize,
    pub layer_number: i32,
    pub layer_is_vertical: bool,
    pub has_track_grid: bool,
    pub center: i32,
}

/// One layer's worth of snapping data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapLayerData {
    pub layer: usize,
    pub layer_number: i32,
    /// The terminals on this layer, **sorted by centre**.
    pub pins: Vec<usize>,
}

/// A layer with no track grid — upstream MPL-39.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingTrackGrid(pub usize);

/// Upstream `Snapper::computeLayerDataList`, less the database.
///
/// 🔑 **The layers come out sorted by LAYER NUMBER**, and that sort is what makes the result
/// deterministic: upstream groups the pins in a `std::map` keyed by track-grid POINTER, whose
/// iteration order is the pointers' and therefore arbitrary. The sort washes that out. Grouping
/// without sorting would be a different answer on every run.
///
/// 🔑 **`layers_data[0].pins[0]` is what drives the whole snap** — the lowest layer's lowest-centred
/// pin. Everything else is only scored against it.
///
/// ⛔ **A terminal appears once per matching MPIN.** Two master pins on the same layer put the same
/// terminal in the list twice, and both count.
///
/// ⚠️ **Only pins whose layer runs in the pass's direction are kept** — a vertical pass sees only
/// pins on vertical layers.
///
/// ⚠️ **The pin sort is not stable and pins CAN share a centre**, so which of two equally-centred
/// pins ends up first is unspecified. That matters: if the tie is at position 0 of the lowest
/// layer, the two pins can have different master-terminal offsets and so snap the macro to
/// different origins.
pub fn snap_layer_data(
    candidates: &[SnapPinCandidate],
    axis: SnapAxis,
) -> Result<Vec<SnapLayerData>, MissingTrackGrid> {
    let want_vertical = axis == SnapAxis::Vertical;
    let mut grouped: std::collections::BTreeMap<usize, (i32, Vec<(i32, usize)>)> =
        std::collections::BTreeMap::new();

    for c in candidates {
        if !c.is_signal {
            continue;
        }
        if c.layer_is_vertical != want_vertical {
            continue;
        }
        if !c.has_track_grid {
            return Err(MissingTrackGrid(c.layer));
        }
        grouped.entry(c.layer).or_insert((c.layer_number, Vec::new())).1.push((c.center, c.iterm));
    }

    let mut out: Vec<SnapLayerData> = grouped
        .into_iter()
        .map(|(layer, (layer_number, mut pins))| {
            // ⚠️ Sorted by CENTRE. Upstream's comparator looks at the centre alone, so equal
            // centres are left in an unspecified order.
            pins.sort_by_key(|(center, _)| *center);
            SnapLayerData {
                layer,
                layer_number,
                pins: pins.into_iter().map(|(_, iterm)| iterm).collect(),
            }
        })
        .collect();

    // 🔑 By LAYER NUMBER — the sort that makes the pointer-keyed grouping deterministic.
    out.sort_by_key(|d| d.layer_number);
    Ok(out)
}

// ---------------------------------------------------------------- flipping a placed macro

/// Upstream `dbOrientType::flipX` and `flipY`, over the orientations `mpl` can reach.
///
/// ⛔ **A "vertical flip" calls `flipY`** — mirroring about the vertical (Y) axis, which moves the
/// macro HORIZONTALLY. The name describes the mirror line, not the direction of travel, and it is
/// the opposite of the reading most people arrive at.
///
/// ⚠️ The four rotated orientations map among themselves and are outside what `mpl` produces; they
/// are folded into `Other` here, which flips to itself. ⛔ That is a divergence: upstream maps
/// `R90 → MYR90` under `flipX`, and a rotated macro reaching this path would take a different
/// orientation than we give it. Nothing in `mpl` rotates a macro.
pub fn flip_orientation(orient: crate::halo::Orient, is_vertical_flip: bool) -> crate::halo::Orient {
    use crate::halo::Orient;
    if is_vertical_flip {
        // `flipY`: mirror about the vertical axis.
        match orient {
            Orient::R0 => Orient::My,
            Orient::My => Orient::R0,
            Orient::R180 => Orient::Mx,
            Orient::Mx => Orient::R180,
            Orient::Other => Orient::Other,
        }
    } else {
        // `flipX`: mirror about the horizontal axis.
        match orient {
            Orient::R0 => Orient::Mx,
            Orient::Mx => Orient::R0,
            Orient::R180 => Orient::My,
            Orient::My => Orient::R180,
            Orient::Other => Orient::Other,
        }
    }
}

/// Upstream `flipRealMacro`.
///
/// 🔑 **The location is re-applied AFTER the orientation.** Setting an orientation mirrors the
/// macro about an axis, which moves its lower-left corner — so the real location is written back to
/// put it where macro placement wanted it. Omitting that leaves every flipped macro displaced by
/// its own width or height.
///
/// ⚠️ Both the instance and the `HardMacro` record the new orientation; they are separate stores.
pub fn flip_real_macro(
    orient: crate::halo::Orient,
    real_location: (i32, i32),
    is_vertical_flip: bool,
) -> (crate::halo::Orient, (i32, i32)) {
    (flip_orientation(orient, is_vertical_flip), real_location)
}

/// One terminal on a net, as the real-wirelength model sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetTerminal {
    /// An instance pin, at its average pin position. ⚠️ A pin with no geometry yields nothing and
    /// is skipped entirely.
    Instance(Option<(i32, i32)>),
    /// A block terminal whose placement is FIXED — its bounding box's CENTRE.
    FixedPin((i32, i32)),
    /// A block terminal that is NOT fixed. ⛔ **Its own position is ignored**; what counts is the
    /// nearest point of its CONSTRAINT REGION, or of the available regions when it has none.
    /// Upstream says so at the site.
    UnplacedPin((i32, i32)),
}

/// The bounding box a net contributes, built by merging one point per terminal.
///
/// ⚠️ **Every terminal is a POINT, never a box** — even a fixed block terminal contributes its
/// centre rather than its extent. So the box is the spread of the terminals, not of their shapes.
///
/// ℹ️ `None` when nothing merged, which is upstream's un-merged rect.
pub fn net_terminal_bbox(terminals: &[NetTerminal]) -> Option<(i32, i32, i32, i32)> {
    let mut merged: Option<(i32, i32, i32, i32)> = None;
    for t in terminals {
        let point = match t {
            NetTerminal::Instance(Some(p)) => *p,
            NetTerminal::Instance(None) => continue,
            NetTerminal::FixedPin(p) | NetTerminal::UnplacedPin(p) => *p,
        };
        merged = Some(match merged {
            None => (point.0, point.1, point.0, point.1),
            Some(m) => (m.0.min(point.0), m.1.min(point.1), m.2.max(point.0), m.3.max(point.1)),
        });
    }
    merged
}

/// Upstream `calculateRealMacroWirelength`.
///
/// ⛔ **A net is counted ONCE PER PIN OF THIS MACRO ON IT.** The loop walks the macro's own pins and
/// adds each pin's whole net half-perimeter — so a macro with two pins on one net contributes that
/// net twice. This is a comparison between two orientations of the same macro, so the doubling
/// cancels; it is still not the wirelength of anything.
///
/// ⚠️ **Only SIGNAL pins**, and a pin with no net contributes nothing.
///
/// ⚠️ Accumulated in `int64_t` and returned as a `float`, so a large design loses precision at the
/// point of return rather than during the sum.
pub fn real_macro_wirelength(nets_of_macro_pins: &[Vec<NetTerminal>]) -> f32 {
    let mut wirelength: i64 = 0;
    for terminals in nets_of_macro_pins {
        if let Some(b) = net_terminal_bbox(terminals) {
            wirelength += (b.2 - b.0) as i64 + (b.3 - b.1) as i64;
        }
    }
    wirelength as f32
}

/// One terminal on a net, as the flip wirelength needs to position it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlipNetTerm {
    /// An instance pin: the instance's index, and its terminal name.
    Instance { inst: usize, term: String },
    /// A block port: its index into the pin list.
    Port(usize),
}

/// What a block port contributes, once its kind is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlipPort {
    /// ⚠️ Upstream tests the FIRST pin's placement status, not the terminal's.
    pub is_fixed: bool,
    /// The port's bounding box — used ONLY when it is fixed.
    pub bbox: (i32, i32, i32, i32),
}

/// Upstream `calculateRealMacroWirelength`, assembled.
///
/// ⛔ **A net is counted ONCE PER PIN OF THIS MACRO ON IT.** The loop walks the macro's own
/// terminals and adds each one's whole net half-perimeter, so a macro with two pins on one net
/// contributes that net twice. This compares two orientations of the same macro, so the doubling
/// cancels — it is still not the wirelength of anything.
///
/// ⛔ **An unplaced port measures from THIS MACRO PIN's bbox centre**, not from the net's other
/// terminals and not from the macro's centre. The nearest point of its constraint region — or of
/// the available regions when it has none — is what joins the net box.
///
/// ⚠️ **Only the macro's own SIGNAL terminals start a net**, and a terminal with no net contributes
/// nothing. But once a net is chosen, EVERY terminal on it counts, signal or not.
///
/// ⚠️ **A terminal with no geometry contributes NOTHING** rather than contributing the origin —
/// `getAvgXY` returns false and the merge is skipped.
#[allow(clippy::too_many_arguments)]
pub fn flip_macro_wirelength(
    macro_pins: &[(String, usize)],
    net_terms: &dyn Fn(usize) -> Vec<FlipNetTerm>,
    inst_state: &dyn Fn(usize) -> Option<(DbOrient, (i32, i32))>,
    term_boxes: &dyn Fn(usize, &str) -> Vec<(i32, i32, i32, i32)>,
    port: &dyn Fn(usize) -> Option<FlipPort>,
    port_region: &dyn Fn(usize) -> Option<Region>,
    available_regions: &[Region],
    this_macro: usize,
) -> f32 {
    let mut nets: Vec<Vec<NetTerminal>> = Vec::new();
    for (term, net) in macro_pins {
        // The point an unplaced port measures its nearest region from: the CENTRE of this macro
        // pin's transformed bounding box.
        let from = inst_state(this_macro).and_then(|(orient, offset)| {
            iterm_bbox(&term_boxes(this_macro, term), orient, offset)
                .map(|b| ((b.0 + b.2) / 2, (b.1 + b.3) / 2))
        });
        let mut terminals = Vec::new();
        for t in net_terms(*net) {
            match t {
                FlipNetTerm::Instance { inst, term } => {
                    let point = inst_state(inst).and_then(|(orient, offset)| {
                        iterm_avg_xy(&term_boxes(inst, &term), orient, offset)
                    });
                    terminals.push(NetTerminal::Instance(point));
                }
                FlipNetTerm::Port(index) => {
                    let Some(p) = port(index) else { continue };
                    if p.is_fixed {
                        terminals.push(NetTerminal::FixedPin((
                            (p.bbox.0 + p.bbox.2) / 2,
                            (p.bbox.1 + p.bbox.3) / 2,
                        )));
                    } else {
                        let Some(from) = from else { continue };
                        let regions: Vec<Region> = match port_region(index) {
                            Some(r) => vec![r],
                            None => available_regions.to_vec(),
                        };
                        if let Some(point) = regions
                            .iter()
                            .map(|r| nearest_point_in_region(r, from))
                            .min_by_key(|p| {
                                (p.0 as i64 - from.0 as i64).abs()
                                    + (p.1 as i64 - from.1 as i64).abs()
                            })
                        {
                            terminals.push(NetTerminal::UnplacedPin(point));
                        }
                    }
                }
            }
        }
        nets.push(terminals);
    }
    real_macro_wirelength(&nets)
}

/// Where `updateMacroOnDb` puts a macro's INSTANCE ORIGIN, which is what the transform offsets by.
///
/// ⛔ **`getRealX`/`getRealY`, and they DEPEND ON THE ORIENTATION.** The halo comes off the left and
/// bottom at `R0`, but off the RIGHT at `R180`/`MY` and the TOP at `R180`/`MX`. So flipping a macro
/// with an asymmetric halo MOVES its instance origin even though `HardMacro::x_` never changes —
/// and every pin position moves with it.
///
/// ⚠️ A symmetric halo hides this completely, which is every design in the suite that does not set
/// a per-macro halo.
pub fn real_origin(
    haloed: (i32, i32),
    halo: (i32, i32, i32, i32),
    orient: DbOrient,
) -> (i32, i32) {
    let (left, bottom, right, top) = halo;
    let x = match orient {
        DbOrient::R180 | DbOrient::MY => haloed.0 + right,
        _ => haloed.0 + left,
    };
    let y = match orient {
        DbOrient::R180 | DbOrient::MX => haloed.1 + top,
        _ => haloed.1 + bottom,
    };
    (x, y)
}

/// The transform OFFSET that puts an instance's bounding box at a given lower-left corner.
///
/// ⛔ **`dbInst::getLocation` returns the BBOX's lower-left, NOT the transform's offset.** It reads
/// `bbox->rect.xMin()`, so `setLocation` positions the *box* and the origin is whatever makes that
/// true. For a mirrored or rotated master those differ — mirroring `0..w` about the origin gives
/// `-w..0`, so the offset must carry an extra `+w` to put the box back.
///
/// 🔑 **Getting this wrong is invisible at `R0` and wrong on every flip**, which is exactly the
/// shape that survives an unflipped measurement and fails the flipped one.
///
/// ⚠️ The master box is `(0, 0, w, h)` — LEF masters are anchored at their own origin.
pub fn instance_offset(
    bbox_min: (i32, i32),
    master: (i32, i32),
    orient: DbOrient,
) -> (i32, i32) {
    let t = transform_rect((0, 0, master.0, master.1), orient, (0, 0));
    (bbox_min.0 - t.0, bbox_min.1 - t.1)
}

/// Upstream `flipRealMacro`'s orientation change, over the eight database orientations.
///
/// ⛔ **A "vertical flip" is `flipY`** — a mirror about the vertical axis, which moves the macro
/// HORIZONTALLY. The name describes the mirror line, not the direction of travel.
pub fn flip_db_orientation(orient: DbOrient, is_vertical_flip: bool) -> DbOrient {
    use DbOrient::*;
    if is_vertical_flip {
        // flipY: mirror about Y.
        match orient {
            R0 => MY,
            MY => R0,
            R180 => MX,
            MX => R180,
            R90 => MXR90,
            MXR90 => R90,
            R270 => MYR90,
            MYR90 => R270,
        }
    } else {
        // flipX: mirror about X.
        match orient {
            R0 => MX,
            MX => R0,
            R180 => MY,
            MY => R180,
            R90 => MYR90,
            MYR90 => R90,
            R270 => MXR90,
            MXR90 => R270,
        }
    }
}

// ---------------------------------------------------------------- clustering data to the database

/// One cluster, as the group builder needs to see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCluster {
    pub name: String,
    pub kind: AreaKind,
    pub leaf_std_cells: Vec<usize>,
    pub leaf_macros: Vec<usize>,
    /// Every leaf instance of this cluster's database modules, and whether each is a block.
    pub module_insts: Vec<(usize, bool)>,
    pub children: Vec<usize>,
}

/// Upstream `createGroupForCluster`.
///
/// 🔑 **The recursion sits BETWEEN the two claiming phases, and that is the whole design.** A
/// cluster adds its own leaf cells and macros, then lets its CHILDREN claim theirs, and only then
/// sweeps its modules' leaf instances — skipping anything already claimed. Upstream says so at the
/// site: *"Skip if it is part of a child cluster."* Sweeping the modules before recursing would put
/// every descendant's instances in the ancestor's group instead.
///
/// ⛔ **An IO cluster gets NO group at all**, and its subtree is not visited either — the early
/// return happens before the group is created.
///
/// ⛔ **A macro is skipped by a STANDARD-CELL cluster's module sweep**, but not by its explicit
/// `leaf_macros` list. So a standard-cell cluster can still own a macro it names directly; what it
/// will not do is pick one up incidentally from a module.
///
/// ⚠️ **An instance is claimed at most once**, by whoever reaches it first — parent before child
/// for the explicit lists, child before parent for the module sweep.
///
/// ℹ️ Groups are created nested, mirroring the cluster tree, and typed `VISUAL_DEBUG`.
pub fn create_groups(clusters: &[GroupCluster], root: usize) -> Vec<(String, Vec<usize>)> {
    let mut claimed: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    create_group(clusters, root, &mut claimed, &mut out);
    out
}

fn create_group(
    clusters: &[GroupCluster],
    id: usize,
    claimed: &mut std::collections::BTreeSet<usize>,
    out: &mut Vec<(String, Vec<usize>)>,
) {
    let cluster = &clusters[id];
    // ⛔ Before the group exists — an IO cluster contributes nothing and is not descended into.
    if cluster.kind == AreaKind::IoCluster {
        return;
    }

    let mut members = Vec::new();
    for &inst in &cluster.leaf_std_cells {
        if claimed.insert(inst) {
            members.push(inst);
        }
    }
    for &inst in &cluster.leaf_macros {
        if claimed.insert(inst) {
            members.push(inst);
        }
    }

    // 🔑 The children claim theirs HERE, before this cluster sweeps its modules.
    let slot = out.len();
    out.push((cluster.name.clone(), Vec::new()));
    for &child in &cluster.children {
        create_group(clusters, child, claimed, out);
    }

    for &(inst, is_block) in &cluster.module_insts {
        if claimed.contains(&inst) {
            continue;
        }
        // ⛔ A standard-cell cluster does not pick up a macro from a module.
        if is_block && cluster.kind == AreaKind::StdCellCluster {
            continue;
        }
        claimed.insert(inst);
        members.push(inst);
    }

    out[slot].1 = members;
}

// ---------------------------------------------------------------- temporary standard-cell places

/// Upstream `setTemporaryStdCellLocation`.
///
/// ⛔ **EVERY standard cell of a cluster goes to the SAME point** — the cluster's centre, offset by
/// each cell's own half-size so its centre lands there. They all overlap, deliberately: upstream's
/// comment says this exists so the orientation step has somewhere to measure wirelength from, not
/// to be a placement.
///
/// ⚠️ **The cluster's centre is the SOFT MACRO's pin centre**, which is `x + 0.5 * width` truncated
/// — see [`pin_center`]. The cell's own half-extent is a separate integer division, so an odd cell
/// width loses its half unit again.
///
/// ⚠️ A cluster with no soft macro places nothing.
pub fn temporary_std_cell_location(
    cluster_soft_macro: Option<((i32, i32), (i32, i32))>,
    cell_extent: (i32, i32),
) -> Option<(i32, i32)> {
    let ((x, y), (width, height)) = cluster_soft_macro?;
    let center = (pin_center(x, width), pin_center(y, height));
    Some((center.0 - cell_extent.0 / 2, center.1 - cell_extent.1 / 2))
}

/// A cluster, as the temporary standard-cell placement walks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdCellPlacementCluster {
    pub is_leaf: bool,
    pub num_std_cell: i32,
    /// Every CORE instance reachable through this cluster's modules, including nested ones.
    /// ⚠️ **Non-core instances are skipped** — a pad or a block found in a module is not placed.
    pub module_core_insts: Vec<usize>,
    pub leaf_std_cells: Vec<usize>,
    pub children: Vec<usize>,
}

/// Upstream `generateTemporaryStdCellsPlacement`.
///
/// ⛔ **Only a LEAF with standard cells places anything.** A leaf with none places nothing, and a
/// non-leaf never places its own cells — it only recurses. So a mixed cluster's own standard cells
/// are placed by whichever leaf descendant owns them, never by the cluster itself.
///
/// ⚠️ **Both loops run for a placing leaf**: its modules' core instances first, then its own leaf
/// standard-cell list. A cell reachable both ways is placed twice, to the same point.
///
/// 🔑 Returns the instances placed, in visit order — which is the order the database sees the
/// writes, and the order a DEF comparison will read them back in.
pub fn temporary_std_cell_placement(
    clusters: &[StdCellPlacementCluster],
    root: usize,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    place_std_cells(clusters, root, &mut out);
    out
}

fn place_std_cells(
    clusters: &[StdCellPlacementCluster],
    id: usize,
    out: &mut Vec<(usize, usize)>,
) {
    let cluster = &clusters[id];
    if cluster.is_leaf && cluster.num_std_cell != 0 {
        // ⚠️ Modules first, then the explicit list.
        for &inst in &cluster.module_core_insts {
            out.push((inst, id));
        }
        for &inst in &cluster.leaf_std_cells {
            out.push((inst, id));
        }
        return;
    }
    for &child in &cluster.children {
        place_std_cells(clusters, child, out);
    }
}

// ---------------------------------------------------------------- the no-standard-cells reset

/// What `resetSAParameters` leaves behind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResetSaParameters {
    pub pos_swap: f32,
    pub neg_swap: f32,
    pub double_swap: f32,
    pub exchange_swap: f32,
    /// ⛔ **Zeroed** — a design with no standard cells never resizes a cluster.
    pub resize: f32,
    pub weights: crate::anneal::SoftWeights,
}

/// Upstream `HierRTLMP::resetSAParameters`.
///
/// ⛔ **Called ONLY when the design has no standard cells**, before coarse shaping — so it is what
/// makes a macros-only design behave differently from every other, not a general default.
///
/// 🔑 **It zeroes RESIZE**, which is the action that changes a cluster's shape. With no standard
/// cells there is no soft area to trade, so the annealer is left with the four ordering moves.
///
/// ⛔ **And it zeroes FENCE as well as boundary, notch and soft blockage.** The fence weight is
/// otherwise `10.0` from the command — this is the only path that turns it off.
///
/// ⚠️ The four swap probabilities are RE-SET to `0.2`, not left alone; if a caller had changed
/// them, this puts them back.
pub fn reset_sa_parameters(base: crate::anneal::SoftWeights) -> ResetSaParameters {
    let mut weights = base;
    weights.fence = 0.0;
    weights.boundary = 0.0;
    weights.notch = 0.0;
    weights.soft_blockage = 0.0;
    ResetSaParameters {
        pos_swap: 0.2,
        neg_swap: 0.2,
        double_swap: 0.2,
        exchange_swap: 0.2,
        resize: 0.0,
        weights,
    }
}

// ---------------------------------------------------------------- the reported wirelength

/// Upstream `HierRTLMP::computeWireLength`.
///
/// ℹ️ **Reporting only** — it emits one metric and changes nothing. The half-perimeter comes from
/// odb's own `WireLengthEvaluator`, which is not `mpl`'s to reproduce; what IS `mpl`'s is the name
/// and the unit.
///
/// ⚠️ **Reported in MICRONS**, through `dbuToMicrons`, so the metric is a `double` division and not
/// the database's own integer.
pub const WIRELENGTH_METRIC: &str = "macro_place__wirelength";

/// The value that metric carries.
pub fn reported_wirelength(hpwl_dbu: i64, dbu_per_micron: i32) -> f64 {
    hpwl_dbu as f64 / dbu_per_micron as f64
}

// ---------------------------------------------------------------- the placement driver

/// What one parent's placement produced, or why it did not happen.
#[derive(Debug, Clone, PartialEq)]
pub enum ParentOutcome {
    /// The parent was placed. Carries the winning run and the macros it settled on.
    Placed { run: SelectedRun, macros: Vec<crate::anneal::SoftMacro> },
    /// A hard-macro cluster: handed to macro placement, which returns the placed hard macros.
    ///
    /// ⚠️ In the CLUSTER's own coordinates — `placeMacros` shifts them onto the outline's corner
    /// only after the summary is written.
    MacroCluster { macros: Vec<crate::anneal::SoftMacro> },
    /// A hard-macro cluster that is FIXED: macro placement refuses it at its first line.
    FixedMacroCluster,
    /// A leaf — an IO cluster, a leaf standard-cell cluster, or a fixed macro.
    Leaf,
    /// ⛔ No run produced a valid solution: MPL-40 at the root, MPL-8 below it.
    NoValidSolution(crate::options::MplError),
}

/// One visit the driver made, in the order it made them.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementVisit {
    pub cluster: i32,
    pub outcome: ParentOutcome,
}

/// What the driver needs to know about one cluster to decide and to place it.
pub struct PlacementTree<'a> {
    pub kind: &'a dyn Fn(i32) -> AreaKind,
    pub is_fixed_macro: &'a dyn Fn(i32) -> bool,
    pub is_leaf: &'a dyn Fn(i32) -> bool,
    pub children: &'a dyn Fn(i32) -> Vec<i32>,
    /// The utilizations to try, already ramped — see [`utilization_list`].
    pub utilizations: &'a [f32],
    pub num_threads: usize,
}

/// Upstream `HierRTLMP::placeChildren`, as composition.
///
/// 🔑 **The recursion is DEPTH-FIRST and happens AFTER the parent is placed**, never before: a
/// child's outline is the shape its parent just chose for it, so placing the child first would
/// place it inside a box that does not exist yet.
///
/// ⚠️ **Every child is visited, including the ones that do nothing.** An IO cluster and a leaf
/// standard-cell cluster both reach `placeChildren` and return at its guards — they are not
/// filtered out by the caller. That is why the visit list is the honest record of what happened.
///
/// ⛔ **A parent that cannot be placed STOPS at that parent**: upstream raises MPL-40 or MPL-8,
/// which throws, so nothing below it is visited. Returning the error here rather than continuing is
/// what reproduces that.
///
/// ℹ️ `place_one` is the annealer: given a parent and a utilization index, it reports whether that
/// run reached a valid solution and, for the winner, what it settled on. Threading it through as a
/// callback is what lets the composition be exercised without the whole database beneath it.
pub fn place_children(
    tree: &PlacementTree,
    root: i32,
    place_one: &mut dyn FnMut(i32, usize, f32) -> Option<Vec<crate::anneal::SoftMacro>>,
    place_macros_one: &mut dyn FnMut(i32) -> Option<Vec<crate::anneal::SoftMacro>>,
    on_parent_placed: &mut dyn FnMut(i32, &[crate::anneal::SoftMacro]),
) -> Vec<PlacementVisit> {
    let mut visits = Vec::new();
    place_one_parent(
        tree,
        root,
        root,
        place_one,
        place_macros_one,
        on_parent_placed,
        &mut visits,
    );
    visits
}

fn place_one_parent(
    tree: &PlacementTree,
    cluster: i32,
    root: i32,
    place_one: &mut dyn FnMut(i32, usize, f32) -> Option<Vec<crate::anneal::SoftMacro>>,
    place_macros_one: &mut dyn FnMut(i32) -> Option<Vec<crate::anneal::SoftMacro>>,
    on_parent_placed: &mut dyn FnMut(i32, &[crate::anneal::SoftMacro]),
    visits: &mut Vec<PlacementVisit>,
) -> bool {
    let action = placement_action((tree.kind)(cluster), (tree.is_fixed_macro)(cluster), (tree.is_leaf)(cluster));
    match action {
        PlacementAction::PlaceMacros => {
            // Upstream `placeChildren`'s first branch: a hard-macro cluster is handed straight to
            // `placeMacros` and the walk does NOT descend into it.
            let outcome = match place_macros_one(cluster) {
                Some(macros) => ParentOutcome::MacroCluster { macros },
                // ⛔ MPL-10, which upstream raises rather than returning.
                None => ParentOutcome::NoValidSolution(crate::options::MplError::new(
                    10,
                    "Macro placement failed for macro cluster",
                )),
            };
            let failed = matches!(outcome, ParentOutcome::NoValidSolution(_));
            visits.push(PlacementVisit { cluster, outcome });
            return !failed;
        }
        PlacementAction::PlaceMacrosButRefused => {
            visits.push(PlacementVisit { cluster, outcome: ParentOutcome::FixedMacroCluster });
            return true;
        }
        PlacementAction::Nothing => {
            visits.push(PlacementVisit { cluster, outcome: ParentOutcome::Leaf });
            return true;
        }
        PlacementAction::PlaceChildren => {}
    }

    let mut winning_macros = None;
    let selected = select_run(
        tree.utilizations,
        tree.num_threads,
        // ⚠️ Validity of the UTILIZATION is the caller's to judge; the driver only sequences.
        &mut |_| true,
        &mut |index, utilization| match place_one(cluster, index, utilization) {
            Some(macros) => {
                winning_macros = Some(macros);
                true
            }
            None => false,
        },
    );

    match selected {
        Ok(run) => {
            let macros = winning_macros.unwrap_or_default();
            // ⛔ **BEFORE the recursion, exactly as upstream orders it.** `placeChildren` calls
            // `updateChildrenShapesAndLocations` and `updateChildrenRealLocation` and only THEN
            // loops into the children — so a macro-cluster child reads the outline this parent
            // just gave it. Deferring the write-back until the walk finishes would hand every
            // macro run a stale outline and look like a search bug.
            on_parent_placed(cluster, &macros);
            visits.push(PlacementVisit {
                cluster,
                outcome: ParentOutcome::Placed { run, macros },
            });
        }
        Err(NoValidSolution) => {
            // ⛔ Upstream's error THROWS, so nothing below this parent runs.
            visits.push(PlacementVisit {
                cluster,
                outcome: ParentOutcome::NoValidSolution(no_valid_solution_error(
                    cluster == root,
                    cluster,
                    "",
                )),
            });
            return false;
        }
    }

    // 🔑 Only now — a child's outline is the shape this parent just chose for it.
    for child in (tree.children)(cluster) {
        if !place_one_parent(
            tree,
            child,
            root,
            place_one,
            place_macros_one,
            on_parent_placed,
            visits,
        ) {
            return false;
        }
    }
    true
}

/// Upstream `HierRTLMP::run`'s reset, then `runHierarchicalMacroPlacement`'s two setup steps.
///
/// ⛔ **THE ORDER IS THE BEHAVIOUR, and the two steps disagree about soft blockage.**
/// `run()` calls `resetSAParameters()` — which zeroes fence, boundary, notch **and soft
/// blockage** — before coarse shaping; `runHierarchicalMacroPlacement` then calls
/// `adjustSoftBlockageWeight`, which sets soft blockage to half the outline weight when the tree
/// is one level deep. 🔑 The reference's own log says `Changing soft blockage weight from 0 to 50`
/// — the `0` is the reset's, and the `50` is the adjustment putting it back.
///
/// ⚠️ **So the reset's soft-blockage zero is INVISIBLE at one level and survives below it.**
/// Applying the two in the other order would agree on every design in the suite and differ on a
/// deeper tree.
///
/// ⚠️ **`adjustSoftBlockageWeight` runs BEFORE the tiny-cluster threshold**, and both run before
/// any placement — so every annealer in the design is built with the same adjusted weight.
///
/// ℹ️ The action shares come back too: the reset zeroes RESIZE, and the four swaps then normalise
/// over `0.8` rather than `1.2` — so a design with no standard cells has different swap
/// probabilities as well as no resize. Coarse shaping already models this half in
/// `ShapingCtx::probabilities`; this is the placement half of the same upstream call.
pub fn placement_setup(
    max_level: i32,
    weights: crate::anneal::SoftWeights,
    block_instance_count: usize,
    has_std_cells: bool,
) -> (crate::anneal::SoftWeights, i32, crate::anneal::ActionProbabilities) {
    let (mut adjusted, probabilities) = if has_std_cells {
        (weights, crate::anneal::ActionProbabilities::placement_defaults())
    } else {
        let reset = reset_sa_parameters(weights);
        (
            reset.weights,
            crate::anneal::ActionProbabilities::normalized(
                reset.pos_swap,
                reset.neg_swap,
                reset.double_swap,
                reset.exchange_swap,
                reset.resize,
            ),
        )
    };
    adjusted.soft_blockage =
        adjusted_soft_blockage_weight(max_level, adjusted.outline, adjusted.soft_blockage);
    (adjusted, tiny_cluster_max_number_of_std_cells(block_instance_count), probabilities)
}

// ---------------------------------------------------------------- applying one utilization

/// A macro's shape curve, as fine shaping leaves it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReshapedMacro {
    pub id: usize,
    pub intervals: Vec<crate::anneal::Interval>,
    pub area: i64,
}

/// What `applyUtilization` needs to know about one macro's cluster.
#[derive(Debug, Clone, PartialEq)]
pub struct ReshapeInput {
    pub kind: Option<AreaKind>,
    /// `Cluster::getArea` — for a standard-cell cluster, the whole thing.
    pub cluster_area: i64,
    /// `Cluster::getStdCellArea` — the standard-cell half only, which is what a mixed cluster
    /// inflates.
    pub cluster_std_cell_area: i64,
    pub num_std_cell: i32,
    /// ⚠️ Ordered by area, so the LAST is the largest — which is the one a mixed cluster inflates
    /// against.
    pub tilings: Vec<(i32, i32)>,
}

/// Upstream `HierRTLMP::applyUtilization`.
///
/// 🔑 **It reshapes only STANDARD-CELL and MIXED clusters.** A hard-macro cluster keeps the shapes
/// `setMacroClustersShapes` already gave it — its macros do not compress, so there is nothing for a
/// utilization to do.
///
/// ⛔ **Three kinds are skipped before either test**: a macro with no cluster behind it — a
/// blockage — an IO cluster, and a FIXED macro. So a fixed macro keeps the clipped box it was built
/// with, at every utilization.
///
/// ⚠️ **`singleArraySingleStdCellCluster` is decided ONCE, from the ORIGINAL macros**, before any
/// reshaping — so the collapse it triggers is judged on the design as it stands, not on a design
/// half-reshaped.
///
/// ⚠️ Upstream copies the whole macro list and reshapes the copy, which is why the original
/// survives to be re-used at the next utilization. Returning only the changes says the same thing.
pub fn apply_utilization(
    macros: &[ReshapeInput],
    tiny_threshold: i32,
    single_array_single_std_cell: bool,
    utilization: f32,
    min_ar: f32,
) -> Vec<ReshapedMacro> {
    let mut out = Vec::new();
    for (id, m) in macros.iter().enumerate() {
        // ⛔ Blockage, IO cluster or fixed macro — untouched at every utilization.
        match m.kind {
            None | Some(AreaKind::IoCluster) | Some(AreaKind::FixedMacro) | Some(AreaKind::Blockage) => {
                continue
            }
            Some(AreaKind::HardMacroCluster) => continue,
            Some(AreaKind::StdCellCluster) => {
                let (interval, area) = std_cell_cluster_shape(
                    m.cluster_area,
                    m.num_std_cell,
                    tiny_threshold,
                    single_array_single_std_cell,
                    utilization,
                    min_ar,
                );
                out.push(ReshapedMacro { id, intervals: vec![interval], area });
            }
            Some(AreaKind::MixedCluster) => {
                if let Some((intervals, area)) =
                    mixed_cluster_shape(&m.tilings, m.cluster_std_cell_area, utilization)
                {
                    out.push(ReshapedMacro { id, intervals, area });
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------- annealing one parent

/// The seam's real implementation, for the placement annealer.
///
/// ⛔ **`notch_thresholds` reports what `calNotchPenalty` LEFT BEHIND, not what a constructor was
/// given.** With the notch term live it overwrote both from the outline — crossed, so the `h`
/// threshold is a tenth of the HEIGHT. With the term dark it never ran, and the constructor's `10`
/// database units stand. Deriving it from the weight is what keeps that coupling honest instead of
/// hiding it behind a stored field.
impl Enhancements for crate::anneal::Search {
    fn macros(&self) -> &[crate::anneal::SoftMacro] {
        &self.macros
    }
    fn macros_mut(&mut self) -> &mut [crate::anneal::SoftMacro] {
        &mut self.macros
    }
    fn order(&self) -> &[usize] {
        &self.sp.pos
    }
    fn outline(&self) -> (i32, i32) {
        (self.outline_width, self.outline_height)
    }
    fn packing(&self) -> (i32, i32) {
        (self.width, self.height)
    }
    fn outline_penalty(&self) -> f32 {
        self.penalties.outline
    }
    fn is_valid(&self) -> bool {
        // ⚠️ Upstream's own test: nothing may overlap a fixed macro, AND the packing must fit.
        crate::anneal::Search::is_valid(self, !self.fixed_bboxes.is_empty())
    }
    fn notch_thresholds(&self) -> (i32, i32) {
        if self.weights.notch > 0.0 {
            // ⛔ Crossed: `h` from the HEIGHT, `v` from the WIDTH. See `notch_penalty`.
            (self.outline_height / 10, self.outline_width / 10)
        } else {
            // ⚠️ The constructor's values, which `mpl.tcl` never overrides.
            (10, 10)
        }
    }
    fn cal_penalty(&mut self) {
        crate::anneal::Search::cal_penalty(self);
    }
    fn norm_cost(&self) -> f32 {
        crate::anneal::Search::norm_cost(self)
    }
}

/// Everything one annealing run of one parent needs.
///
/// ⚠️ **Assembled ONCE per parent and reused across its ten runs** — only the utilization and the
/// seed change between them. Rebuilding it per run would be the same answer at ten times the cost,
/// and would make the shared `random_seed_` meaningless.
pub struct ParentProblem {
    /// The macro list in id order, from [`assemble`].
    pub macros: Vec<crate::anneal::SoftMacro>,
    /// One entry per macro, describing what the utilization does to it.
    pub reshape: Vec<ReshapeInput>,
    /// `0..number_of_sequence_pair_macros` is what the annealer permutes.
    pub number_of_sequence_pair_macros: usize,
    pub inputs: PlacementInputs,
    pub outline: (i32, i32),
    pub dbu_per_micron: i32,
    /// The bounding boxes of the fixed macros, for the fixed-macro penalty.
    pub fixed_bboxes: Vec<(i32, i32, i32, i32)>,
    pub tiny_threshold: i32,
    pub min_ar: f32,
    /// ⛔ Set when `singleArraySingleStdCellCluster` holds — it forces centralization to stick.
    pub force_centralization: bool,
    /// Each macro's name, in id order — `writeFloorplanFile`'s first column.
    ///
    /// ℹ️ Carried so the placed geometry can be diffed against the reference's `.fp.txt`, which is
    /// the only per-cluster oracle this stage has: the penalty table summarises the same placement
    /// into nine numbers, and a term can agree while the geometry behind it does not.
    pub names: Vec<String>,
}

/// Upstream's per-run construction and `runSA`, for one soft-macro annealer.
///
/// 🔑 **The order is `initialize` then `run`, and `initialize` MOVES THE STATE.** Its sweep never
/// restores, so the annealer starts `fastSA` from wherever the last sampling perturbation left it
/// — not from the packing it was constructed with. Treating `initialize` as measurement-only is the
/// single easiest way to get a different search.
///
/// 🔑 **`run` is `fastSA` FOLLOWED BY the enhancements** — centralization, and alignment only if
/// centralization was reverted. They are part of the run, not a post-process, and the result the
/// caller reads has been through them.
///
/// ⚠️ **Validity is judged AFTER all of that**, on the final state — so a run that annealed to a
/// valid packing and was then pushed out of the outline by centralization reports invalid.
///
/// ⚠️ The reshaping is applied to a COPY, so the parent's own macro list survives for the next
/// utilization — see [`ParentProblem`].
pub fn anneal_one_run(
    problem: &ParentProblem,
    utilization: f32,
    seed: u32,
    params: &crate::anneal::SaParameters,
    probabilities: crate::anneal::ActionProbabilities,
    weights: crate::anneal::SoftWeights,
) -> Option<crate::anneal::Search> {
    let single_array = problem.force_centralization;
    let reshaped = apply_utilization(
        &problem.reshape,
        problem.tiny_threshold,
        single_array,
        utilization,
        problem.min_ar,
    );

    // ⚠️ A macro with no reshaping keeps whatever curve its own shapes gave it — a hard-macro
    // cluster's tilings, or nothing at all for a blockage or a terminal.
    let mut macros = problem.macros.clone();
    let mut curves = vec![crate::anneal::ShapeCurve::default(); macros.len()];
    for r in &reshaped {
        if let Some((curve, width, height, area)) =
            crate::anneal::shape_curve_from_intervals(&r.intervals, r.area)
        {
            curves[r.id] = curve;
            macros[r.id].width = width;
            macros[r.id].height = height;
            macros[r.id].area = area;
        }
    }

    // Upstream `setMacroClustersShapes`, run over the macro list just before the annealer is
    // built.
    //
    // ⛔ **A macro cluster is never reshaped by the utilization**, so it never appears in
    // `reshaped` — but it still needs a shape curve, and its curve is its TILING LIST. Leaving it
    // empty makes a `resize` action on a macro cluster a no-op where upstream reshapes it, so the
    // random walk diverges from the first resize onwards.
    //
    // ⚠️ `setShapes(tilings)` also moves the macro ONTO its first tiling — it is not only a
    // constraint list — and it refuses anything that is not a non-fixed `HardMacroCluster`.
    for (id, r) in problem.reshape.iter().enumerate() {
        if r.kind == Some(AreaKind::HardMacroCluster) && !r.tilings.is_empty() {
            let (curve, width, height, area) = crate::anneal::shape_curve_from_tilings(&r.tilings);
            curves[id] = curve;
            macros[id].width = width;
            macros[id].height = height;
            macros[id].area = area;
        }
    }

    let mut search = crate::anneal::Search {
        sp: crate::anneal::init_sequence_pair(problem.number_of_sequence_pair_macros),
        macros,
        curves,
        width: 0,
        height: 0,
        penalties: crate::anneal::Penalties::default(),
        placement: Some(Box::new(problem.inputs.clone())),
        outline_width: problem.outline.0,
        outline_height: problem.outline.1,
        dbu_per_micron: problem.dbu_per_micron,
        fixed_bboxes: problem.fixed_bboxes.clone(),
        weights,
        normalization: crate::anneal::Normalization::default(),
        probabilities,
        action: None,
        // Cluster placement is the SOFT core; the hard one is `place_macros`'s.
        hard_probabilities: None,
        cost_history: Vec::new(),
    };

    let mut rng = crate::rng::Mt19937::new(seed);

    // ⛔ `initialize` leaves the state where its last perturbation put it; `fast_sa` starts there.
    let init_temperature = search.initialize(&mut rng, params);
    // ⚠️ Read AFTER `initialize`, which re-runs `findFixedMacros` over the sequence pair. Upstream
    // has no snapshot at all — `isValid()` consults `fixed_macros_` on every call — so taking it
    // beforehand would freeze a list the sweep is about to rebuild.
    let fixed_present = !search.fixed_bboxes.is_empty();
    search.fast_sa(&mut rng, params, init_temperature, fixed_present);

    // 🔑 Part of `run`, not a post-process.
    run_enhancements(&mut search, single_array);

    if !search.is_valid(fixed_present) {
        return None;
    }
    Some(search)
}

/// Upstream's `Cluster Placement Summary` table, byte for byte.
///
/// ⛔ **The Area row's normalisation factor is a HARDCODED `1.0`**, not the measured one. Upstream
/// passes the literal at the call site, and it is consistent with `calNormCost`, which does not
/// divide the area term — but the printed column is a constant, not a measurement. Confirmed
/// against all 34 reference captures: every Area row reads `1.0000` while every other term spans
/// `0.03` to `0.99`.
///
/// ⛔ **The Cost column is RECOMPUTED as `weight * value / factor`**, not taken from the cost
/// function. A term whose factor were zero would print an infinity here while `calNormCost` drops
/// it. ℹ️ Not reachable in the suite — no capture contains an `inf` or a `nan` — so this is read
/// from the source and cannot be measured.
///
/// ⛔ **The FIXED MACROS term is in the cost and NOT in the table.** Eight rows are printed; nine
/// terms are summed. A reader adding the Cost column up will not reach the total on any design
/// with a fixed macro.
///
/// ⚠️ **Every row ends in a trailing SPACE**, and the header is preceded by a blank line. Both are
/// in the format string.
pub fn cluster_placement_summary(
    cluster_id: i32,
    outline: (i32, i32, i32, i32),
    penalties: &crate::anneal::Penalties,
    weights: &crate::anneal::SoftWeights,
    norms: &crate::anneal::Normalization,
    area_penalty: f32,
    total_cost: f32,
    dbu_per_micron: i32,
) -> String {
    let um = |v: i32| v as f64 / dbu_per_micron as f64;
    let mut out = String::new();
    out.push_str(&format!("Id: {cluster_id}\n"));
    out.push_str(&format!(
        "Outline: ({:^8.2} {:^8.2}) ({:^8.2} {:^8.2})\n",
        um(outline.0),
        um(outline.1),
        um(outline.2),
        um(outline.3)
    ));
    out.push_str("\n  Penalty Type  |  Weight  |  Value  |  Norm. Factor  |  Cost\n");
    out.push_str("---------------------------------------------------------------\n");

    let mut row = |name: &str, weight: f32, value: f32, factor: f32| {
        out.push_str(&format!(
            "{:>15} | {:>8.4} | {:>7.4} | {:>14.4} | {:>7.4} \n",
            name,
            weight,
            value,
            factor,
            weight * value / factor
        ));
    };

    // ⛔ The literal `1.0`, not `norms.area`.
    row("Area", weights.area, area_penalty, 1.0);
    row("Outline", weights.outline, penalties.outline, norms.outline);
    row("Wire Length", weights.wirelength, penalties.wirelength, norms.wirelength);
    row("Guidance", weights.guidance, penalties.guidance, norms.guidance);
    row("Fence", weights.fence, penalties.fence, norms.fence);
    row("Boundary", weights.boundary, penalties.boundary, norms.boundary);
    row("Soft Blockage", weights.soft_blockage, penalties.soft_blockage, norms.soft_blockage);
    row("Notch", weights.notch, penalties.notch, norms.notch);

    out.push_str("---------------------------------------------------------------\n");
    out.push_str(&format!("  Total Cost  {total_cost:>49.4} \n\n"));
    out
}

// ---------------------------------------------------------------- from a cluster tree

/// Upstream's classification of one child, as every placement stage asks for it.
///
/// ⛔ **The order of the tests is upstream's and is not interchangeable.** An IO cluster is asked
/// about FIRST — before fixedness and before the cluster type — because `placeChildren` defers it
/// before anything else looks at it. A FIXED macro is asked about second, ahead of its type, which
/// is `HardMacroCluster` and would otherwise swallow it.
pub fn area_kind_of(cluster: &crate::cluster::Cluster) -> AreaKind {
    if cluster.is_io_cluster() {
        return AreaKind::IoCluster;
    }
    if cluster.is_fixed_macro {
        return AreaKind::FixedMacro;
    }
    match cluster.cluster_type {
        crate::cluster::ClusterType::StdCell => AreaKind::StdCellCluster,
        crate::cluster::ClusterType::HardMacro => AreaKind::HardMacroCluster,
        crate::cluster::ClusterType::Mixed => AreaKind::MixedCluster,
    }
}

/// What the caller must supply that a `Cluster` does not carry.
///
/// ⚠️ **Nets and regions are NOT on the tree.** Connections live in `netlist::Connections`,
/// rebuilt per level, and the IO regions come from coarse shaping — so they arrive here rather
/// than being read off a cluster. That is the shape of our clustering output, not upstream's.
pub struct ParentContext<'a> {
    /// `Connections::of(cluster_id)` for each child, plus the parent's virtual connections.
    pub connections_of: &'a dyn Fn(i32) -> Vec<(i32, f32)>,
    pub virtual_connections: &'a [(i32, i32)],
    /// Placement blockages, already clipped to this outline and rebased onto it.
    pub blockages: &'a [(i32, i32, i32, i32)],
    /// IO blockages, likewise — these are the SOFT ones the blockage penalty scores against.
    pub soft_blockages: &'a [(i32, i32, i32, i32)],
    /// A fence or a guide declared for a child, in DIE coordinates.
    pub fence_of: &'a dyn Fn(i32) -> Option<(i32, i32, i32, i32)>,
    pub guide_of: &'a dyn Fn(i32) -> Option<(i32, i32, i32, i32)>,
    /// The fixed terminals, from `fixed_terminal_walk` up the tree.
    pub terminals: &'a [(String, crate::anneal::SoftMacro)],
    pub root: Root,
    pub die_margin: i64,
    /// ⛔ **OUTLINE-RELATIVE — the caller must rebase these**, unlike `constraint_region_of`
    /// below, which stays absolute. See that field for why the two differ.
    pub available_regions: &'a [Region],
    /// ⛔ **ABSOLUTE die coordinates — do NOT rebase these onto the outline.**
    ///
    /// 🔑 **The two region inputs are in DIFFERENT coordinate systems, and that is upstream's,
    /// not ours.** `setAvailableRegionsForUnconstrainedPins` subtracts the outline's corner from
    /// every region it is handed, so `available_regions` arrives outline-relative; but
    /// `io_cluster_to_constraint_` is assigned straight off the tree and keeps die coordinates.
    /// Both are then measured against an outline-relative pin, so the constrained branch compares
    /// across two systems deliberately. Rebasing this one to "fix" the inconsistency is a
    /// different program.
    ///
    /// ⚠️ Keyed by CLUSTER id, like `fence_of` and `guide_of` beside it — the translation to the
    /// assembled macro index happens below, where the assembly is known. A caller cannot predict
    /// that index: it depends on the blockage count and on where the IO clusters were deferred to.
    pub constraint_region_of: &'a dyn Fn(i32) -> Option<Region>,
    pub weights: crate::anneal::SoftWeights,
    pub dbu_per_micron: i32,
    pub tiny_threshold: i32,
    pub min_ar: f32,
}

/// Build one parent's placement problem from its children.
///
/// 🔑 **The macro that represents a child depends on what the child IS**, and the three cases are
/// upstream's: a FIXED macro is its own clustering-time soft macro CLIPPED to this outline and
/// rebased; an IO cluster is its clustering-time soft macro rebased but NOT clipped, because it is
/// a region rather than an occupant; anything else starts from its tilings or its area and is
/// reshaped per utilization.
///
/// ⚠️ **`single_array_single_std_cell_cluster` is decided here, once**, from the assembled list —
/// and it is what forces centralization to stick as well as what collapses the cell cluster.
///
/// ⛔ **A cluster with no soft macro contributes a macro at the ORIGIN**, because that is what its
/// accessors report. Upstream has the same behaviour and it is only ever safe because the four
/// setters cover every kind that reaches here without going through placement first.
pub fn build_parent_problem(
    parent: &crate::cluster::Cluster,
    outline: (i32, i32, i32, i32),
    ctx: &ParentContext,
) -> ParentProblem {
    let origin = (outline.0, outline.1);
    let size = (outline.2 - outline.0, outline.3 - outline.1);

    let blockage_macros = soft_macros_for_blockages(ctx.blockages);
    let mut reshape: Vec<ReshapeInput> = blockage_macros
        .iter()
        .map(|_| ReshapeInput {
            kind: Some(AreaKind::Blockage),
            cluster_area: 0,
            cluster_std_cell_area: 0,
            num_std_cell: 0,
            tilings: Vec::new(),
        })
        .collect();

    let mut children = Vec::new();
    for child in &parent.children {
        let kind = area_kind_of(child);
        let macro_ = child_soft_macro(child, kind, outline);
        children.push(AssemblyChild {
            name: child.name.clone(),
            kind,
            macro_,
            fence: (ctx.fence_of)(child.id),
            guide: (ctx.guide_of)(child.id),
        });
    }

    // ⚠️ In the same order `assemble` will place them: children first, then the deferred IO
    // clusters, then the terminals — so `reshape` indexes match the macro list.
    for child in children.iter().filter(|c| c.kind != AreaKind::IoCluster) {
        reshape.push(reshape_input_for(parent, &child.name, child.kind));
    }
    for child in children.iter().filter(|c| c.kind == AreaKind::IoCluster) {
        reshape.push(reshape_input_for(parent, &child.name, child.kind));
    }
    for _ in ctx.terminals {
        // ⚠️ A terminal has no cluster behind it, so nothing reshapes it.
        reshape.push(ReshapeInput {
            kind: None,
            cluster_area: 0,
            cluster_std_cell_area: 0,
            num_std_cell: 0,
            tilings: Vec::new(),
        });
    }

    let assembly = assemble(&blockage_macros, &children, outline, ctx.terminals);

    let attributes = assembly
        .macros
        .iter()
        .enumerate()
        .map(|(id, _)| attributes_for(parent, &assembly, id))
        .collect();

    let nets = parent_nets(parent, &assembly, ctx);

    // ⛔ **In the reference's vocabulary, not ours.** `singleArraySingleStdCellCluster` reads
    // `SoftMacro::isMixedCluster()`/`isMacroCluster()`/`isStdCellCluster()`, and every one of those
    // is a test on `Cluster::getClusterType()` — so an IO cluster answers MIXED and a fixed macro
    // cluster answers HARD MACRO. `AreaKind` has variants for both and would skip them.
    //
    // ⚠️ **A fixed terminal is `None` unless it is a cluster of UNPLACED IO PINS**, which is the
    // one kind `createFixedTerminal` keeps the cluster pointer for — and that cluster is Mixed, so
    // it ends the scan. A conventional terminal is a bare point with no cluster behind it.
    let entries: Vec<(Option<crate::cluster::ClusterType>, bool)> = reshape
        .iter()
        .enumerate()
        .map(|(id, r)| {
            let is_array = r.tilings.len() > 1;
            let cluster_type = match r.kind {
                Some(AreaKind::Blockage) => None,
                Some(AreaKind::IoCluster) => Some(crate::cluster::ClusterType::Mixed),
                Some(AreaKind::FixedMacro) | Some(AreaKind::HardMacroCluster) => {
                    Some(crate::cluster::ClusterType::HardMacro)
                }
                Some(AreaKind::StdCellCluster) => Some(crate::cluster::ClusterType::StdCell),
                Some(AreaKind::MixedCluster) => Some(crate::cluster::ClusterType::Mixed),
                // A terminal. Its index into `ctx.terminals` is its position past the others.
                //
                // ⚠️ **A terminal's SIZE stands in for its cluster pointer**, and the two are set
                // together at the one site that builds them: `createFixedTerminal` gives a
                // conventional terminal a bare point and a null cluster, and gives a cluster of
                // UNPLACED IO PINS both its real shape and its cluster. So a sized terminal is
                // exactly the one whose cluster is non-null — and that cluster is Mixed.
                None => {
                    let first_terminal = reshape.len() - ctx.terminals.len();
                    match ctx.terminals.get(id.wrapping_sub(first_terminal)) {
                        Some((_, m)) if m.width > 0 || m.height > 0 => {
                            Some(crate::cluster::ClusterType::Mixed)
                        }
                        _ => None,
                    }
                }
            };
            (cluster_type, is_array)
        })
        .collect();
    let single_array = single_array_single_std_cell_cluster(&entries);

    ParentProblem {
        macros: assembly.macros.clone(),
        reshape,
        number_of_sequence_pair_macros: assembly.number_of_sequence_pair_macros,
        inputs: PlacementInputs {
            attributes,
            nets,
            guides: assembly.guides.clone(),
            fences: assembly.fences.clone(),
            soft_blockages: ctx.soft_blockages.to_vec(),
            outline_origin: origin,
            root: ctx.root,
            die_margin: ctx.die_margin,
            available_regions: ctx.available_regions.to_vec(),
            constraint_regions: parent
                .children
                .iter()
                .filter_map(|c| {
                    let region = (ctx.constraint_region_of)(c.id)?;
                    Some((assembly.id(&c.name)?, region))
                })
                .collect(),
            weights: ctx.weights,
        },
        outline: size,
        dbu_per_micron: ctx.dbu_per_micron,
        // ⛔ **`findFixedMacros` walks the SEQUENCE PAIR and takes anything `isFixed()`** — not
        // the children, and not only the fixed macros. A BLOCKAGE proxy is fixed and sits in the
        // sequence pair ahead of every child, so it belongs here too.
        //
        // ⚠️ Filtering the children by `AreaKind::FixedMacro` missed it, and the miss is invisible
        // until something overlaps the blockage: the fixed-macro penalty stays zero, `isValid`
        // then reads true, and the notch term takes its ordinary scan where upstream treats the
        // whole floorplan as one huge notch.
        //
        // ℹ️ IO clusters are fixed too, and are correctly absent — they are appended AFTER the
        // sequence pair ends, so the bound below excludes them.
        fixed_bboxes: assembly
            .macros
            .iter()
            .take(assembly.number_of_sequence_pair_macros)
            .filter(|m| m.fixed)
            .map(|m| m.bbox())
            .collect(),
        tiny_threshold: ctx.tiny_threshold,
        min_ar: ctx.min_ar,
        force_centralization: single_array,
        names: {
            let mut names = vec![String::new(); assembly.macros.len()];
            for (name, id) in &assembly.id_of {
                if let Some(slot) = names.get_mut(*id) {
                    slot.clone_from(name);
                }
            }
            names
        },
    }
}

/// The soft macro one child contributes to its parent's annealer.
fn child_soft_macro(
    child: &crate::cluster::Cluster,
    kind: AreaKind,
    outline: (i32, i32, i32, i32),
) -> crate::anneal::SoftMacro {
    let origin = (outline.0, outline.1);
    match kind {
        // ⛔ CLIPPED to the outline and rebased — the fixed macro's own soft macro is neither.
        AreaKind::FixedMacro => {
            let own = child.soft_macro.unwrap_or_default();
            let bbox = (own.x, own.y, own.x + own.width, own.y + own.height);
            let clipped = (
                bbox.0.max(outline.0),
                bbox.1.max(outline.1),
                bbox.2.min(outline.2),
                bbox.3.min(outline.3),
            );
            let (w, h) = ((clipped.2 - clipped.0).max(0), (clipped.3 - clipped.1).max(0));
            crate::anneal::SoftMacro {
                x: clipped.0 - origin.0,
                y: clipped.1 - origin.1,
                width: w,
                height: h,
                fixed: true,
                area: w as i64 * h as i64,
                // ⛔ **TRUE.** The three-argument `SoftMacro(logger, hard_macro, outline)` ends
                // with `cluster_ = hard_macro->getCluster()`, and that cluster is a
                // `HardMacroCluster` — so `isMacroCluster()` holds for a FIXED macro too, and it
                // obstructs the notch grid like any other. ⚠️ Every guard that must still exclude
                // it does so through `fixed`, not through this flag.
                is_macro_cluster: true,
            }
        }
        // ⚠️ Rebased but NOT clipped — an IO cluster is a region, not an occupant, and its area
        // stays zero.
        AreaKind::IoCluster => {
            let own = child.soft_macro.unwrap_or_default();
            crate::anneal::SoftMacro {
                x: own.x - origin.0,
                y: own.y - origin.1,
                width: own.width,
                height: own.height,
                fixed: true,
                area: 0,
                is_macro_cluster: false,
            }
        }
        _ => {
            let first = child.tilings.first();
            crate::anneal::SoftMacro {
                x: 0,
                y: 0,
                width: first.map_or(0, |t| t.width as i32),
                height: first.map_or(0, |t| t.height as i32),
                fixed: false,
                area: first.map_or(child.area(), |t| t.area()),
                is_macro_cluster: kind == AreaKind::HardMacroCluster,
            }
        }
    }
}

fn reshape_input_for(
    parent: &crate::cluster::Cluster,
    name: &str,
    kind: AreaKind,
) -> ReshapeInput {
    let child = parent.children.iter().find(|c| c.name == name);
    ReshapeInput {
        kind: Some(kind),
        cluster_area: child.map_or(0, |c| c.area()),
        cluster_std_cell_area: child.map_or(0, |c| c.std_cell_area()),
        num_std_cell: child.map_or(0, |c| c.num_std_cell()),
        tilings: child
            .map(|c| c.tilings.iter().map(|t| (t.width as i32, t.height as i32)).collect())
            .unwrap_or_default(),
    }
}

fn attributes_for(
    parent: &crate::cluster::Cluster,
    assembly: &Assembly,
    id: usize,
) -> MacroAttributes {
    let Some((name, _)) = assembly.id_of.iter().find(|(_, i)| *i == id) else {
        // ⚠️ A blockage has an id but no name; it has no cluster behind it either.
        return MacroAttributes::default();
    };
    let Some(child) = parent.children.iter().find(|c| &c.name == name) else {
        return MacroAttributes::default();
    };
    let kind = area_kind_of(child);
    MacroAttributes {
        kind: Some(kind),
        num_macro: child.num_macro(),
        cluster_macro_area: child.macro_area(),
        cluster_area: child.area(),
        is_cluster_of_unplaced_io_pins: child.is_cluster_of_unplaced_io_pins,
        is_unconstrained_io_cluster: child.is_cluster_of_unconstrained_io_pins,
        // ℹ️ Cluster placement is the SOFT core: the pin is the macro's centre.
        pin_offset: None,
    }
}

/// Upstream `buildBundledNets` for cluster placement, driven from the real tree.
///
/// ⚠️ **Virtual connections first, at weight `10`, then each child's own** — and only where the
/// child's id is strictly GREATER than the target's, which halves the undirected pairs.
fn parent_nets(
    parent: &crate::cluster::Cluster,
    assembly: &Assembly,
    ctx: &ParentContext,
) -> Vec<BundledNet> {
    let id_of_cluster = |cluster_id: i32| -> Option<usize> {
        parent
            .children
            .iter()
            .find(|c| c.id == cluster_id)
            .and_then(|c| assembly.id(&c.name))
    };
    let mut nets = Vec::new();
    for &(a, b) in ctx.virtual_connections {
        if let (Some(x), Some(y)) = (id_of_cluster(a), id_of_cluster(b)) {
            nets.push(BundledNet { source: x, target: y, weight: VIRTUAL_CONNECTION_WEIGHT });
        }
    }
    for child in &parent.children {
        let Some(source) = assembly.id(&child.name) else { continue };
        for (target_cluster, weight) in (ctx.connections_of)(child.id) {
            let Some(target) = id_of_cluster(target_cluster) else { continue };
            // ⚠️ Strictly greater — halves the pairs, and drops a self-connection.
            if child.id > target_cluster {
                nets.push(BundledNet { source, target, weight });
            }
        }
    }
    nets
}

/// Upstream `runHierarchicalMacroPlacement`, composed over a real tree.
///
/// 🔑 **Connections are REBUILT before every parent, not once.** Upstream calls
/// `rebuildConnections` inside `placeChildren`, so each level is annealed against a connection map
/// derived from the instance-to-cluster association as it stands at that level — and that
/// association is itself updated per parent. Building the map once, up front, is a different
/// netlist for every level below the first.
///
/// ⚠️ **The caller supplies the per-parent context**, because everything in it — connections,
/// blockages clipped to this outline, the fixed-terminal walk — depends on where in the tree we
/// are. Threading it as a callback keeps that dependency visible instead of smuggling a mutable
/// world through.
///
/// ℹ️ Returns the visits in order, which is the record the per-term oracle is scored against.
pub fn run_hierarchical_macro_placement(
    root: &crate::cluster::Cluster,
    utilizations: &[f32],
    num_threads: usize,
    params: &crate::anneal::SaParameters,
    probabilities: crate::anneal::ActionProbabilities,
    weights: crate::anneal::SoftWeights,
    random_seed: u32,
    context_for: &mut dyn FnMut(&crate::cluster::Cluster) -> Option<(ParentProblem, i32)>,
    // ⛔ Called with each parent's winning macros BEFORE the walk descends into its children —
    // upstream writes the shapes onto the tree at exactly this point, so a macro-cluster child
    // reads the outline its parent just gave it. The caller owns the write-back because it owns
    // the outlines and the name map; the driver only guarantees the ORDER.
    on_parent_placed: &mut dyn FnMut(i32, &[crate::anneal::SoftMacro]),
    // The per-macro-cluster inputs `placeMacros` assembles. `None` refuses the cluster.
    macro_context_for: &mut dyn FnMut(&crate::cluster::Cluster) -> Option<MacroProblem>,
    num_runs: i32,
) -> Vec<PlacementVisit> {
    let by_id = |id: i32| -> Option<&crate::cluster::Cluster> { find_cluster(root, id) };

    let tree = PlacementTree {
        kind: &|id| by_id(id).map_or(AreaKind::StdCellCluster, area_kind_of),
        is_fixed_macro: &|id| by_id(id).is_some_and(|c| c.is_fixed_macro),
        is_leaf: &|id| by_id(id).is_none_or(|c| c.children.is_empty()),
        children: &|id| by_id(id).map_or(Vec::new(), |c| c.children.iter().map(|k| k.id).collect()),
        utilizations,
        num_threads,
    };

    // Upstream `placeChildren`'s first branch: a hard-macro cluster goes straight to `placeMacros`.
    let mut place_macros_one = |cluster_id: i32| -> Option<Vec<crate::anneal::SoftMacro>> {
        let cluster = find_cluster(root, cluster_id)?;
        // ⛔ `placeMacros` returns at its first line for a FIXED macro cluster — it is not the
        // placer's to move — and the driver reports that separately, so it never reaches here.
        let problem = macro_context_for(cluster)?;
        place_macros(&problem, weights, probabilities, params, num_runs, random_seed)
            .map(|search| search.macros)
    };

    let mut cached: Option<(i32, ParentProblem)> = None;
    place_children(&tree, root.id, &mut |cluster_id, index, utilization| {
        // ⚠️ Assembled ONCE per parent and reused across its ten runs — only the utilization and
        // the seed change between them.
        if cached.as_ref().is_none_or(|(id, _)| *id != cluster_id) {
            let parent = by_id(cluster_id)?;
            let (problem, _) = context_for(parent)?;
            cached = Some((cluster_id, problem));
        }
        let (_, problem) = cached.as_ref()?;
        // Upstream computes this at the call site, once per parent, and hands it to the core.
        let mut params = *params;
        params.num_perturb_per_step = cluster_perturbations_per_step(
            params.num_perturb_per_step,
            problem.macros.len() as i32,
        );
        anneal_one_run(
            problem,
            utilization,
            // ⚠️ Cluster placement shares ONE seed across its runs — unlike macro placement,
            // which varies it. The runs differ by their utilization alone.
            random_seed,
            &params,
            probabilities,
            weights,
        )
        .map(|mut search| {
            let _ = index;
            // ⛔ Upstream fills the dead space on `best_sa` AFTER the ten runs are judged, not on
            // each one. Doing it per run reaches the same answer because nothing in the selection
            // reads the filled geometry — `select_run` is handed a bool — but the cost figures a
            // run is judged on are the annealer's, and those are computed before this point.
            let valid = search.is_valid(!search.fixed_bboxes.is_empty());
            let kinds: Vec<Option<AreaKind>> = problem.reshape.iter().map(|r| r.kind).collect();
            fill_dead_space_on_solution(
                &mut search.macros,
                &kinds,
                (search.outline_width, search.outline_height),
                valid,
            );
            search.macros
        })
    }, &mut place_macros_one, on_parent_placed)
}

/// Depth-first lookup by id. ⚠️ Ids are unique across the tree, so the first match is the only one.
fn find_cluster(root: &crate::cluster::Cluster, id: i32) -> Option<&crate::cluster::Cluster> {
    if root.id == id {
        return Some(root);
    }
    root.children.iter().find_map(|c| find_cluster(c, id))
}

/// Upstream `SACoreHardMacro::printResults`, via `printPlacementResult`.
///
/// ⛔ **FIVE rows, not nine.** Boundary, soft blockage and notch are `SACoreSoftMacro`'s own
/// members; the hard core does not weight them at zero, it has no such members. Printing them as
/// zeros would be a different table.
///
/// ⚠️ The Area row's divisor is the literal `1.0`, as in the cluster summary — `norm_area_penalty_`
/// is measured but never used in the report.
pub fn macro_placement_summary(
    cluster_id: i32,
    outline: (i32, i32, i32, i32),
    penalties: &crate::anneal::Penalties,
    weights: &crate::anneal::SoftWeights,
    norms: &crate::anneal::Normalization,
    area_penalty: f32,
    total_cost: f32,
    dbu_per_micron: i32,
) -> String {
    let um = |v: i32| v as f64 / dbu_per_micron as f64;
    let mut out = String::new();
    out.push_str(&format!("Id: {cluster_id}\n"));
    out.push_str(&format!(
        "Outline: ({:^8.2} {:^8.2}) ({:^8.2} {:^8.2})\n",
        um(outline.0),
        um(outline.1),
        um(outline.2),
        um(outline.3)
    ));
    out.push_str("\n  Penalty Type  |  Weight  |  Value  |  Norm. Factor  |  Cost\n");
    out.push_str("---------------------------------------------------------------\n");
    let mut row = |name: &str, weight: f32, value: f32, factor: f32| {
        out.push_str(&format!(
            "{:>15} | {:>8.4} | {:>7.4} | {:>14.4} | {:>7.4} \n",
            name,
            weight,
            value,
            factor,
            weight * value / factor
        ));
    };
    row("Area", weights.area, area_penalty, 1.0);
    row("Outline", weights.outline, penalties.outline, norms.outline);
    row("Wire Length", weights.wirelength, penalties.wirelength, norms.wirelength);
    row("Guidance", weights.guidance, penalties.guidance, norms.guidance);
    row("Fence", weights.fence, penalties.fence, norms.fence);
    out.push_str("---------------------------------------------------------------\n");
    // ⚠️ `reportTotalCost` is the BASE class's, so both cores print this line identically —
    // including the trailing blank line the logger's own `\n` adds.
    out.push_str(&format!("  Total Cost  {total_cost:>49.4} \n\n"));
    out
}

// ---------------------------------------------------------------- placeMacros

/// Everything one macro cluster's hard-macro run needs.
#[derive(Debug, Clone)]
pub struct MacroProblem {
    /// The temp macro clusters' boxes, in the CLUSTER's outline coordinates.
    pub macros: Vec<crate::anneal::SoftMacro>,
    /// ⚠️ The hard macros only — the fixed terminals appended after are outside the pair.
    pub number_of_sequence_pair_macros: usize,
    pub inputs: PlacementInputs,
    pub outline: (i32, i32),
    pub dbu_per_micron: i32,
    pub is_macro_array: bool,
    pub array_has_empty_space: bool,
    /// `computeArraySequencePair`'s grid, for a macro array. `None` gives the identity.
    pub initial_sequence_pair: Option<crate::anneal::SequencePair>,
    /// Distinct masters among the hard macros — it scales the exchange probability.
    pub master_count: usize,
}

/// Upstream `HierRTLMP::placeMacros`' run loop, for ONE macro cluster.
///
/// 🔑 **The hard core is not the soft one with terms switched off.** Four actions and no resize,
/// four penalties and not even the fixed-macro one, and [`hard_norm_cost`] for the cost.
///
/// ⛔ **The per-run weights shape the SEARCH ONLY.** Each run makes the outline weight harsher and
/// the wirelength weight softer, but every run's weights are RESET to the caller's before the
/// costs are compared — so the comparison is on equal terms and a late run is not penalised for
/// having been driven differently.
///
/// ⛔ **Selection is the LOWEST cost among valid runs**, unlike cluster placement, which takes the
/// FIRST valid utilization and stops. Getting these two the same way round is a silent difference:
/// both pick "a valid one", and only a design where the first valid is not the cheapest shows it.
///
/// ⚠️ Returns `None` when no run is valid — upstream raises MPL-10 there.
pub fn place_macros(
    problem: &MacroProblem,
    base_weights: crate::anneal::SoftWeights,
    base_probabilities: crate::anneal::ActionProbabilities,
    params: &crate::anneal::SaParameters,
    num_runs: i32,
    random_seed: u32,
) -> Option<crate::anneal::Search> {
    let probabilities = macro_placement_probabilities(
        base_probabilities.pos_swap,
        base_probabilities.neg_swap,
        base_probabilities.double_swap,
        base_probabilities.exchange,
        problem.master_count,
        problem.number_of_sequence_pair_macros.max(1),
    );
    let setup = macro_array_setup(
        probabilities,
        problem.is_macro_array,
        problem.array_has_empty_space,
    );
    let perturbations = macro_perturbations_per_step(
        params.num_perturb_per_step,
        problem.number_of_sequence_pair_macros as i32,
        problem.is_macro_array,
    );

    let mut runs: Vec<crate::anneal::Search> = Vec::new();
    let mut costs: Vec<(bool, f32)> = Vec::new();

    for run_id in 0..num_runs {
        let mut run_params = *params;
        run_params.num_perturb_per_step = perturbations;
        run_params.invalid_states_allowed = setup.invalid_states_allowed;

        let mut search = crate::anneal::Search {
            sp: crate::anneal::init_sequence_pair_with(
                problem.macros.len(),
                problem.number_of_sequence_pair_macros,
                problem.initial_sequence_pair.clone(),
            ),
            macros: problem.macros.clone(),
            // ⚠️ A hard macro has ONE shape, so there are no curves to resize within.
            curves: vec![crate::anneal::ShapeCurve::default(); problem.macros.len()],
            width: 0,
            height: 0,
            penalties: crate::anneal::Penalties::default(),
            placement: Some(Box::new(problem.inputs.clone())),
            outline_width: problem.outline.0,
            outline_height: problem.outline.1,
            dbu_per_micron: problem.dbu_per_micron,
            fixed_bboxes: Vec::new(),
            weights: macro_run_weights(base_weights, run_id),
            normalization: crate::anneal::Normalization::default(),
            probabilities: base_probabilities,
            action: None,
            hard_probabilities: Some(setup.probabilities),
            cost_history: Vec::new(),
        };
        let (w, h) = crate::anneal::pack_floorplan(&mut search.macros, &search.sp);
        search.width = w;
        search.height = h;
        search.cal_penalty();

        let mut rng = crate::rng::Mt19937::new(macro_run_seed(random_seed, run_id));
        let t = search.initialize(&mut rng, &run_params);
        // ℹ️ `SACoreHardMacro::run` is `fastSA` alone — the two enhancements belong to the soft core.
        let fixed_present = !search.fixed_bboxes.is_empty();
        search.fast_sa(&mut rng, &run_params, t, fixed_present);

        // ⛔ Reset before scoring, so every run is compared on the caller's weights.
        search.weights = base_weights;
        costs.push((search.is_valid(fixed_present), search.norm_cost()));
        runs.push(search);
    }

    let best = best_macro_run(&costs)?;
    Some(runs.swap_remove(best))
}
