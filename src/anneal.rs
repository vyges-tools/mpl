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
