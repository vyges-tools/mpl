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
