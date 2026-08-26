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
use crate::trace::CoarseTrace;

/// What the shaping stage needs that lives outside the tree.
pub struct ShapingCtx<'a> {
    /// 🔑 **The ROOT's outline, not the parent's.** Every tiling is tested against the whole
    /// floorplan, at every depth — a child is never restricted to the space its parent occupies,
    /// because at this stage no parent has a position yet.
    pub outline: crate::design::Rect,
    /// A macro's dimensions **with halo**, by instance.
    pub macro_dims: &'a dyn Fn(usize) -> (i64, i64),
    /// A macro's bounding box **with halo**, in absolute database units, by instance.
    ///
    /// ⚠️ Needed only for a FIXED macro, whose position is part of its contribution — every other
    /// macro is placed by the search and its starting position is irrelevant.
    pub macro_bbox: &'a dyn Fn(usize) -> Rect,
    pub dbu_per_micron: i32,
    /// ⚠️ **`resetSAParameters` zeroes the resize share for a design with no standard cells**, and
    /// it runs before this stage. The two probability sets are otherwise identical.
    pub has_std_cells: bool,
    pub search: crate::anneal::TilingSearch,
}

impl ShapingCtx<'_> {
    /// The action shares the search is driven with, already normalised.
    ///
    /// ⚠️ The reference divides each share by the sum of all five, and the sums differ between
    /// the two cases — `1.2` normally and `0.8` once resize is zeroed — so the four swap
    /// probabilities are NOT the same in both.
    pub fn probabilities(&self) -> crate::anneal::ActionProbabilities {
        let resize = if self.has_std_cells { 0.4 } else { 0.0 };
        crate::anneal::ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, resize)
    }
}

/// Why shaping stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapingRefusal {
    /// ⛔ A cluster whose tilings upstream generates by simulated annealing. Refused by name;
    /// **never approximated**, because a plausible tiling set is not the same tiling set.
    /// ℹ️ No longer produced — the search is built. Kept so a caller matching on it still compiles.
    NeedsAnnealing(ClusterId),
    /// MPL-3: the search produced no tiling that fits the outline.
    NoValidTilings(ClusterId),
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
    calculate_children_tilings_traced(parent, ctx, &mut CoarseTrace::silent())
}

