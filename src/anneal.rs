// SPDX-License-Identifier: Apache-2.0
//! The annealing search that generates a mixed cluster's tilings.
//!
//! 🔑 **A floorplan here is a SEQUENCE PAIR**, not a set of coordinates. Two orderings of the same
//! macros encode the whole packing: `pack_floorplan` turns them into positions, and every
//! perturbation the annealer makes is a change to the orderings rather than to any position.
//!
//! ⚠️ **Coordinates are 32-bit.** Upstream holds a soft macro's `x`, `y`, `width` and `height` as
//! `int` and only widens to 64 bits to compute an area. Widening them here would change nothing
//! on a real design and would quietly stop reproducing the arithmetic being matched.

/// One placeable rectangle, as the annealer sees it.
///
/// ⚠️ A **fixed** macro keeps the position it was given: the packer skips the assignment but still
/// lets the macro push the accumulated edge along, so it displaces its neighbours without moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SoftMacro {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub fixed: bool,
    /// ⚠️ Carried on the macro, not recomputed from `width * height` — the resize paths set it
    /// from an interval CORNER, so it routinely disagrees with the current shape's product.
    pub area: i64,
    /// A cluster of hard macros. ⛔ `set_width`/`set_height` refuse to touch one; only
    /// [`resize_randomly`] moves it, which is why the resize action tests this first.
    pub is_macro_cluster: bool,
}

/// The two orderings that encode a packing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SequencePair {
    pub pos: Vec<usize>,
    pub neg: Vec<usize>,
}

impl SequencePair {
    pub fn len(&self) -> usize {
        self.pos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }
}

/// Upstream `packFloorplan`: place every macro from the sequence pair, and return the bounding
/// `(width, height)`.
///
/// 🔑 **Two passes over the same scratch vector.** The x pass walks the positive sequence
/// forwards; the y pass walks it **reversed**. That reversal is the whole of the sequence-pair
/// rule — the same pair read one way gives left-of relations and the other way gives below-of.
///
/// ℹ️ **The `break` is an early exit, not a rule.** The accumulated edge is monotone
/// non-decreasing — every update writes one value `c` across `[p, k)` where `c > accumulated[p]`,
/// and `accumulated[p-1] <= accumulated[p] < c` keeps it so — therefore once `c` fails to improve
/// position `j`, it cannot improve any later one either. Running the loop to the end would give
/// the SAME answer, more slowly.
/// ⛔ **So do not add a mutation that deletes it.** It would be equivalent, and an equivalent
/// mutant reported as a hole teaches people to ignore holes.
///
/// ℹ️ An empty sequence pair returns `(0, 0)`. Upstream reads `accumulated_length.back()` on an
/// empty vector instead, which is undefined; it cannot arise, because a cluster with nothing in it
/// is never shaped.
pub fn pack_floorplan(macros: &mut [SoftMacro], sp: &SequencePair) -> (i32, i32) {
    let n = sp.len();
    if n == 0 {
        return (0, 0);
    }
    debug_assert_eq!(sp.pos.len(), sp.neg.len(), "the two sequences are a pair");

    // Per macro: (position in the positive sequence, position in the negative sequence).
    let mut sp_pos = vec![(0usize, 0usize); n];
    for i in 0..n {
        sp_pos[sp.pos[i]].0 = i;
        sp_pos[sp.neg[i]].1 = i;
    }

    let mut accumulated = vec![0i32; n];
    for &id in &sp.pos {
        let neg_pos = sp_pos[id].1;
        if !macros[id].fixed {
            macros[id].x = accumulated[neg_pos];
        }
        let current = macros[id].x + macros[id].width;
        for slot in accumulated.iter_mut().skip(neg_pos) {
            if current > *slot {
                *slot = current;
            } else {
                break;
            }
        }
    }
    let width = accumulated[n - 1];

    // ⚠️ The y pass reuses `sp_pos` and `accumulated` rather than allocating, so both have to be
    // rewritten in full — `accumulated` back to zero, and the positions against the REVERSED
    // positive sequence.
    let mut reversed = vec![0usize; n];
    for i in 0..n {
        reversed[i] = sp.pos[n - 1 - i];
    }
    for i in 0..n {
        // ℹ️ `.0` is written and never read again. Upstream writes it too; kept so the two read
        // as the same loop.
        sp_pos[reversed[i]].0 = i;
        sp_pos[sp.neg[i]].1 = i;
        accumulated[i] = 0;
    }

    for i in 0..n {
        let id = reversed[i];
        let neg_pos = sp_pos[id].1;
        if !macros[id].fixed {
            macros[id].y = accumulated[neg_pos];
        }
        let current = macros[id].y + macros[id].height;
        for slot in accumulated.iter_mut().skip(neg_pos) {
            if current > *slot {
                *slot = current;
            } else {
                break;
            }
        }
    }
    let height = accumulated[n - 1];

    (width, height)
}

