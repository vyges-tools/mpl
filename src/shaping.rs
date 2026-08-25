// SPDX-License-Identifier: Apache-2.0
//! Coarse shaping: the set of legal outlines each cluster may take.
//!
//! 🔑 **A macro cluster's shapes are DISCRETE, not a range.** A group of `n` identical macros can
//! only be laid out as a `cols × rows` grid with `cols · rows = n`, so its legal widths are the
//! divisors of `n` scaled by the macro width — which is why every width interval this stage
//! produces is degenerate.
//!
//! ⚠️ This module covers shaping **without** the annealing search. Upstream generates a mixed
//! cluster's tilings by running `SACoreSoftMacro`; a parent with two or more macro-bearing
//! children needs it and is **refused by name** rather than approximated.

use crate::design::Rect;

/// One legal outline.
///
/// ⚠️ Ordered by **width first, then height** — upstream keeps tilings in a `std::set`, so the
/// ordering is part of the output, not an implementation detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Tiling {
    pub width: i64,
    pub height: i64,
}

impl Tiling {
    pub fn area(self) -> i64 {
        self.width * self.height
    }

    /// ⚠️ **height / width**, not the other way round, and computed in `f32` as upstream does.
    pub fn aspect_ratio(self) -> f32 {
        self.height as f32 / self.width as f32
    }
}

/// A range of legal widths. ℹ️ Degenerate (`min == max`) for every macro cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Interval {
    pub min: i64,
    pub max: i64,
}

/// Upstream `generateTilingsForMacroCluster`.
///
/// Every factorisation `cols · rows = n`, with `cols` running **1 to n inclusive**, kept only when
/// the resulting grid fits the outline in both dimensions. Order is `cols` ascending.
///
/// ℹ️ Divisor pairs only — never a ragged grid. That is legal precisely because every macro in the
/// cluster has the same dimensions.
pub fn generate_tilings_for_macro_cluster(
    macro_width: i64,
    macro_height: i64,
    number_of_macros: i64,
    outline: &Rect,
) -> Vec<Tiling> {
    let mut out = Vec::new();
    for cols in 1..=number_of_macros {
        if number_of_macros % cols != 0 {
            continue;
        }
        let rows = number_of_macros / cols;
        let (w, h) = (cols * macro_width, rows * macro_height);
        if w <= outline.x_max - outline.x_min && h <= outline.y_max - outline.y_min {
            out.push(Tiling { width: w, height: h });
        }
    }
    out
}

/// Why a cluster could not be shaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unshapeable {
    pub macro_width: i64,
    pub macro_height: i64,
    pub number_of_macros: i64,
}

/// Upstream `calculateMacroTilings`, minus the reporting.
///
/// 🔑 **When nothing fits, the search is retried with `n + 1` macros.** A prime `n` factors only
/// as `1 × n` and `n × 1`, both of which are the most extreme aspect ratios available; `n + 1`
/// may factor into something squarer that fits. ⚠️ This is a second, different search — not a
/// tolerance or a rounding step — and skipping it turns a shapeable cluster into an MPL-4 error.
///
/// ⚠️ `macro_width`/`macro_height` come from the cluster's **first** hard macro. The array is
/// assumed uniform, which is exactly the invariant `groupSingleMacroClusters` enforces when it
/// refuses to merge macros of different sizes.
pub fn macro_tilings(
    macro_width: i64,
    macro_height: i64,
    number_of_macros: i64,
    outline: &Rect,
) -> Result<Vec<Tiling>, Unshapeable> {
    let mut tilings =
        generate_tilings_for_macro_cluster(macro_width, macro_height, number_of_macros, outline);
    if tilings.is_empty() {
        tilings = generate_tilings_for_macro_cluster(
            macro_width,
            macro_height,
            number_of_macros + 1,
            outline,
        );
    }
    if tilings.is_empty() {
        return Err(Unshapeable { macro_width, macro_height, number_of_macros });
    }
    Ok(tilings)
}

/// Upstream `computeWidthIntervals`: one **degenerate** interval per tiling, sorted by minimum
/// width.
///
/// ⚠️ The sort compares `min` **only** — two intervals with the same minimum keep their relative
/// order, and `std::ranges::sort` is not stable, so a design that produced two such intervals
/// would not have a defined order upstream either. Degenerate intervals from distinct tilings
/// cannot collide, which is why that never arises here.
pub fn compute_width_intervals(tilings: &[Tiling]) -> Vec<Interval> {
    let mut out: Vec<Interval> =
        tilings.iter().map(|t| Interval { min: t.width, max: t.width }).collect();
    out.sort_by_key(|i| i.min);
    out
}

