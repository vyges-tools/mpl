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
    pub kind: AreaKind,
}

impl NotchMacro {
    /// ⛔ **Only a hard-macro cluster or a mixed cluster obstructs.** A standard-cell cluster does
    /// not, and neither does a blockage or an IO cluster.
    ///
    /// ⛔ **Nor does a FIXED macro** — and that one is easy to miss. Upstream tests
    /// `isMacroCluster() || isMixedCluster()`, both of which return false when the soft macro has
    /// no cluster behind it, and the constructor that builds a soft macro from a fixed hard macro
    /// never sets one. So the space beside a fixed macro can be declared a notch, and the fixed
    /// macro itself sits inside one.
    fn obstructs(&self) -> bool {
        matches!(self.kind, AreaKind::HardMacroCluster | AreaKind::MixedCluster)
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
    entries: &[(AreaKind, bool)],
) -> bool {
    let mut arrays = 0;
    let mut std_clusters = 0;
    for &(kind, is_array_of_interconnected_macros) in entries {
        if kind == AreaKind::MixedCluster {
            return false;
        }
        if kind == AreaKind::Blockage || kind == AreaKind::IoCluster {
            continue;
        }
        match kind {
            AreaKind::HardMacroCluster => {
                if !is_array_of_interconnected_macros {
                    return false;
                }
                arrays += 1;
            }
            AreaKind::StdCellCluster => std_clusters += 1,
            _ => {}
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

            macros[id].x = x_grid[x_start];
            macros[id].y = y_grid[y_start];
            macros[id].width = x_grid[x_end] - x_grid[x_start];
            macros[id].height = y_grid[y_end] - y_grid[y_start];
        }
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
        macros[id].x = locations[id].0;
        macros[id].y = locations[id].1;
    }
    Ok(())
}

/// Upstream `moveFloorplan`: shift every macro in the sequence pair by one offset.
///
/// ⛔ **A FIXED macro moves too.** There is no `isFixed` test here, and a blockage's soft macro is
/// both fixed and inside the sequence pair — so centralizing a floorplan displaces the proxies of
/// the die's hard blockages along with everything else. The packer would have pinned them; this
/// runs after the packer and nothing puts them back.
pub fn move_floorplan(
    macros: &mut [crate::anneal::SoftMacro],
    order: &[usize],
    offset: (i32, i32),
) {
    for &id in order {
        macros[id].x += offset.0;
        macros[id].y += offset.1;
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
            macros[id].x = 0;
        } else if outline.0 - ux < h_th {
            macros[id].x = outline.0 - macros[id].width;
        }
        if ly < v_th {
            macros[id].y = 0;
        } else if outline.1 - uy < v_th {
            macros[id].y = outline.1 - macros[id].height;
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
                kind: a.kind.unwrap_or(AreaKind::FixedMacro),
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
    kind_of: &dyn Fn(usize) -> AreaKind,
    children_of: &dyn Fn(usize) -> Vec<usize>,
) -> Vec<usize> {
    let mut out = Vec::new();
    for child in children_of(root) {
        match kind_of(child) {
            AreaKind::HardMacroCluster => out.push(child),
            AreaKind::MixedCluster => {
                out.extend(fetch_macro_clusters(child, kind_of, children_of));
            }
            _ => {}
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
/// ⚠️ **The count test is INSIDE the loop**, so a second macro cluster fails it at once rather
/// than after the whole scan — which matters only if a later child would also have failed it, but
/// it is the reference's shape.
///
/// ℹ️ **A root with no children returns TRUE**, vacuously — zero arrays counts as "a single
/// centralized macro array" and the push is skipped. Nothing reaches it: a design with no children
/// under the root has already been refused.
pub fn has_single_centralized_macro_array(children: &[(AreaKind, i64)]) -> bool {
    let mut macro_cluster_count = 0;
    for &(kind, soft_macro_area) in children {
        match kind {
            AreaKind::MixedCluster => return false,
            AreaKind::HardMacroCluster => macro_cluster_count += 1,
            AreaKind::StdCellCluster => {
                if soft_macro_area != 0 {
                    return false;
                }
            }
            // ⚠️ Upstream's `switch` covers only the three cluster types; anything else — an IO
            // cluster, a fixed macro — falls through it without a case and is simply ignored.
            _ => {}
        }
        if macro_cluster_count > 1 {
            return false;
        }
    }
    true
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
    root_kind: AreaKind,
    root_children: &[(AreaKind, i64)],
) -> Result<(), NoPush> {
    if root_kind == AreaKind::HardMacroCluster {
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
    overlaps: &dyn Fn((i32, i32, i32, i32)) -> bool,
) -> ((i32, i32, i32, i32), Vec<PushAttempt>) {
    let mut attempts = Vec::new();
    for &(boundary, distance) in boundaries {
        if distance == 0 {
            continue;
        }
        let moved = move_towards_boundary(cluster_box, boundary, distance);
        if overlaps(moved) {
            // ⚠️ Upstream moves the box back by the same distance rather than restoring a copy.
            attempts.push(PushAttempt { boundary, distance, committed: false });
        } else {
            cluster_box = moved;
            attempts.push(PushAttempt { boundary, distance, committed: true });
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