// ---------------------------------------------------------------- the perturbations

use crate::rng::{canonical_f32, uniform_int, Mt19937};

/// Which perturbation a step chose. ⚠️ The numbering is upstream's `action_id_`, and it is
/// observable in its debug output, so it is kept rather than renumbered from zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SwapPositive = 1,
    SwapNegative = 2,
    SwapBoth = 3,
    Exchange = 4,
    Resize = 5,
}

/// The five action probabilities, already divided by their sum.
///
/// ⚠️ **`resize` is never compared against.** Upstream builds four cumulative thresholds and lets
/// resize be the `else`, so its share is whatever is left. Storing it is still worth it: the
/// caller normalises by the sum of all five, and that sum needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActionProbabilities {
    pub pos_swap: f32,
    pub neg_swap: f32,
    pub double_swap: f32,
    pub exchange: f32,
    pub resize: f32,
}

impl ActionProbabilities {
    /// Each share divided by the sum of all five, as the caller passes them.
    pub fn normalized(pos: f32, neg: f32, double: f32, exchange: f32, resize: f32) -> Self {
        let sum = pos + neg + double + exchange + resize;
        Self {
            pos_swap: pos / sum,
            neg_swap: neg / sum,
            double_swap: double / sum,
            exchange: exchange / sum,
            resize: resize / sum,
        }
    }

    /// Upstream's dispatch: four running totals, each tested with `<=`.
    ///
    /// ⚠️ **`<=`, not `<`.** With a draw landing exactly on a boundary the earlier action wins.
    /// The draw is a float in `[0, 1)`, so the boundaries are reachable.
    ///
    /// ⛔ A consequence worth knowing: **an action whose probability is zero can still fire.**
    /// With `pos_swap` at zero the first test is `0.0 <= 0.0`, and the generator's word `0` maps
    /// to exactly `0.0`. Using `<` would be more sensible and would diverge from the reference on
    /// that one draw.
    pub fn action_for(&self, draw: f32) -> Action {
        let one = self.pos_swap;
        let two = one + self.neg_swap;
        let three = two + self.double_swap;
        let four = three + self.exchange;
        if draw <= one {
            Action::SwapPositive
        } else if draw <= two {
            Action::SwapNegative
        } else if draw <= three {
            Action::SwapBoth
        } else if draw <= four {
            Action::Exchange
        } else {
            Action::Resize
        }
    }
}

/// Upstream `generateRandomIndices`: two DISTINCT positions in the sequence.
///
/// ⚠️ **The retry loop redraws only the second index**, and it draws again for as long as they
/// match — so the number of generator words consumed is not fixed. With `n` positions the chance
/// of a repeat is `1/n`, which for the two-macro clusters this code sees is one time in two.
///
/// ⛔ Callers must reject `n <= 1` before calling: upstream computes `pos_seq_.size() - 1` on an
/// unsigned type, so a single-element sequence would ask for a range of `SIZE_MAX` and a zero
/// element sequence would never terminate.
pub fn generate_random_indices(rng: &mut Mt19937, n: usize) -> (usize, usize) {
    debug_assert!(n > 1, "the caller guards this; upstream underflows instead");
    let index1 = uniform_int(rng, n as u32) as usize;
    let mut index2 = uniform_int(rng, n as u32) as usize;
    while index1 == index2 {
        index2 = uniform_int(rng, n as u32) as usize;
    }
    (index1, index2)
}

/// Upstream `singleSeqSwap`: swap two positions in ONE of the sequences.
///
/// ℹ️ Swapping within a single sequence changes the relation between the two macros in one
/// dimension only — left-of becomes above, or the reverse.
pub fn single_seq_swap(rng: &mut Mt19937, sp: &mut SequencePair, positive: bool) {
    if sp.len() <= 1 {
        return;
    }
    let (i, j) = generate_random_indices(rng, sp.len());
    if positive {
        sp.pos.swap(i, j);
    } else {
        sp.neg.swap(i, j);
    }
}

/// Upstream `doubleSeqSwap`: the SAME two positions, in both sequences.
///
/// ⚠️ Both sequences are swapped at the same INDICES, which is not the same as swapping the same
/// two macros — the macro at position `i` of one sequence is generally not the one at position
/// `i` of the other. Exchanging the macros is [`exchange_macros`], a different action.
pub fn double_seq_swap(rng: &mut Mt19937, sp: &mut SequencePair) {
    if sp.len() <= 1 {
        return;
    }
    let (i, j) = generate_random_indices(rng, sp.len());
    sp.pos.swap(i, j);
    sp.neg.swap(i, j);
}