/// Upstream `setRootShapes`: the root takes the floorplan shape exactly.
///
/// ⚠️ Everything downstream reads its outline from the ROOT's bounding box, so a global fence
/// reaches the shaping stage only through this — never by being consulted again.
pub fn root_shape(floorplan: &Rect) -> (Interval, i64) {
    let width = floorplan.x_max - floorplan.x_min;
    (Interval { min: width, max: width }, floorplan.area())
}

// ---------------------------------------------------------------- the recursion

use crate::cluster::{Cluster, ClusterId, ClusterType};

/// What the shaping stage needs that lives outside the tree.
pub struct ShapingCtx<'a> {
    /// 🔑 **The ROOT's outline, not the parent's.** Every tiling is tested against the whole
    /// floorplan, at every depth — a child is never restricted to the space its parent occupies,
    /// because at this stage no parent has a position yet.
    pub outline: crate::design::Rect,
    /// A macro's dimensions **with halo**, by instance.
    pub macro_dims: &'a dyn Fn(usize) -> (i64, i64),
}

/// Why shaping stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapingRefusal {
    /// ⛔ A cluster whose tilings upstream generates by simulated annealing. Refused by name;
    /// **never approximated**, because a plausible tiling set is not the same tiling set.
    NeedsAnnealing(ClusterId),
    /// MPL-4: no arrangement of the cluster's macros fits the outline.
    Unshapeable(ClusterId, Unshapeable),
}

/// Upstream `calculateChildrenTilings`, minus the annealing search.
///
/// The order is the algorithm: **children are shaped before their parent**, because the parent's
/// own shapes are built out of theirs.
pub fn calculate_children_tilings(
    parent: &mut Cluster,
    ctx: &ShapingCtx,
) -> Result<(), ShapingRefusal> {
    // ⚠️ The base case is `num_macro == 0`, NOT "is a leaf". A cluster with no macros has no
    // shape to choose — its area is soft — so shaping skips it and everything below it.
    if parent.num_macro() == 0 {
        return Ok(());
    }

    if parent.cluster_type == ClusterType::HardMacro {
        return macro_cluster_tilings(parent, ctx);
    }

    for child in &mut parent.children {
        // ℹ️ Redundant with the callee's own base case — the same test, one frame apart. Kept
        // because upstream carries it, and a reader comparing the two should find them the same
        // shape. ⚠️ Not mutation-testable for exactly that reason.
        if child.num_macro() > 0 {
            calculate_children_tilings(child, ctx)?;
        }
    }

    // ⚠️ A **fixed** macro cluster still counts here even though it has no tilings of its own —
    // it occupies space the parent has to shape around. That is why `fixed_covers` and
    // `fixed_macros*` need the annealing search despite having one movable macro apiece.
    //
    // ℹ️ `is_fixed_macro ||` is currently redundant: a fixed macro cluster is a `HardMacro`
    // cluster holding its macro, so it reports `num_macro() == 1` — the `fixed_covers` dump says
    // `Type: Fixed Macro Leaf, Macros: 1`. Upstream nonetheless adds a fixed cluster
    // UNCONDITIONALLY and only then tests the count for the others, so the two conditions are
    // kept apart here as well.
    let contributors: Vec<ClusterId> = parent
        .children
        .iter()
        .filter(|c| c.is_fixed_macro || c.num_macro() > 0)
        .map(|c| c.id)
        .collect();

    if contributors.len() == 1 {
        // 🔑 The parent takes the shapes of its only macro-bearing child verbatim. Upstream
        // re-scans the children for the first with `num_macro > 0` rather than reusing the one it
        // just built, so a lone FIXED macro leaves the parent with no tilings at all.
        if let Some(child) = parent.children.iter().find(|c| c.num_macro() > 0) {
            parent.tilings = child.tilings.clone();
        }
        return Ok(());
    }

    // ⛔ Two or more contributors: upstream varies the outline and runs `SACoreSoftMacro` to
    // find tilings. Not built here.
    Err(ShapingRefusal::NeedsAnnealing(parent.id))
}

