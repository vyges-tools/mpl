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
        penalty += area_to_microns_f32(best, dbu_per_micron);
    }
    penalty / guides.len() as f32
}

/// `dbuAreaToMicrons`, narrowed to the `f32` the penalty accumulates in.
fn area_to_microns_f32(dbu_area: i64, dbu_per_micron: i32) -> f32 {
    let d = dbu_per_micron as f64;
    (dbu_area as f64 / (d * d)) as f32
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