/// Upstream `exchangeMacros`: swap two macros in the positive sequence, then find those same two
/// MACROS in the negative sequence and swap them there.
///
/// 🔑 **This is the action that keeps a macro's pair of positions together.** `double_seq_swap`
/// swaps positions and so generally moves four macros; this moves exactly two.
///
/// ⚠️ The negative-sequence search runs AFTER the positive swap, so it looks for the macros at
/// their new positions. Reading `pos[i]` before the swap finds the wrong pair.
///
/// ⛔ Upstream raises MPL-18 if either macro is missing from the negative sequence. Here that is
/// a `debug_assert`: the two sequences are permutations of the same set by construction, and a
/// divergence is a defect in this file rather than anything a design can cause.
pub fn exchange_macros(rng: &mut Mt19937, sp: &mut SequencePair) {
    if sp.len() <= 1 {
        return;
    }
    let (i, j) = generate_random_indices(rng, sp.len());
    sp.pos.swap(i, j);

    let (a, b) = (sp.pos[i], sp.pos[j]);
    let mut neg_i = None;
    let mut neg_j = None;
    for (k, &id) in sp.neg.iter().enumerate() {
        if id == a {
            neg_i = Some(k);
        }
        if id == b {
            neg_j = Some(k);
        }
    }
    match (neg_i, neg_j) {
        (Some(x), Some(y)) => sp.neg.swap(x, y),
        _ => debug_assert!(false, "the sequences diverged: {a} or {b} is not in the negative one"),
    }
}

/// One draw, and the action it selects — upstream `perturb`'s first two statements.
///
/// ⛔ **An empty macro list consumes NOTHING.** Upstream returns before the draw, so a cluster
/// with no macros leaves the generator untouched; drawing and discarding would desynchronise
/// every later step.
pub fn choose_action(
    rng: &mut Mt19937,
    probabilities: &ActionProbabilities,
    macro_count: usize,
) -> Option<Action> {
    if macro_count == 0 {
        return None;
    }
    Some(probabilities.action_for(canonical_f32(rng)))
}

// ---------------------------------------------------------------- the cost

impl SoftMacro {
    /// `(x_min, y_min, x_max, y_max)`, as upstream's `getBBox`.
    pub fn bbox(&self) -> (i32, i32, i32, i32) {
        (self.x, self.y, self.x + self.width, self.y + self.height)
    }
}

/// `dbuAreaToMicrons`: an area divided by the square of the units per micron, in `f64`.
fn area_to_microns(dbu_area: i64, dbu_per_micron: i32) -> f64 {
    let d = dbu_per_micron as f64;
    dbu_area as f64 / (d * d)
}

/// Upstream `getAreaPenalty`: the packing's area over the outline's, as a ratio.
///
/// ⚠️ **Both sides are converted to microns first**, and the conversion then cancels. Reproduced
/// as written rather than simplified to `area / outline_area`: the two divisions round, and the
/// point of this file is to be the same arithmetic, not equivalent arithmetic.
///
/// ⚠️ **`calNormCost` does NOT divide this by its normalisation factor** — unlike every other
/// term, the area penalty enters the cost raw. The factor is computed and then used only as a
/// `> 0` guard.
pub fn area_penalty(width: i32, height: i32, outline_area: i64, dbu_per_micron: i32) -> f32 {
    // `getArea` widens only the second operand, which is enough: both are `int`.
    let area = width as i64 * height as i64;
    (area_to_microns(area, dbu_per_micron) / area_to_microns(outline_area, dbu_per_micron)) as f32
}

/// Upstream `calOutlinePenalty`: how much the packing's bounding box overhangs the outline.
///
/// 🔑 **Zero when the packing fits.** `max` on each axis pins the box to the outline whenever the
/// packing is smaller, so the product equals the outline's area and the difference vanishes. The
/// penalty only measures overhang.
///
/// ⛔ **The int64 difference is NARROWED TO `f32` BEFORE the division, not after.** Upstream
/// assigns it to a `float` member and divides on the next statement. With a die area in the
/// billions of database units that narrowing loses bits, and computing the whole expression in
/// `f64` before narrowing gives a different answer.
pub fn outline_penalty(width: i32, height: i32, outline_width: i32, outline_height: i32) -> f32 {
    let max_width = outline_width.max(width);
    let max_height = outline_height.max(height);
    let outline_area = outline_width as i64 * outline_height as i64;
    let overhang = (max_width as i64 * max_height as i64) - outline_area;
    let narrowed = overhang as f32;
    narrowed / outline_area as f32
}