/// Upstream `calculateMacroTilings` against the tree.
fn macro_cluster_tilings(
    cluster: &mut Cluster,
    ctx: &ShapingCtx,
) -> Result<(), ShapingRefusal> {
    // ⚠️ A fixed macro is not the placer's to shape: it returns with NO tilings, which is not the
    // same as an empty result from a search that found nothing.
    if cluster.is_fixed_macro {
        return Ok(());
    }
    let Some(&first) = cluster.leaf_macros.first() else {
        return Ok(());
    };
    let (w, h) = (ctx.macro_dims)(first);
    match macro_tilings(w, h, cluster.leaf_macros.len() as i64, &ctx.outline) {
        Ok(t) => {
            cluster.tilings = t;
            Ok(())
        }
        Err(e) => Err(ShapingRefusal::Unshapeable(cluster.id, e)),
    }
}

// ---------------------------------------------------------------- pin access depth

/// How deep a pin-access blockage may reach, per axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DepthLimits {
    pub x_min: i64,
    pub x_max: i64,
    pub y_min: i64,
    pub y_max: i64,
}

/// Upstream `computePinAccessDepthLimits`.
///
/// The limits are proportions of the die — **10% deep at most, 4% at least** — computed in `f32`
/// and then truncated, which is what the assignment to an `int` does.
///
/// 🔑 **The tight-design override is all-or-nothing.** When the root's first tiling leaves less
/// margin than the minimum on **BOTH** axes, **BOTH** minima are replaced. It is an `&&`, not a
/// per-axis test: a design tight in one direction only keeps the proportional minimum on both.
/// Upstream's comment names `MockArray` as the design that forced this.
///
/// ⚠️ `(die - tiling) / 2` is integer division, so an odd difference truncates toward zero.
pub fn pin_access_depth_limits(die: &Rect, root_tiling: Tiling) -> DepthLimits {
    let (dx, dy) = (die.x_max - die.x_min, die.y_max - die.y_min);
    let mut limits = DepthLimits {
        x_max: (0.10_f32 * dx as f32) as i64,
        y_max: (0.10_f32 * dy as f32) as i64,
        x_min: (0.04_f32 * dx as f32) as i64,
        y_min: (0.04_f32 * dy as f32) as i64,
    };
    let tiling_min_width = (dx - root_tiling.width) / 2;
    let tiling_min_height = (dy - root_tiling.height) / 2;
    if tiling_min_width < limits.x_min && tiling_min_height < limits.y_min {
        limits.x_min = tiling_min_width;
        limits.y_min = tiling_min_height;
    }
    limits
}

/// Why the base depth could not be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootAreaIsZero;

/// Upstream `computePinAccessBaseDepth`: how deep the blockage for one span of IO should be.
///
/// 🔑 **The standard-cell area is taken from the root's STD-CELL children, and only if that comes
/// to zero is it taken from the MIXED ones.** Two passes, not one condition — a design with both
/// kinds counts only the std-cell clusters, and the mixed ones do not contribute at all.
///
/// ⚠️ **`macro_dominance_factor` squares its complement**: the more of the floorplan the macros
/// occupy, the sharply shallower the blockage. Dropping the square leaves blockages that eat the
/// core on a macro-dominated design.
///
/// ℹ️ The division is in `f64` and the result truncates to an integer.
pub fn pin_access_base_depth(
    std_cell_area_of_children: i64,
    mixed_area_of_children: i64,
    macro_with_halo_area: i64,
    root_area: i64,
    io_span: i64,
) -> Result<i64, RootAreaIsZero> {
    if root_area == 0 {
        return Err(RootAreaIsZero);
    }
    let std_cell_area =
        if std_cell_area_of_children == 0 { mixed_area_of_children } else { std_cell_area_of_children };
    let macro_dominance_factor = macro_with_halo_area as f64 / root_area as f64;
    Ok((std_cell_area as f64 / io_span as f64 * (1.0 - macro_dominance_factor).powi(2)) as i64)
}

/// Upstream `setPlacementBlockages`: every blockage the block holds, taken as it stands.
///
/// ℹ️ No filtering, no clipping, no union — the placer wants each rectangle. Contrast
/// [`crate::feasibility::movable_cells_fit`], which unions the same blockages and clips them to
/// the placement area, because there the question is how much AREA they occupy.
pub fn placement_blockages(blockages: &[Rect]) -> Vec<Rect> {
    blockages.to_vec()
}