/// As [`calculate_children_tilings`], recording upstream's `coarse_shaping` trace as it goes.
///
/// 🔑 **The trace sites are part of the algorithm's shape, not decoration.** Each one sits where
/// upstream's `debugPrint` sits, so the recorded order IS the traversal order — which is the
/// property the oracle actually scores.
pub fn calculate_children_tilings_traced(
    parent: &mut Cluster,
    ctx: &ShapingCtx,
    trace: &mut CoarseTrace,
) -> Result<(), ShapingRefusal> {
    // ⚠️ The base case is `num_macro == 0`, NOT "is a leaf". A cluster with no macros has no
    // shape to choose — its area is soft — so shaping skips it and everything below it.
    if parent.num_macro() == 0 {
        return Ok(());
    }

    trace.determine_shapes(&parent.name);

    if parent.cluster_type == ClusterType::HardMacro {
        trace.is_macro_cluster(&parent.name);
        return macro_cluster_tilings(parent, ctx, trace);
    }

    // ⚠️ Upstream guards BOTH visiting lines and the loop with `!getChildren().empty()`. The loop
    // alone is the same computation either way, but a childless mixed cluster must print neither
    // line — an unguarded loop that iterates zero times would still print both.
    if !parent.children.is_empty() {
        trace.started_visiting(&parent.name);
        for child in &mut parent.children {
            // ℹ️ Redundant with the callee's own base case — the same test, one frame apart.
            // Kept because upstream carries it, and a reader comparing the two should find them
            // the same shape. ⚠️ Not mutation-testable for exactly that reason.
            if child.num_macro() > 0 {
                calculate_children_tilings_traced(child, ctx, trace)?;
            }
        }
        trace.done_visiting(&parent.name);
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

    // Two or more contributors: vary the outline and anneal, as upstream does.
    let (macros, curves) = build_soft_macros(parent, ctx);
    match crate::anneal::search_tilings(
        &macros,
        &curves,
        (ctx.outline.x_max - ctx.outline.x_min) as i32,
        (ctx.outline.y_max - ctx.outline.y_min) as i32,
        ctx.dbu_per_micron,
        ctx.probabilities(),
        &ctx.search,
    ) {
        Ok(result) => {
            // ⚠️ Traced BEFORE the filter and over EVERY tiling found — the reference prints the
            // unfiltered list here and only then narrows it.
            for &(width, height) in &result.all {
                trace.mixed_tiling_candidate(width, height, ctx.search.min_ar);
            }
            parent.tilings = result
                .chosen
                .iter()
                .map(|&(width, height)| Tiling { width: width as i64, height: height as i64 })
                .collect();
            // ⚠️ The summary line reports the KEPT list, not the one above.
            trace.mixed_cluster_tilings(&parent.name, &result.chosen);
            Ok(())
        }
        // MPL-3: the search found nothing that fits.
        Err(_) => Err(ShapingRefusal::NoValidTilings(parent.id)),
    }
}

/// The soft macros a parent's children contribute to the search.
///
/// 🔑 **Order follows the CHILD order**, fixed and movable interleaved — the sequence pair indexes
/// into this list, so a different order is a different search.
///
/// ⚠️ A fixed macro is clipped to the outline and translated to be RELATIVE to it, and carries no
/// shape curve: it cannot be resized, and an empty curve is what makes a resize action spend no
/// randomness on it.
fn build_soft_macros(
    parent: &Cluster,
    ctx: &ShapingCtx,
) -> (Vec<crate::anneal::SoftMacro>, Vec<crate::anneal::ShapeCurve>) {
    use crate::anneal::{shape_curve_from_intervals, shape_curve_from_tilings, ShapeCurve, SoftMacro};

    let outline = ctx.outline;
    let mut macros = Vec::new();
    let mut curves = Vec::new();

    for child in &parent.children {
        if child.is_fixed_macro {
            let Some(&inst) = child.leaf_macros.first() else { continue };
            let bbox = (ctx.macro_bbox)(inst);
            // ⚠️ Clipped to the outline, THEN moved so the outline's corner is the origin.
            let x_min = bbox.x_min.max(outline.x_min);
            let y_min = bbox.y_min.max(outline.y_min);
            let x_max = bbox.x_max.min(outline.x_max);
            let y_max = bbox.y_max.min(outline.y_max);
            let width = (x_max - x_min) as i32;
            let height = (y_max - y_min) as i32;
            macros.push(SoftMacro {
                x: (x_min - outline.x_min) as i32,
                y: (y_min - outline.y_min) as i32,
                width,
                height,
                fixed: true,
                area: width as i64 * height as i64,
                // ⚠️ Its cluster IS a macro cluster, and the resize action tests that FIRST —
                // which routes it to a random resize that then finds no curve and draws nothing.
                is_macro_cluster: true,
            });
            curves.push(ShapeCurve::default());
            continue;
        }

        if child.num_macro() == 0 {
            continue;
        }

        if child.cluster_type == ClusterType::HardMacro {
            let tilings: Vec<(i32, i32)> =
                child.tilings.iter().map(|t| (t.width as i32, t.height as i32)).collect();
            let (curve, width, height, area) = shape_curve_from_tilings(&tilings);
            macros.push(SoftMacro {
                width,
                height,
                area,
                is_macro_cluster: true,
                ..Default::default()
            });
            curves.push(curve);
        } else {
            let intervals: Vec<crate::anneal::Interval> = compute_width_intervals(&child.tilings)
                .into_iter()
                .map(|i| crate::anneal::Interval { min: i.min as i32, max: i.max as i32 })
                .collect();
            // ⚠️ "We can use the area of any tiling" — upstream takes the first.
            let area = child.tilings.first().map(|t| t.area()).unwrap_or(0);
            match shape_curve_from_intervals(&intervals, area) {
                Some((curve, width, height, area)) => {
                    macros.push(SoftMacro { width, height, area, ..Default::default() });
                    curves.push(curve);
                }
                None => {
                    // Upstream's `setShapes` returns leaving the macro at its defaults.
                    macros.push(SoftMacro::default());
                    curves.push(ShapeCurve::default());
                }
            }
        }
    }
    (macros, curves)
}

/// Upstream `calculateMacroTilings` against the tree.
fn macro_cluster_tilings(
    cluster: &mut Cluster,
    ctx: &ShapingCtx,
    trace: &mut CoarseTrace,
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
    // ⚠️ The cluster's OWN macro count, kept for the trace. `macro_tilings` may succeed only on
    // its `n + 1` retry, and upstream still reports `n` — printing the retry's count would
    // misreport every cluster that needed it.
    let number_of_macros = cluster.leaf_macros.len();
    match macro_tilings(w, h, number_of_macros as i64, &ctx.outline) {
        Ok(t) => {
            cluster.tilings = t;
            // ⚠️ After the assignment, as upstream traces after `setTilings` — an empty tiling
            // list still prints a header line.
            trace.hard_cluster_tilings(&cluster.name, number_of_macros, &cluster.tilings);
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
    /// A macro's bounding box **with halo**, in absolute database units, by instance.
    pub macro_bbox: &'a dyn Fn(usize) -> Rect,
    /// ⚠️ Whether the design has ANY standard cells — it selects the action probabilities, because
    /// `resetSAParameters` zeroes the resize share when it does not.
    pub has_std_cells: bool,
    pub search: crate::anneal::TilingSearch,
    pub io_bundles: &'a [crate::regions::IoRegion],
    pub fixed_ios: i64,
    pub constrained_regions: &'a [crate::regions::IoRegion],
    pub unfixed_ios: i64,
    /// The die-edge stretches where pins may NOT go, as read from the database.
    ///
    /// 🔑 **The available regions are derived from this INSIDE the stage, not handed in.**
    /// Upstream computes them in `searchAvailableRegionsForUnconstrainedPins`, called between the
    /// tilings and the blockages — so their position in the order is part of the algorithm, and a
    /// caller that computed them earlier would be free to do so against different state.
    pub blocked_regions_for_pins: &'a [Rect],
    /// Upstream `treeHasUnconstrainedIOs`: at least one unplaced-IO cluster is unconstrained.
    ///
    /// ⚠️ **Gates the SEARCH, not the builder.** With this false the available-region list stays
    /// empty, yet `createBlockagesForAvailableRegions` still runs — its own guard is on the
    /// BLOCKED regions. Upstream then divides by a zero span; the corpus never reaches it.
    pub has_unconstrained_ios: bool,
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
    run_coarse_shaping_traced(root, input, 0, &mut CoarseTrace::silent())
}

/// As [`run_coarse_shaping`], recording upstream's `coarse_shaping` trace.
///
/// ⚠️ `dbu_per_micron` is a REPORTING input, not a shaping one — half these lines are in microns
/// and half in database units, and upstream converts at each print site with `dbuToMicrons`. It
/// is deliberately not on [`CoarseInput`]: nothing the stage computes depends on it.
pub fn run_coarse_shaping_traced(
    root: &mut Cluster,
    input: &CoarseInput,
    dbu_per_micron: i32,
    trace: &mut CoarseTrace,
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

    let ctx = ShapingCtx {
        outline: input.floorplan,
        macro_dims: input.macro_dims,
        macro_bbox: input.macro_bbox,
        dbu_per_micron,
        has_std_cells: input.has_std_cells,
        search: input.search,
    };
    calculate_children_tilings_traced(root, &ctx, trace)?;

    // 🔑 **Here, between the tilings and the blockages** — upstream's own position for
    // `searchAvailableRegionsForUnconstrainedPins`. It reads no tiling, so moving it earlier
    // would compute the same regions today; it is kept here because the order IS the algorithm,
    // and the next stage to land is the one that makes that stop being true.
    let available_regions = search_available_regions_for_unconstrained_pins(input, trace);

    let io_blockages =
        pin_access_blockages(root, input, &available_regions, dbu_per_micron, trace);
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

/// Upstream `searchAvailableRegionsForUnconstrainedPins`.
///
/// ⛔ **Returns nothing at all when no unplaced-IO cluster is unconstrained.** That is the whole
/// of the gate: with it closed the design casts no available-region blockages, however much of
/// its boundary is free.
fn search_available_regions_for_unconstrained_pins(
    input: &CoarseInput,
    trace: &mut CoarseTrace,
) -> Vec<crate::regions::BoundaryRegion> {
    if !input.has_unconstrained_ios {
        return Vec::new();
    }
    crate::regions::available_regions_traced(&input.die, input.blocked_regions_for_pins, trace)
        .into_iter()
        .map(|line| crate::regions::BoundaryRegion {
            boundary: crate::regions::boundary_of(&input.die, &line),
            line,
        })
        .collect()
}

/// Upstream `createPinAccessBlockages`: two guards, then the three builders in order.
fn pin_access_blockages(
    root: &Cluster,
    input: &CoarseInput,
    available_regions: &[crate::regions::BoundaryRegion],
    dbu_per_micron: i32,
    trace: &mut CoarseTrace,
) -> Vec<Rect> {
    let Some(limits) = depth_limits_for(root, input) else {
        return Vec::new();
    };
    // ⚠️ Emitted HERE, not with the other results: upstream prints the table from inside
    // `computePinAccessDepthLimits`, which runs after the two guards and BEFORE the three
    // blockage builders. Printing it later would put it after the blockage lines it bounds.
    trace.depth_limits(&limits, dbu_per_micron);
    let mut out = crate::regions::blockages_for_regions_traced(
        input.io_bundles,
        input.fixed_ios,
        input.base_depth,
        &limits,
        dbu_per_micron,
        trace,
    );
    out.extend(crate::regions::blockages_for_available_regions_traced(
        available_regions,
        !input.blocked_regions_for_pins.is_empty(),
        input.base_depth,
        &limits,
        dbu_per_micron,
        trace,
    ));
    out.extend(crate::regions::blockages_for_regions_traced(
        input.constrained_regions,
        input.unfixed_ios,
        input.base_depth,
        &limits,
        dbu_per_micron,
        trace,
    ));
    out
}