/// Upstream `calFixedMacrosPenalty`: the total area a movable macro steals from a fixed one.
///
/// ⛔ **This term is live even for shaping.** Its weight is a hardcoded `100.0` on the class, not
/// one of the soft weights the shaping caller zeroes — so the three `fixed_*` designs are scored
/// on it while every other soft penalty drops out.
///
/// ⚠️ **The accumulation order is fixed macros OUTER, sequence INNER.** Floating-point addition is
/// not associative, so swapping the loops changes the sum's last bits.
///
/// ⚠️ **`< 0`, not `<= 0`.** A zero-area touching overlap is not skipped; it adds nothing, so the
/// distinction is invisible in the result — but an empty intersection whose two dimensions are
/// BOTH negative has a positive `area()`, and it is this guard that stops it being counted.
pub fn fixed_macros_penalty(
    macros: &[SoftMacro],
    fixed: &[(i32, i32, i32, i32)],
    sp: &SequencePair,
    dbu_per_micron: i32,
) -> f32 {
    if fixed.is_empty() {
        return 0.0;
    }
    let mut penalty = 0.0f32;
    for &(fx0, fy0, fx1, fy1) in fixed {
        for &id in &sp.pos {
            let macro_ = &macros[id];
            if macro_.fixed {
                continue;
            }
            let (mx0, my0, mx1, my1) = macro_.bbox();
            let (x0, y0) = (fx0.max(mx0), fy0.max(my0));
            let (x1, y1) = (fx1.min(mx1), fy1.min(my1));
            let (dx, dy) = (x1 - x0, y1 - y0);
            if dx < 0 || dy < 0 {
                continue;
            }
            penalty += area_to_microns(dx as i64 * dy as i64, dbu_per_micron) as f32;
        }
    }
    penalty
}

/// The weights the tiling search is built with.
///
/// 🔑 **Only two of the nine terms are alive.** The shaping caller passes area `1.0` and outline
/// `1000.0`, zeroes wirelength, guidance and fence, and passes a default `SASoftWeights` — so
/// boundary, notch and soft blockage vanish too. What remains is area, outline, and the fixed
/// macros term whose weight is a class constant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapingWeights {
    pub area: f32,
    pub outline: f32,
    pub fixed_macros: f32,
}

impl Default for ShapingWeights {
    fn default() -> Self {
        Self { area: 1.0, outline: 1000.0, fixed_macros: 100.0 }
    }
}

/// The normalisation factors `initialize` measures, after its `<= 1e-4` floor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Normalization {
    pub area: f32,
    pub outline: f32,
    pub fixed_macros: f32,
}

impl Default for Normalization {
    fn default() -> Self {
        Self { area: 1.0, outline: 1.0, fixed_macros: 1.0 }
    }
}

/// One packing's penalties, as `calPenalty` leaves them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Penalties {
    pub area: f32,
    pub outline: f32,
    pub fixed_macros: f32,
}

/// Upstream `SACoreSoftMacro::calNormCost`, reduced to the terms shaping leaves alive.
///
/// ⚠️ **Each term is gated on its normalisation factor being `> 0`**, and `initialize` floors any
/// factor at or below `1e-4` to `1.0` — so in practice every gate is open. The gate is reproduced
/// because a factor of exactly zero would otherwise divide by zero rather than drop the term.
///
/// ⚠️ **The addition order is area, outline, then fixed macros**, matching the source. Reordering
/// changes the last bits, and the accept/reject test compares these values directly.
pub fn norm_cost(p: &Penalties, w: &ShapingWeights, n: &Normalization) -> f32 {
    let mut cost = 0.0f32;
    if n.area > 0.0 {
        // ⚠️ No division here — see `area_penalty`.
        cost += w.area * p.area;
    }
    if n.outline > 0.0 {
        cost += w.outline * p.outline / n.outline;
    }
    if n.fixed_macros > 0.0 {
        cost += w.fixed_macros * p.fixed_macros / n.fixed_macros;
    }
    cost
}

/// Upstream `calAverage`: the mean, or zero for an empty list.
///
/// ⚠️ **`std::accumulate` from `0.0f`, so the sum is in `f32`** — it accumulates in the initial
/// value's type, and summing in `f64` before narrowing gives a different mean.
pub fn average(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let sum = values.iter().fold(0.0f32, |a, b| a + b);
    sum / values.len() as f32
}

// ---------------------------------------------------------------- shapes

/// A width or height range a soft macro may take.
///
/// ⚠️ **32-bit, like upstream's `Interval`.** The shaping stage's own `Interval` is 64-bit because
/// it carries database units around; inside the annealer the arithmetic is `int`, and the two must
/// not be conflated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub min: i32,
    pub max: i32,
}

/// The shape curve a soft macro may be resized along.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShapeCurve {
    pub width_intervals: Vec<Interval>,
    pub height_intervals: Vec<Interval>,
}

/// Upstream `SoftMacro::setShapes(TilingList, force)` — the **hard macro cluster** form.
///
/// 🔑 **Every interval is DEGENERATE**: a hard cluster's tilings are exact shapes, so `min` and
/// `max` are the same number and a "random" resize along one of them can only return that shape.
/// The randomness picks WHICH tiling, not a size within it.
///
/// ⚠️ The intervals are deliberately left UNSORTED — upstream says so in a comment. The order is
/// the tiling order, and the index the resize draws is an index into that order.
pub fn shape_curve_from_tilings(tilings: &[(i32, i32)]) -> (ShapeCurve, i32, i32, i64) {
    let mut curve = ShapeCurve::default();
    for &(width, height) in tilings {
        curve.width_intervals.push(Interval { min: width, max: width });
        curve.height_intervals.push(Interval { min: height, max: height });
    }
    let (width, height) = tilings.first().copied().unwrap_or((0, 0));
    let area = width as i64 * height as i64;
    (curve, width, height, area)
}