// ---------------------------------------------------------------- the composer

/// Everything `runCoarseShaping` reads that does not live on the tree.
pub struct CoarseInput<'a> {
    pub die: Rect,
    pub floorplan: Rect,
    /// ⚠️ MPL-27: the whole stage returns after the root's shape when this is set.
    pub has_only_macros: bool,
    /// A design with IO pads casts no pin-access blockages at all.
    pub has_io_pads: bool,
    /// The top module's standard-cell area. Zero means every blockage would have zero depth.
    pub top_std_cell_area: i64,
    pub blockages: &'a [Rect],
    /// A macro's dimensions **with halo**, by instance.
    pub macro_dims: &'a dyn Fn(usize) -> (i64, i64),
    pub io_bundles: &'a [crate::regions::IoRegion],
    pub fixed_ios: i64,
    pub constrained_regions: &'a [crate::regions::IoRegion],
    pub unfixed_ios: i64,
    pub available_regions: &'a [crate::regions::BoundaryRegion],
    pub any_blocked_regions: bool,
    /// `computePinAccessBaseDepth`, which needs tree state this function does not carry.
    pub base_depth: &'a dyn Fn(i64) -> i64,
}

/// What the stage produced.
#[derive(Debug, Clone, PartialEq)]
pub struct CoarseShaping {
    /// The root's single width interval and its area.
    pub root_shape: (Interval, i64),
    pub depth_limits: DepthLimits,
    /// ⚠️ In creation order: bundles, then available regions, then constraint regions. Upstream
    /// appends to one list in that sequence and the placer reads it in order.
    pub io_blockages: Vec<Rect>,
    pub placement_blockages: Vec<Rect>,
}

/// Upstream `runCoarseShaping`, minus the annealing tiling search.
///
/// 🔑 **The order is the algorithm.** The root's shape is set first because everything downstream
/// measures against the ROOT's bounding box; the tilings come next because the depth limits read
/// the root's first tiling; the blockages last because they are clamped by those limits.
pub fn run_coarse_shaping(
    root: &mut Cluster,
    input: &CoarseInput,
) -> Result<CoarseShaping, ShapingRefusal> {
    let root_shape = root_shape(&input.floorplan);

    // ⚠️ MPL-27, and it returns: a design of nothing but macros gets its root retyped and no
    // tilings, no pin-access blockages and no placement blockages at all.
    if input.has_only_macros {
        root.cluster_type = ClusterType::HardMacro;
        return Ok(CoarseShaping {
            root_shape,
            depth_limits: DepthLimits::default(),
            io_blockages: Vec::new(),
            placement_blockages: Vec::new(),
        });
    }

    let ctx = ShapingCtx { outline: input.floorplan, macro_dims: input.macro_dims };
    calculate_children_tilings(root, &ctx)?;

    let io_blockages = pin_access_blockages(root, input);
    Ok(CoarseShaping {
        root_shape,
        depth_limits: depth_limits_for(root, input).unwrap_or_default(),
        io_blockages,
        placement_blockages: placement_blockages(input.blockages),
    })
}

/// The limits, or `None` when the stage does not reach them.
fn depth_limits_for(root: &Cluster, input: &CoarseInput) -> Option<DepthLimits> {
    if input.has_io_pads || input.top_std_cell_area == 0 {
        return None;
    }
    // ⚠️ Upstream reads `getTilings().front()` unguarded. A root with no tilings would be a
    // read past the end there; here it is simply no limits, and therefore no blockages.
    Some(pin_access_depth_limits(&input.die, *root.tilings.first()?))
}

/// Upstream `createPinAccessBlockages`: two guards, then the three builders in order.
fn pin_access_blockages(root: &Cluster, input: &CoarseInput) -> Vec<Rect> {
    let Some(limits) = depth_limits_for(root, input) else {
        return Vec::new();
    };
    let mut out = crate::regions::blockages_for_regions(
        input.io_bundles,
        input.fixed_ios,
        input.base_depth,
        &limits,
    );
    out.extend(crate::regions::blockages_for_available_regions(
        input.available_regions,
        input.any_blocked_regions,
        input.base_depth,
        &limits,
    ));
    out.extend(crate::regions::blockages_for_regions(
        input.constrained_regions,
        input.unfixed_ios,
        input.base_depth,
        &limits,
    ));
    out
}