/// Upstream `SoftMacro::setShapes(IntervalList, area)` — the **mixed cluster** form.
///
/// 🔑 **A piecewise shape curve at constant area.** The widths are merged into disjoint ranges and
/// each gets the height range that keeps the area: a wider macro is a shorter one.
///
/// ⚠️ **The merge is `min > back().max`, so intervals that merely TOUCH are merged.** Two
/// degenerate intervals at the same width collapse to one, which is why a cluster with repeated
/// tiling widths offers fewer choices than it has tilings.
///
/// ⚠️ **The height bounds are integer divisions, and they cross over**: the minimum height comes
/// from the maximum width and the maximum height from the minimum width. Truncation means the
/// recovered area is generally a little under the real one.
///
/// ⛔ Returns `None` when upstream would return leaving the curve EMPTY — an empty interval list
/// or a non-positive area. An empty curve makes `resize_randomly` consume no randomness at all,
/// so the distinction is not cosmetic.
pub fn shape_curve_from_intervals(
    width_intervals: &[Interval],
    area: i64,
) -> Option<(ShapeCurve, i32, i32, i64)> {
    if width_intervals.is_empty() || area <= 0 {
        return None;
    }
    let mut sorted = width_intervals.to_vec();
    // ⚠️ `isMinWidthSmaller` compares the MINIMUM only, and `std::ranges::sort` is not stable.
    // Two intervals sharing a minimum have no defined order upstream either.
    sorted.sort_by_key(|i| i.min);

    let mut merged: Vec<Interval> = Vec::new();
    for interval in sorted {
        match merged.last_mut() {
            Some(back) if interval.min <= back.max => {
                if interval.max > back.max {
                    back.max = interval.max;
                }
            }
            _ => merged.push(interval),
        }
    }

    let heights: Vec<Interval> = merged
        .iter()
        .map(|w| Interval {
            min: (area / w.max as i64) as i32,
            max: (area / w.min as i64) as i32,
        })
        .collect();

    let width = merged[0].min;
    let height = heights[0].max;
    let curve = ShapeCurve { width_intervals: merged, height_intervals: heights };
    Some((curve, width, height, area))
}

/// Upstream `SoftMacro::resizeRandomly`: pick an interval, pick a width inside it, and recover the
/// height from the area.
///
/// ⛔ **An empty curve consumes NO randomness.** Upstream returns before either draw, so a macro
/// with no shapes leaves the generator untouched — drawing anyway would desynchronise every later
/// step of the search.
///
/// ⚠️ **Two draws when it does run**: an integer for the interval, then a float for the position
/// inside it. That count is the load-bearing part; the values only matter afterwards.
///
/// ⚠️ **The area is recomputed from the interval's `min` width and `max` height, NOT from the
/// width just chosen.** So a resize that lands in the middle of a range still uses the area of the
/// range's tallest, narrowest corner, and the height that comes back does not multiply out to the
/// chosen width times anything in particular.
///
/// ⚠️ **`min + draw * (max - min)` is computed in `f32` and TRUNCATED toward zero** on assignment
/// to an `int`. On a degenerate interval `max - min` is zero, so the width is exactly `min`.
pub fn resize_randomly(rng: &mut Mt19937, curve: &ShapeCurve, macro_: &mut SoftMacro) -> i64 {
    if curve.width_intervals.is_empty() {
        return macro_.width as i64 * macro_.height as i64;
    }
    let index = uniform_int(rng, curve.width_intervals.len() as u32) as usize;
    let width_interval = curve.width_intervals[index];
    let draw = canonical_f32(rng);

    let span = (width_interval.max - width_interval.min) as f32;
    macro_.width = (width_interval.min as f32 + draw * span) as i32;

    let area = width_interval.min as i64 * curve.height_intervals[index].max as i64;
    // ⛔ Upstream divides without guarding. A zero width can only arise from a zero-width
    // interval, which `generateTilingsForMacroCluster` never produces.
    macro_.height = if macro_.width != 0 { (area / macro_.width as i64) as i32 } else { 0 };
    area
}

/// Upstream `initSequencePair`: both sequences start as the identity.
///
/// 🔑 **The search therefore starts from a single ROW.** Same order in both sequences means every
/// macro is left of the next, so the first packing is as wide as the sum of the widths — usually
/// far outside the outline, which is what gives the outline penalty something to work against.
///
/// ℹ️ No randomness at all. The shaping caller never supplies an initial pair.
pub fn init_sequence_pair(macro_count: usize) -> SequencePair {
    SequencePair { pos: (0..macro_count).collect(), neg: (0..macro_count).collect() }
}

// ---------------------------------------------------------------- snapping to the curve

/// Upstream `findIntervalIndex`: the interval holding `value`, **snapping `value` into it**.
///
/// ⛔ **It MUTATES its argument.** Upstream takes `int&`, and `setWidth` passes `width_` itself —
/// so the member is snapped inside the call and the area and height that follow are computed from
/// the snapped number, not the requested one. Reading it as a pure lookup loses that.
///
/// ⚠️ A value falling in a GAP between two intervals is pulled to the near edge of the next one:
/// up for widths, down for heights.
pub fn find_interval_index(intervals: &[Interval], value: &mut i32, increasing: bool) -> usize {
    let mut idx = 0;
    if increasing {
        while idx < intervals.len() && intervals[idx].max < *value {
            idx += 1;
        }
        // ⛔ Upstream indexes without re-checking, which is out of bounds when the value is past
        // the last interval. Its callers guarantee the value is in range; clamping here keeps
        // that guarantee from becoming a panic if a caller ever stops honouring it.
        let idx = idx.min(intervals.len() - 1);
        *value = intervals[idx].min.max(*value);
        idx
    } else {
        while idx < intervals.len() && intervals[idx].min > *value {
            idx += 1;
        }
        let idx = idx.min(intervals.len() - 1);
        *value = intervals[idx].max.min(*value);
        idx
    }
}

/// Upstream `SoftMacro::setWidth`: move along the shape curve to (about) this width.
///
/// ⛔ **A hard macro cluster is refused outright**, along with an empty curve and a zero area.
/// Only [`resize_randomly`] reshapes a macro cluster.
///
/// 🔑 **Three cases: below the curve, above it, or inside.** The two ends clamp to a whole
/// interval and take the area from the shape they land on. The interior case is different — it
/// keeps the requested width and takes the area from the interval's OPPOSITE corner, `max` width
/// times `min` height, which is not the corner [`resize_randomly`] uses.
pub fn set_width(macro_: &mut SoftMacro, curve: &ShapeCurve, width: i32) {
    if width <= 0
        || macro_.area == 0
        || curve.width_intervals.len() != curve.height_intervals.len()
        || curve.width_intervals.is_empty()
        || macro_.is_macro_cluster
    {
        return;
    }
    let first_w = curve.width_intervals[0];
    let last_w = curve.width_intervals[curve.width_intervals.len() - 1];
    if width <= first_w.min {
        macro_.width = first_w.min;
        macro_.height = curve.height_intervals[0].max;
        macro_.area = macro_.width as i64 * macro_.height as i64;
    } else if width >= last_w.max {
        macro_.width = last_w.max;
        macro_.height = curve.height_intervals[curve.height_intervals.len() - 1].min;
        macro_.area = macro_.width as i64 * macro_.height as i64;
    } else {
        macro_.width = width;
        let idx = find_interval_index(&curve.width_intervals, &mut macro_.width, true);
        macro_.area = curve.width_intervals[idx].max as i64
            * curve.height_intervals[idx].min as i64;
        macro_.height = (macro_.area / macro_.width as i64) as i32;
    }
}

/// Upstream `SoftMacro::setHeight`, the mirror of [`set_width`].
///
/// ⚠️ **The height intervals run in NON-INCREASING order**, because they were built by inverting
/// the widths — so "the first" is the tallest and the comparisons are reversed.
pub fn set_height(macro_: &mut SoftMacro, curve: &ShapeCurve, height: i32) {
    if height <= 0
        || macro_.area == 0
        || curve.width_intervals.len() != curve.height_intervals.len()
        || curve.width_intervals.is_empty()
        || macro_.is_macro_cluster
    {
        return;
    }
    let first_h = curve.height_intervals[0];
    let last_h = curve.height_intervals[curve.height_intervals.len() - 1];
    if height >= first_h.max {
        macro_.height = first_h.max;
        macro_.width = curve.width_intervals[0].min;
        macro_.area = macro_.width as i64 * macro_.height as i64;
    } else if height <= last_h.min {
        macro_.height = last_h.min;
        macro_.width = curve.width_intervals[curve.width_intervals.len() - 1].max;
        macro_.area = macro_.width as i64 * macro_.height as i64;
    } else {
        macro_.height = height;
        let idx = find_interval_index(&curve.height_intervals, &mut macro_.height, false);
        macro_.area = curve.width_intervals[idx].max as i64
            * curve.height_intervals[idx].min as i64;
        macro_.width = (macro_.area / macro_.height as i64) as i32;
    }
}

// ---------------------------------------------------------------- the resize action

/// Upstream `SACoreSoftMacro::resizeOneCluster`.
///
/// 🔑 **The branch structure IS the randomness budget.** Every path draws a different number of
/// generator words, so getting a branch wrong desynchronises everything after it even when the
/// resulting shape happens to look reasonable:
///
/// | path | words |
/// | --- | --- |
/// | macro cluster | 1 index + a `resize_randomly` |
/// | already outside the outline | 1 index + a `resize_randomly` |
/// | the `< 0.4` roll succeeds | 1 index + 1 roll + a `resize_randomly` |
/// | otherwise | 1 index + 1 roll + 1 option |
///
/// ⚠️ **The `< 0.4` roll is drawn either way** — it is consumed before its own test, so the
/// branch it does not take still pays for it.
///
/// ⚠️ **`>=` against the outline, not `>`.** A macro whose far edge lands exactly on the outline
/// counts as outside and is resized at random.
///
/// ⚠️ The four option branches split at 0.25 / 0.5 / 0.75 with `<=`, and the two GROW branches
/// (wider, taller) are unconditional while the two SHRINK branches only act if they found an edge
/// strictly inside. A grow that finds no neighbour stretches to the outline.
pub fn resize_one_cluster(
    rng: &mut Mt19937,
    macros: &mut [SoftMacro],
    curves: &[ShapeCurve],
    sp: &SequencePair,
    outline_width: i32,
    outline_height: i32,
) -> usize {
    debug_assert!(!sp.pos.is_empty(), "upstream raises MPL-51 on an empty sequence");
    let index = uniform_int(rng, sp.len() as u32) as usize;

    if macros[index].is_macro_cluster {
        resize_randomly(rng, &curves[index], &mut macros[index]);
        return index;
    }

    let (lx, ly, ux, uy) = macros[index].bbox();
    // ⚠️ `>=`: touching the outline counts as outside.
    if ux >= outline_width || uy >= outline_height {
        resize_randomly(rng, &curves[index], &mut macros[index]);
        return index;
    }

    // ⚠️ Drawn unconditionally, then tested.
    if canonical_f32(rng) < 0.4 {
        resize_randomly(rng, &curves[index], &mut macros[index]);
        return index;
    }

    let option = canonical_f32(rng);
    if option <= 0.25 {
        // Widen to the nearest right edge STRICTLY beyond this macro's, else to the outline.
        let mut edge = outline_width;
        for &id in &sp.pos {
            let x2 = macros[id].x + macros[id].width;
            if x2 > ux && x2 < edge {
                edge = x2;
            }
        }
        set_width(&mut macros[index], &curves[index], edge - lx);
    } else if option <= 0.5 {
        // Narrow to the nearest right edge strictly before this macro's.
        let mut edge = lx;
        for &id in &sp.pos {
            let x2 = macros[id].x + macros[id].width;
            if x2 < ux && x2 > edge {
                edge = x2;
            }
        }
        // ⚠️ Guarded, unlike the widen branch: with no neighbour the macro is left alone rather
        // than collapsed to zero width.
        if edge > lx {
            set_width(&mut macros[index], &curves[index], edge - lx);
        }
    } else if option <= 0.75 {
        let mut edge = outline_height;
        for &id in &sp.pos {
            let y2 = macros[id].y + macros[id].height;
            if y2 > uy && y2 < edge {
                edge = y2;
            }
        }
        set_height(&mut macros[index], &curves[index], edge - ly);
    } else {
        let mut edge = ly;
        for &id in &sp.pos {
            let y2 = macros[id].y + macros[id].height;
            if y2 < uy && y2 > edge {
                edge = y2;
            }
        }
        if edge > ly {
            set_height(&mut macros[index], &curves[index], edge - ly);
        }
    }
    index
}

// ---------------------------------------------------------------- the search state

/// The annealer's working state for one tiling run.
///
/// 🔑 **Mirrors the members of upstream's core rather than a tidier arrangement**, because the
/// save/restore pair below is defined in terms of exactly which members it copies — and it does
/// not copy all of them.
#[derive(Debug, Clone)]
pub struct Search {
    pub macros: Vec<SoftMacro>,
    pub curves: Vec<ShapeCurve>,
    pub sp: SequencePair,
    pub width: i32,
    pub height: i32,
    pub outline_penalty: f32,
    /// ⛔ **Deliberately outside the saved set** — see [`Search::restore_state`].
    pub fixed_macros_penalty: f32,
    pub outline_width: i32,
    pub outline_height: i32,
    pub dbu_per_micron: i32,
    /// The bounding boxes of the fixed macros, taken once before the search starts.
    pub fixed_bboxes: Vec<(i32, i32, i32, i32)>,
    pub weights: ShapingWeights,
    pub normalization: Normalization,
    pub probabilities: ActionProbabilities,
    /// The action the last `perturb` chose. ⚠️ Restoring reads this, so it must survive the call.
    pub action: Option<Action>,
}

/// What `saveState` copies.
///
/// ⛔ **`fixed_macros_penalty` is NOT here, and that is upstream's own omission**: `saveState`
/// lists seven penalties and leaves that one out, and `restoreState` leaves it out to match. A
/// restore therefore keeps the *rejected* state's fixed-macro penalty until the next `perturb`
/// recomputes it. Adding it to the saved set would be more correct and would not reproduce the
/// reference.
#[derive(Debug, Clone)]
pub struct Saved {
    macros: Vec<SoftMacro>,
    pos: Vec<usize>,
    neg: Vec<usize>,
    width: i32,
    height: i32,
    outline_penalty: f32,
}

impl Search {
    pub fn outline_area(&self) -> i64 {
        self.outline_width as i64 * self.outline_height as i64
    }

    /// Upstream `getAreaPenalty`, from the current packing.
    pub fn area_penalty(&self) -> f32 {
        area_penalty(self.width, self.height, self.outline_area(), self.dbu_per_micron)
    }

    /// Upstream `calPenalty`, reduced to the terms shaping leaves alive.
    ///
    /// ℹ️ Boundary, soft-blockage and notch all early-return on a zero weight without touching
    /// anything else, and wirelength does the same — so for a tiling run they are permanently
    /// zero and are not modelled.
    pub fn cal_penalty(&mut self) {
        self.outline_penalty =
            outline_penalty(self.width, self.height, self.outline_width, self.outline_height);
        self.fixed_macros_penalty = fixed_macros_penalty(
            &self.macros,
            &self.fixed_bboxes,
            &self.sp,
            self.dbu_per_micron,
        );
    }

    /// Upstream `SACoreSoftMacro::calNormCost`.
    pub fn norm_cost(&self) -> f32 {
        norm_cost(
            &Penalties {
                area: self.area_penalty(),
                outline: self.outline_penalty,
                fixed_macros: self.fixed_macros_penalty,
            },
            &self.weights,
            &self.normalization,
        )
    }

    /// Upstream `resultFitsInOutline`.
    pub fn fits_in_outline(&self) -> bool {
        self.width <= self.outline_width && self.height <= self.outline_height
    }

    /// Upstream `saveState`. ⚠️ A run with no macros saves nothing at all.
    pub fn save_state(&self) -> Option<Saved> {
        if self.macros.is_empty() {
            return None;
        }
        Some(Saved {
            macros: self.macros.clone(),
            pos: self.sp.pos.clone(),
            neg: self.sp.neg.clone(),
            width: self.width,
            height: self.height,
            outline_penalty: self.outline_penalty,
        })
    }

    /// Upstream `restoreState`.
    ///
    /// ⛔ **Which sequences come back depends on the ACTION that was taken.** A positive-sequence
    /// swap restores only the positive sequence, a negative-sequence swap only the negative, a
    /// double swap and an exchange both, and a **resize restores neither** — it never touched
    /// them. Restoring both unconditionally would be harmless for correctness and would still be
    /// a different program.
    ///
    /// ⚠️ **The packing is NOT recomputed.** Upstream says so at the site: the width and height
    /// are copied back instead, and a final `packFloorplan` at the end of the search puts the
    /// macros where the restored sequences say they belong.
    pub fn restore_state(&mut self, saved: &Saved) {
        if self.macros.is_empty() {
            return;
        }
        match self.action {
            Some(Action::SwapPositive) => self.sp.pos.clone_from(&saved.pos),
            Some(Action::SwapNegative) => self.sp.neg.clone_from(&saved.neg),
            Some(Action::SwapBoth) | Some(Action::Exchange) => {
                self.sp.pos.clone_from(&saved.pos);
                self.sp.neg.clone_from(&saved.neg);
            }
            // ⚠️ Resize, or no action yet: the sequences are left as they are.
            _ => {}
        }
        self.macros.clone_from(&saved.macros);
        self.width = saved.width;
        self.height = saved.height;
        self.outline_penalty = saved.outline_penalty;
    }

    /// Upstream `SACoreSoftMacro::perturb`: choose an action, take it, repack, rescore.
    ///
    /// ⛔ **An empty macro list returns before the draw**, leaving the generator untouched.
    pub fn perturb(&mut self, rng: &mut Mt19937) {
        let Some(action) = choose_action(rng, &self.probabilities, self.macros.len()) else {
            return;
        };
        self.action = Some(action);
        match action {
            Action::SwapPositive => single_seq_swap(rng, &mut self.sp, true),
            Action::SwapNegative => single_seq_swap(rng, &mut self.sp, false),
            Action::SwapBoth => double_seq_swap(rng, &mut self.sp),
            Action::Exchange => exchange_macros(rng, &mut self.sp),
            Action::Resize => {
                resize_one_cluster(
                    rng,
                    &mut self.macros,
                    &self.curves,
                    &self.sp,
                    self.outline_width,
                    self.outline_height,
                );
            }
        }
        let (width, height) = pack_floorplan(&mut self.macros, &self.sp);
        self.width = width;
        self.height = height;
        self.cal_penalty();
    }
}
