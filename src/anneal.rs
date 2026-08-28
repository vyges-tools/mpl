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
impl SoftMacro {
    /// Upstream `SoftMacro::setX`.
    ///
    /// ⛔ **THE GUARD LIVES IN THE SETTER, not at the call sites.** `moveFloorplan`,
    /// `setClustersLocations` and `packFloorplan` all assign positions with no `isFixed` test of
    /// their own — they do not need one, because `setX`/`setY` refuse silently for a fixed macro.
    ///
    /// ⚠️ **Reading a call site and concluding "there is no fixed test" is how this was got wrong
    /// four separate times** in this engine's notes and comments. When the reference assigns
    /// through a setter, read the setter.
    pub fn set_x(&mut self, x: i32) {
        if !self.fixed {
            self.x = x;
        }
    }

    /// Upstream `SoftMacro::setY`. See [`SoftMacro::set_x`] — the guard is here, not at the callers.
    pub fn set_y(&mut self, y: i32) {
        if !self.fixed {
            self.y = y;
        }
    }
}

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
        macros[id].set_x(accumulated[neg_pos]);
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
        macros[id].set_y(accumulated[neg_pos]);
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

    /// Upstream's five action shares as `HierRTLMP` declares them, normalised.
    ///
    /// ⛔ **`resize_prob_` is `0.4`, not `0.2`** — the four swaps are `0.2` each and resize is
    /// double. The five sum to `1.2`, so after normalisation a swap is `0.1667` and a resize
    /// `0.3333`. Passing five equal shares is a plausible-looking mistake that changes every
    /// random walk: the final placement of a small design still converges, so the VALUES match
    /// and only the normalisation factors — averages over the walk — reveal it.
    ///
    /// ⚠️ **Coarse shaping does NOT share this constructor.** It zeroes the resize share on a
    /// design with no standard cells, which changes the divisor to `0.8` and therefore all four
    /// swap probabilities too — see `ShapingCtx::probabilities`. Cluster placement never zeroes it.
    pub fn placement_defaults() -> Self {
        Self::normalized(0.2, 0.2, 0.2, 0.2, 0.4)
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

/// Upstream's `float += double`, which is NOT the same as narrowing the addend first.
///
/// ⛔ **C++ promotes the accumulator, adds in `double`, and rounds ONCE on the assignment.**
/// Writing `acc += addend as f32` rounds twice — the addend to `f32`, then the sum — and double
/// rounding gives a different answer for some inputs. It is a one-ulp difference that fires on
/// specific bit patterns rather than on large or small values, so it is invisible in a fixture and
/// perfectly capable of changing an annealing trajectory.
///
/// 🔑 Every penalty that accumulates a `dbuToMicrons` or `dbuAreaToMicrons` result goes through
/// here, because every one of them is a `float` member taking a `double` addend.
pub fn plus_double(accumulator: f32, addend: f64) -> f32 {
    (accumulator as f64 + addend) as f32
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
            // ⛔ `float += double`: the sum is formed in `f64` and rounded once. See
            // [`plus_double`] — narrowing the addend first is a different number.
            penalty = plus_double(penalty, area_to_microns(dx as i64 * dy as i64, dbu_per_micron));
        }
    }
    penalty
}

/// The nine weights a soft-macro annealer is built with.
///
/// 🔑 **Shaping is the case where six of them are ZERO.** The tiling search passes area `1.0` and
/// outline `1000.0`, zeroes wirelength, guidance and fence, and passes a default `SASoftWeights`
/// — so boundary, notch and soft blockage vanish too. What is left is area, outline, and the
/// fixed-macros term whose weight is a class constant. Cluster placement lights all nine, and
/// [`Default`] is the shaping set because that is the caller this file was built for.
///
/// ⚠️ **`fence` is NOT dead by default at placement.** Its command default is `10.0`, and only a
/// design with no standard cells zeroes it — so the term is live on any design that declares a
/// fence, and merely has nothing to score on one that does not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoftWeights {
    pub area: f32,
    pub outline: f32,
    pub wirelength: f32,
    pub guidance: f32,
    pub fence: f32,
    pub boundary: f32,
    pub soft_blockage: f32,
    /// ⚠️ Not a command option — a class constant.
    pub fixed_macros: f32,
    pub notch: f32,
}

impl Default for SoftWeights {
    fn default() -> Self {
        Self {
            area: 1.0,
            outline: 1000.0,
            wirelength: 0.0,
            guidance: 0.0,
            fence: 0.0,
            boundary: 0.0,
            soft_blockage: 0.0,
            fixed_macros: 100.0,
            notch: 0.0,
        }
    }
}

impl SoftWeights {
    /// The weights `placeChildren` builds its annealers with — `mpl.tcl`'s own defaults, with the
    /// soft-blockage weight already adjusted for the tree's depth by the caller.
    ///
    /// ⚠️ **`soft_blockage` is `10.0` from the command**, not the `50.0` a single-level tree ends
    /// up with; that raise is `adjustSoftBlockageWeight`'s, and it happens once, before any
    /// annealer is built.
    pub fn placement_defaults() -> Self {
        Self {
            area: 0.1,
            outline: 100.0,
            wirelength: 100.0,
            guidance: 10.0,
            fence: 10.0,
            boundary: 50.0,
            soft_blockage: 10.0,
            fixed_macros: 100.0,
            notch: 50.0,
        }
    }
}

/// The normalisation factors `initialize` measures, after its `<= 1e-4` floor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Normalization {
    pub area: f32,
    pub outline: f32,
    pub wirelength: f32,
    pub guidance: f32,
    pub fence: f32,
    pub boundary: f32,
    pub soft_blockage: f32,
    pub fixed_macros: f32,
    pub notch: f32,
}

impl Default for Normalization {
    fn default() -> Self {
        Self {
            area: 1.0,
            outline: 1.0,
            wirelength: 1.0,
            guidance: 1.0,
            fence: 1.0,
            boundary: 1.0,
            soft_blockage: 1.0,
            fixed_macros: 1.0,
            notch: 1.0,
        }
    }
}

/// One packing's penalties, as `calPenalty` leaves them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Penalties {
    pub area: f32,
    pub outline: f32,
    pub wirelength: f32,
    pub guidance: f32,
    /// ℹ️ Computed by [`crate::placement::fence_penalty`]. It is zero for every design in the
    /// suite because none of them declares a fence — not because the term is unbuilt.
    pub fence: f32,
    pub boundary: f32,
    pub soft_blockage: f32,
    pub fixed_macros: f32,
    pub notch: f32,
}

/// Upstream `SACoreSoftMacro::calNormCost`.
///
/// ⚠️ **Each term is gated on its normalisation factor being `> 0`**, and `initialize` floors any
/// factor at or below `1e-4` to `1.0` — so in practice every gate is open. The gate is reproduced
/// because a factor of exactly zero would otherwise divide by zero rather than drop the term.
///
/// ⚠️ **The addition order is the source's**: area, outline, wirelength, guidance, fence,
/// boundary, soft blockage, fixed macros, notch. Floating-point addition is not associative and
/// the accept/reject test compares these values directly, so reordering is a different search.
///
/// ℹ️ Shaping's six dark terms each add an exact `0.0`, which leaves the running sum bit for bit
/// as it was — so a shaping cost is the same number it was before the other six existed.
pub fn norm_cost(p: &Penalties, w: &SoftWeights, n: &Normalization) -> f32 {
    let mut cost = 0.0f32;
    if n.area > 0.0 {
        // ⚠️ No division here — see `area_penalty`.
        cost += w.area * p.area;
    }
    if n.outline > 0.0 {
        cost += w.outline * p.outline / n.outline;
    }
    if n.wirelength > 0.0 {
        cost += w.wirelength * p.wirelength / n.wirelength;
    }
    if n.guidance > 0.0 {
        cost += w.guidance * p.guidance / n.guidance;
    }
    if n.fence > 0.0 {
        cost += w.fence * p.fence / n.fence;
    }
    if n.boundary > 0.0 {
        cost += w.boundary * p.boundary / n.boundary;
    }
    if n.soft_blockage > 0.0 {
        cost += w.soft_blockage * p.soft_blockage / n.soft_blockage;
    }
    if n.fixed_macros > 0.0 {
        cost += w.fixed_macros * p.fixed_macros / n.fixed_macros;
    }
    if n.notch > 0.0 {
        cost += w.notch * p.notch / n.notch;
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
/// width just chosen.** ℹ️ That is not the anomaly it first looks like: the height bounds were
/// built as `A / width`, so `w.min * h.max` reconstructs the cluster's area `A` — exactly when
/// the division was exact, and short by less than `w.min` when it truncated. `set_width`'s
/// interior branch uses the OTHER corner, `w.max * h.min`, and recovers the same `A` the same
/// way. The two are equivalent up to truncation, not two different areas.
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

/// Upstream `initSequencePair` in full, including the branch macro placement uses.
///
/// ⛔ **An INITIAL sequence pair suppresses the default entirely.** `setInitialSequencePair` sets a
/// flag, and `initSequencePair` then returns before building anything — so a macro array keeps the
/// grid arrangement `computeArraySequencePair` produced, rather than the identity ordering.
///
/// ⚠️ **The count is `number_of_sequence_pair_macros` when it is non-zero, and the macro count
/// otherwise** — the two differ wherever fixed terminals or IO clusters were appended after the
/// placeable macros.
pub fn init_sequence_pair_with(
    macro_count: usize,
    number_of_sequence_pair_macros: usize,
    initial: Option<SequencePair>,
) -> SequencePair {
    if let Some(sp) = initial {
        return sp;
    }
    let size = if number_of_sequence_pair_macros != 0 {
        number_of_sequence_pair_macros
    } else {
        macro_count
    };
    init_sequence_pair(size)
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
    /// Every penalty the core keeps as a member.
    ///
    /// ⚠️ **`area` is NOT one of them upstream** — it is derived from the packing on every read
    /// by `getAreaPenalty()`. The field here is never written by [`Search::cal_penalty`] and never
    /// read by [`Search::norm_cost`], which derives it too; it exists only so one struct carries
    /// the whole cost vector.
    ///
    /// ⛔ **`fixed_macros` is deliberately outside the saved set** — see [`Search::restore_state`].
    pub penalties: Penalties,
    pub outline_width: i32,
    pub outline_height: i32,
    pub dbu_per_micron: i32,
    /// The bounding boxes of the fixed macros, taken once before the search starts.
    pub fixed_bboxes: Vec<(i32, i32, i32, i32)>,
    /// What the six placement-only terms need in order to be scored.
    ///
    /// ⚠️ **`None` for a tiling run**, which is why those six stay at zero there — see
    /// [`Search::cal_penalty`].
    pub placement: Option<Box<crate::placement::PlacementInputs>>,
    pub weights: SoftWeights,
    pub normalization: Normalization,
    pub probabilities: ActionProbabilities,
    /// The action the last `perturb` chose. ⚠️ Restoring reads this, so it must survive the call.
    pub action: Option<Action>,
    /// Set for a HARD-macro run, and the presence of it IS the mode switch.
    ///
    /// ⛔ **`SACoreHardMacro` is a different core, not the soft one with terms zeroed.** It has
    /// FOUR actions and no resize, it computes only outline, wirelength, guidance and fence — not
    /// even the fixed-macro penalty — and its cost is [`crate::placement::hard_norm_cost`].
    /// Boundary, notch, soft blockage and fixed macros are `SACoreSoftMacro`'s own members and do
    /// not exist here at all.
    pub hard_probabilities: Option<crate::placement::HardActionProbabilities>,
    /// `(temperature, pre_cost)` once per STEP, as `writeCostFile` emits them.
    ///
    /// 🔑 **The trajectory, not the endpoint.** Two runs can finish at the same cost by different
    /// routes, and the floorplan only shows where a walk ended; this shows where it went. It is
    /// the instrument for any divergence that survives once the inputs match.
    ///
    /// ⚠️ **The temperature recorded is the NEXT step's** — upstream decays it and *then* pushes,
    /// so entry `i` pairs the cost of step `i` with the temperature step `i+1` will use.
    pub cost_history: Vec<(f32, f32)>,
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
    /// ⚠️ Copied whole, but only the SEVEN upstream lists are put back — see
    /// [`Search::restore_state`].
    penalties: Penalties,
}

impl Search {
    pub fn outline_area(&self) -> i64 {
        self.outline_width as i64 * self.outline_height as i64
    }

    /// Upstream `getAreaPenalty`, from the current packing.
    pub fn area_penalty(&self) -> f32 {
        area_penalty(self.width, self.height, self.outline_area(), self.dbu_per_micron)
    }

    /// Upstream `calPenalty`.
    ///
    /// ⚠️ **The term ORDER is load-bearing, and not because of arithmetic.** `calNotchPenalty`
    /// asks `isValid()`, which reads the fixed-macro penalty — and `calFixedMacrosPenalty` runs
    /// AFTER it. So the notch term judges validity against the PREVIOUS perturbation's
    /// fixed-macro penalty, not this one's. Computing the fixed-macro term first would be the
    /// obvious tidy-up and would be a different program.
    ///
    /// ℹ️ Without a placement context the six extra terms are not computed at all. Upstream calls
    /// each of them and each early-returns on its zero weight, leaving the member as it was —
    /// which for a tiling run is the `0.0` it was constructed with. Skipping them is the same
    /// arithmetic, and it is why the shaping path costs nothing extra.
    pub fn cal_penalty(&mut self) {
        self.penalties.outline =
            outline_penalty(self.width, self.height, self.outline_width, self.outline_height);

        // ⛔ `SACoreHardMacro::calPenalty` calls FOUR functions: outline, wirelength, guidance,
        // fence. It does NOT call the fixed-macro penalty, and the four soft-only terms are not
        // its members at all — so they stay at the zero they were built with.
        if self.hard_probabilities.is_some() {
            if let Some(inputs) = self.placement.take() {
                let outline = (self.outline_width, self.outline_height);
                self.penalties.wirelength = inputs.wirelength(&self.macros, outline);
                self.penalties.guidance = inputs.guidance(&self.macros, self.dbu_per_micron);
                self.penalties.fence = inputs.fence(&self.macros, outline);
                self.placement = Some(inputs);
            }
            return;
        }

        if let Some(inputs) = self.placement.take() {
            let outline = (self.outline_width, self.outline_height);

            self.penalties.wirelength = inputs.wirelength(&self.macros, outline);
            self.penalties.guidance = inputs.guidance(&self.macros, self.dbu_per_micron);
            self.penalties.fence = inputs.fence(&self.macros, outline);
            self.penalties.boundary = inputs.boundary(&self.macros, &self.sp, self.dbu_per_micron);
            self.penalties.soft_blockage = inputs.soft_blockage(&self.macros, &self.sp);
            // ⛔ Reads the fixed-macro penalty that is about to be overwritten. See above.
            let valid = self.is_valid(!self.fixed_bboxes.is_empty());
            self.penalties.notch =
                inputs.notch(&self.macros, outline, (self.width, self.height), valid);

            self.placement = Some(inputs);
        }

        self.penalties.fixed_macros = fixed_macros_penalty(
            &self.macros,
            &self.fixed_bboxes,
            &self.sp,
            self.dbu_per_micron,
        );
    }

    /// Upstream `SACoreSoftMacro::calNormCost`.
    pub fn norm_cost(&self) -> f32 {
        let p = Penalties { area: self.area_penalty(), ..self.penalties };
        if self.hard_probabilities.is_some() {
            return crate::placement::hard_norm_cost(&p, &self.weights, &self.normalization);
        }
        norm_cost(&p, &self.weights, &self.normalization)
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
            penalties: self.penalties,
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
        // ⛔ The SEVEN upstream restores, named one by one. `area` is not a member there and
        // `fixed_macros` is upstream's own omission — putting either back would be a different
        // program.
        self.penalties.outline = saved.penalties.outline;
        self.penalties.wirelength = saved.penalties.wirelength;
        self.penalties.guidance = saved.penalties.guidance;
        self.penalties.fence = saved.penalties.fence;
        self.penalties.boundary = saved.penalties.boundary;
        self.penalties.soft_blockage = saved.penalties.soft_blockage;
        self.penalties.notch = saved.penalties.notch;
    }

    /// Upstream `SACoreSoftMacro::perturb`: choose an action, take it, repack, rescore.
    ///
    /// ⛔ **An empty macro list returns before the draw**, leaving the generator untouched.
    pub fn perturb(&mut self, rng: &mut Mt19937) {
        // ⛔ A hard-macro run draws the SAME single word and dispatches on FOUR actions — there is
        // no resize, because a hard macro has one shape. Exchange is the `else`, so it absorbs
        // whatever slack the normalisation left, exactly as the soft core's resize does.
        let Some(action) = (match &self.hard_probabilities {
            Some(hard) if !self.macros.is_empty() => Some(hard.action_for(canonical_f32(rng))),
            Some(_) => None,
            None => choose_action(rng, &self.probabilities, self.macros.len()),
        }) else {
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

// ---------------------------------------------------------------- the best result

/// The best packing the search has seen.
///
/// ⚠️ **Only each macro's WIDTH is kept**, never its height. Restoring re-derives the height —
/// from the stored area for a macro cluster, and by walking the shape curve for anything else.
#[derive(Debug, Clone, Default)]
pub struct BestResult {
    pub cost: f32,
    pub pos: Vec<usize>,
    pub neg: Vec<usize>,
    pub macro_widths: Vec<i32>,
}

impl BestResult {
    /// ⚠️ The cost starts at the largest finite float, so the first candidate always wins.
    pub fn new() -> Self {
        Self { cost: f32::MAX, pos: Vec::new(), neg: Vec::new(), macro_widths: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }
}

impl Search {
    /// Upstream `SACoreSoftMacro::isValid`.
    ///
    /// ⚠️ Two conditions: nothing may overlap a fixed macro, AND the packing must fit. A design
    /// with no fixed macros is judged on the fit alone.
    pub fn is_valid(&self, fixed_present: bool) -> bool {
        if fixed_present && self.penalties.fixed_macros > 0.0 {
            return false;
        }
        self.fits_in_outline()
    }

    /// Upstream `updateBestResult`.
    pub fn update_best_result(&self, best: &mut BestResult, cost: f32) {
        best.pos.clone_from(&self.sp.pos);
        best.neg.clone_from(&self.sp.neg);
        best.macro_widths = vec![0; self.macros.len()];
        for &id in &self.sp.pos {
            best.macro_widths[id] = self.macros[id].width;
        }
        best.cost = cost;
    }

    /// Upstream `useBestResult`: put the sequences back and rebuild each macro from its width.
    ///
    /// ⚠️ **A macro cluster is restored with `setShapeF`, which BYPASSES the shape curve** — the
    /// height is `area / width` and both are assigned directly. Everything else goes through
    /// `set_width` and is snapped to the curve, so the two paths can disagree about what a given
    /// width means.
    pub fn use_best_result(&mut self, best: &BestResult) {
        self.sp.pos.clone_from(&best.pos);
        self.sp.neg.clone_from(&best.neg);
        for &id in &self.sp.pos {
            let width = best.macro_widths[id];
            if self.macros[id].is_macro_cluster {
                // ⚠️ `getArea` reports zero for an area of 1 or less.
                let area = if self.macros[id].area > 1 { self.macros[id].area } else { 0 };
                let height = if width != 0 { (area / width as i64) as i32 } else { 0 };
                if !self.macros[id].fixed {
                    self.macros[id].width = width;
                    self.macros[id].height = height;
                    self.macros[id].area = width as i64 * height as i64;
                }
            } else {
                let curve = self.curves[id].clone();
                set_width(&mut self.macros[id], &curve, width);
            }
        }
        let (width, height) = pack_floorplan(&mut self.macros, &self.sp);
        self.width = width;
        self.height = height;
        self.cal_penalty();
    }
}

// ---------------------------------------------------------------- initialize

/// The hyperparameters the tiling search is built with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SaParameters {
    pub init_prob: f32,
    pub max_num_step: i32,
    pub num_perturb_per_step: i32,
    /// ⛔ **`false` ONLY for a macro array with empty space** — `disallowInvalidStates` is called
    /// from exactly one place in the whole engine. Coarse shaping and cluster placement both leave
    /// it `true`, which is why the restore branch it guards was unmodelled until macro placement.
    pub invalid_states_allowed: bool,
}

impl Default for SaParameters {
    fn default() -> Self {
        Self {
            init_prob: 0.9,
            max_num_step: 2000,
            num_perturb_per_step: 500,
            invalid_states_allowed: true,
        }
    }
}

impl SaParameters {
    /// Upstream's own adjustment at the COARSE SHAPING call site — and at that site only.
    ///
    /// ⚠️ **A TENTH of the configured count is the floor**, and the macro count wins only when it
    /// exceeds that — so every cluster with fewer than 50 macros runs exactly 50 perturbations
    /// per step, not one per macro.
    ///
    /// ⛔ **Cluster placement does NOT use this rule.** It takes `max(macros, num_perturb_per_step)`
    /// — the full configured count, ten times this floor — and macro placement has a third rule
    /// again. The adjustment belongs to the caller; `Search::initialize` uses whatever count it is
    /// handed.
    pub fn perturbations_for(&self, macro_count: usize) -> usize {
        let tenth = (self.num_perturb_per_step / 10) as usize;
        if macro_count > tenth {
            macro_count
        } else {
            tenth
        }
    }
}

impl Search {
    /// Upstream `SACoreSoftMacro::initialize`: measure the penalties, then set the temperature.
    ///
    /// 🔑 **The sweep never restores.** It saves state each iteration and only puts it back when
    /// invalid states are disallowed — which the shaping caller never asks for. So the samples
    /// walk a RANDOM TRAJECTORY: the twentieth is twenty moves from the start, not one.
    ///
    /// ⚠️ **Every factor at or below `1e-4` becomes exactly `1.0`.** That is not a clamp to a
    /// small number; a penalty that is almost always zero ends up dividing by one, so its raw
    /// magnitude reaches the cost undamped on the rare step where it is not zero.
    ///
    /// ⚠️ **The replay MUTATES the live state.** Upstream assigns each sample back into the
    /// members to recompute its cost, so when this returns the width, height and penalties are
    /// the LAST sample's — which is also the last perturbation's, since nothing was restored.
    ///
    /// ⚠️ The initial temperature comes from the mean ABSOLUTE step-to-step change in cost, not
    /// from the spread of the costs: `-(mean delta) / ln(init_prob)`.
    pub fn initialize(&mut self, rng: &mut Mt19937, params: &SaParameters) -> f32 {
        // ⛔ The count is the CALLER's, not the core's own derivation. Upstream computes it at
        // each call site and passes it in, and the three sites do NOT agree: coarse shaping takes
        // `max(macros, num/10)`, cluster placement takes `max(macros, num)` — ten times larger —
        // and macro placement has a third rule. Re-deriving the shaping formula in here silently
        // gave every other caller the shaping count.
        let perturbations = params.num_perturb_per_step.max(0) as usize;

        let mut samples: Vec<Sample> = Vec::with_capacity(perturbations);

        for _ in 0..perturbations {
            let saved = self.save_state();
            self.perturb(rng);
            // ⛔ Same branch as `fast_sa`'s: an invalid state is restored and SKIPPED, so it is
            // never recorded — which makes the sample lists SHORTER than the perturbation count,
            // and the normalisation factors an average over fewer samples.
            if !params.invalid_states_allowed && !self.is_valid(!self.fixed_bboxes.is_empty()) {
                if let Some(saved) = &saved {
                    self.restore_state(saved);
                }
                continue;
            }
            samples.push(Sample {
                width: self.width,
                height: self.height,
                area: self.area_penalty(),
                penalties: self.penalties,
            });
        }

        // ⚠️ **All NINE terms, not the three coarse shaping moves.** Without a placement context
        // `cal_penalty` writes only `outline` and `fixed_macros`, so for shaping the other six
        // average zero and the floor lifts them to `1.0` — exactly what shaping got when only
        // three were sampled. Sampling all nine is therefore inert there and is the only way the
        // placement path gets real factors.
        let floor_at = |value: f32| if value <= 1e-4 { 1.0 } else { value };
        let mean = |f: &dyn Fn(&Sample) -> f32| -> f32 {
            floor_at(average(&samples.iter().map(f).collect::<Vec<f32>>()))
        };
        self.normalization = Normalization {
            area: mean(&|s| s.area),
            outline: mean(&|s| s.penalties.outline),
            wirelength: mean(&|s| s.penalties.wirelength),
            guidance: mean(&|s| s.penalties.guidance),
            fence: mean(&|s| s.penalties.fence),
            boundary: mean(&|s| s.penalties.boundary),
            soft_blockage: mean(&|s| s.penalties.soft_blockage),
            notch: mean(&|s| s.penalties.notch),
            fixed_macros: mean(&|s| s.penalties.fixed_macros),
        };

        let mut cost_list = Vec::with_capacity(samples.len());
        for s in &samples {
            self.width = s.width;
            self.height = s.height;
            // ⚠️ Upstream assigns EIGHT members here; `area` is not one of them, because it is not
            // a member at all — `norm_cost` derives it from the width and height just restored.
            // ℹ️ Assigning the whole struct is the same program: `Penalties::area` is never
            // written by `cal_penalty` and never read by `norm_cost`, so the sample carries the
            // same untouched zero the live state holds. Spelling out a partial update here reads
            // as a distinction and is not one.
            self.penalties = s.penalties;
            cost_list.push(self.norm_cost());
        }

        let mut delta_cost = 0.0f32;
        for i in 1..cost_list.len() {
            delta_cost += (cost_list[i] - cost_list[i - 1]).abs();
        }

        if cost_list.len() > 1 && delta_cost > 0.0 {
            -(delta_cost / (cost_list.len() - 1) as f32) / params.init_prob.ln()
        } else {
            1.0
        }
    }

    /// Upstream `fastSA`.
    ///
    /// 🔑 **The temperature decays GEOMETRICALLY to `1e-10`** over exactly `max_num_step` steps,
    /// the ratio fixed in advance rather than adapted.
    ///
    /// ⚠️ **A random word is drawn ONLY when a move makes things worse.** An improving move is
    /// accepted without consulting the generator, so the number of words a run consumes depends
    /// on its own trajectory — which is why every earlier piece had to be exact.
    ///
    /// ⚠️ **`num < prob`, strictly.** The draw is in `[0, 1)` and `prob` can reach 1 when the
    /// temperature is still high, so the boundary is reachable.
    ///
    /// ⚠️ The final packing is recomputed once at the end, because `restore_state` deliberately
    /// left the macros where the rejected move had put them.
    pub fn fast_sa(
        &mut self,
        rng: &mut Mt19937,
        params: &SaParameters,
        init_temperature: f32,
        fixed_present: bool,
    ) -> BestResult {
        // ⛔ The caller's count, for the same reason as `initialize` — upstream hands ONE
        // adjusted number to the core and both loops read that member.
        let perturbations = params.num_perturb_per_step.max(0) as usize;
        let mut best = BestResult::new();
        let mut is_best_valid = false;

        let mut cost = self.norm_cost();
        let mut pre_cost = cost;
        let mut temperature = init_temperature;
        const MIN_T: f32 = 1e-10;
        let t_factor = ((MIN_T / init_temperature).ln() / params.max_num_step as f32).exp();

        self.update_best_result(&mut best, cost);

        let mut step = 1;
        while step <= params.max_num_step {
            for _ in 0..perturbations {
                let saved = self.save_state();
                self.perturb(rng);
                cost = self.norm_cost();

                let is_valid = self.is_valid(fixed_present);

                // ⛔ **Restored AND SKIPPED.** An invalid state is not scored, does not update the
                // best result, and — the part that matters most — never reaches the acceptance
                // test, so it consumes NO random word. Disallowing invalid states therefore changes
                // the generator's trajectory, not just which states survive.
                //
                // ⚠️ `pre_cost` is left alone, so the next comparison is still against the last
                // ACCEPTED cost rather than this rejected one.
                if !params.invalid_states_allowed && !is_valid {
                    if let Some(saved) = &saved {
                        self.restore_state(saved);
                    }
                    continue;
                }

                let found_new_best = cost < best.cost;
                if (!is_best_valid || is_valid) && found_new_best {
                    self.update_best_result(&mut best, cost);
                    is_best_valid = is_valid;
                }

                let delta_cost = cost - pre_cost;
                if delta_cost <= 0.0 {
                    pre_cost = cost;
                } else {
                    let num = canonical_f32(rng);
                    let prob = (-delta_cost / temperature).exp();
                    if num < prob {
                        pre_cost = cost;
                    } else if let Some(saved) = &saved {
                        self.restore_state(saved);
                    }
                }
            }
            temperature *= t_factor;
            step += 1;
            // ⚠️ AFTER the decay, matching upstream's own order — see `cost_history`.
            self.cost_history.push((temperature, pre_cost));
        }

        let (width, height) = pack_floorplan(&mut self.macros, &self.sp);
        self.width = width;
        self.height = height;
        self.cal_penalty();
        cost = self.norm_cost();

        let found_new_best = cost < best.cost;
        if (is_best_valid && !self.is_valid(fixed_present)) || !found_new_best {
            self.use_best_result(&best);
        }
        best
    }
}

// ---------------------------------------------------------------- the driver

/// How the tiling search is driven.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TilingSearch {
    /// How many outlines to try per axis.
    pub num_runs: i32,
    /// ⚠️ **Every run uses the SAME seed.** The runs differ only in their outline, never in their
    /// randomness — so two runs given the same outline would produce identical answers.
    pub random_seed: u32,
    /// The aspect-ratio band a tiling must fall in to survive the final filter.
    ///
    /// ⛔ **`0.33`, from the COMMAND, not the `0.3` the C++ member is initialised with.** The Tcl
    /// layer passes its own default down on every run, so the member's value is never the one in
    /// effect — and the difference is visible in the reference's own trace, which prints
    /// `min_ar: 0.33`.
    pub min_ar: f32,
    pub sa: SaParameters,
}

impl Default for TilingSearch {
    fn default() -> Self {
        Self { num_runs: 10, random_seed: 0, min_ar: 0.33, sa: SaParameters::default() }
    }
}

impl TilingSearch {
    /// Upstream's `vary_factor_list`: `1.0`, then `1 - i/num_runs` down to `1/num_runs`.
    ///
    /// ⚠️ **The first entry is the FULL outline**, and the rest shrink. The list is exactly
    /// `num_runs` long and never reaches zero.
    pub fn vary_factors(&self) -> Vec<f32> {
        let step = 1.0 / self.num_runs as f32;
        let mut factors = vec![1.0f32];
        for i in 1..self.num_runs {
            factors.push(1.0 - i as f32 * step);
        }
        factors
    }
}

/// No tiling survived — upstream raises MPL-3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoValidTilings;

/// Upstream's tiling search: vary the outline, anneal against each, keep what fits.
///
/// 🔑 **Two passes of `num_runs`, one per axis.** The width is varied with the height held, then
/// the height with the width held — 20 runs in all at the default. The run counter is reset
/// between the passes, so both walk the same factor list from the start.
///
/// ⛔ **`fits_in` is tested against the ORIGINAL outline, not the varied one.** A run given a
/// shrunken outline that overflows it can still contribute, so long as the result fits the real
/// one. That is the point of shrinking: it pushes the annealer into tighter packings whose
/// results are then judged against the true outline.
///
/// ⚠️ **Results go into a SET keyed on `(width, height)`** — duplicates collapse, which they
/// frequently do, since neighbouring outline factors often anneal to the same answer.
///
/// ⚠️ The final ordering is by AREA and then by width — a total order, so there is no ambiguity
/// to reproduce.
///
/// ⚠️ **The aspect-ratio filter is applied only if something survives it.** A run whose every
/// tiling is too extreme keeps them all rather than returning nothing.
#[allow(clippy::too_many_arguments)]
pub fn search_tilings(
    macros: &[SoftMacro],
    curves: &[ShapeCurve],
    outline_width: i32,
    outline_height: i32,
    dbu_per_micron: i32,
    probabilities: ActionProbabilities,
    search: &TilingSearch,
) -> Result<TilingResult, NoValidTilings> {
    let factors = search.vary_factors();
    let mut found: Vec<(i32, i32)> = Vec::new();

    let run_one = |width: i32, height: i32, found: &mut Vec<(i32, i32)>| {
        let mut state = new_search(
            macros,
            curves,
            width,
            height,
            dbu_per_micron,
            probabilities,
        );
        let fixed_present = !state.fixed_bboxes.is_empty();
        let mut rng = Mt19937::new(search.random_seed);
        // Upstream's adjustment at THIS call site — see `SaParameters::perturbations_for`.
        let mut sa = search.sa;
        sa.num_perturb_per_step = sa.perturbations_for(macros.len()) as i32;
        let temperature = state.initialize(&mut rng, &sa);
        state.fast_sa(&mut rng, &sa, temperature, fixed_present);
        // ⛔ Against the ORIGINAL outline.
        if state.width <= outline_width && state.height <= outline_height {
            found.push((state.width, state.height));
        }
    };

    // Vary the width, holding the height.
    for &factor in &factors {
        // ⚠️ `int * float` narrowed back to `int`, so this truncates.
        let varied = (outline_width as f32 * factor) as i32;
        run_one(varied, outline_height, &mut found);
    }
    // Vary the height, holding the width. ⚠️ The factor list restarts from the beginning.
    for &factor in &factors {
        let varied = (outline_height as f32 * factor) as i32;
        run_one(outline_width, varied, &mut found);
    }

    // The set: dedup on the pair, ordered by width then height.
    found.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    found.dedup();
    if found.is_empty() {
        return Err(NoValidTilings);
    }

    // ⚠️ `isAreaSmaller`: area, then width. A total order, so ties are decided too.
    found.sort_by(|a, b| {
        let (area_a, area_b) = (a.0 as i64 * a.1 as i64, b.0 as i64 * b.1 as i64);
        area_a.cmp(&area_b).then(a.0.cmp(&b.0))
    });

    let aspect_ratio = |t: &(i32, i32)| t.1 as f32 / t.0 as f32;
    let filtered: Vec<(i32, i32)> = found
        .iter()
        .copied()
        .filter(|t| {
            let ratio = aspect_ratio(t);
            ratio >= search.min_ar && ratio <= 1.0 / search.min_ar
        })
        .collect();

    // ⚠️ Only if something survived; otherwise the extreme tilings are kept.
    let chosen = if filtered.is_empty() { found.clone() } else { filtered };
    Ok(TilingResult { all: found, chosen })
}

/// What the search produced.
///
/// 🔑 **Both lists are needed, because the reference TRACES one and USES the other.** The
/// per-tiling debug line is emitted for every tiling found, before the aspect-ratio filter runs;
/// the summary line and the cluster's shapes come from the filtered list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TilingResult {
    /// Everything that fit, ordered by area then width — before the aspect-ratio filter.
    pub all: Vec<(i32, i32)>,
    /// What the filter left, or everything if it left nothing.
    pub chosen: Vec<(i32, i32)>,
}

/// A search over a fresh copy of the macros, packed from the identity sequence pair.
///
/// ⚠️ **Each run starts from the SAME macro shapes.** The reference passes the macro list by
/// const reference and the core copies it, so one run's resizing never reaches the next.
fn new_search(
    macros: &[SoftMacro],
    curves: &[ShapeCurve],
    outline_width: i32,
    outline_height: i32,
    dbu_per_micron: i32,
    probabilities: ActionProbabilities,
) -> Search {
    let mut state = Search {
        macros: macros.to_vec(),
        curves: curves.to_vec(),
        sp: init_sequence_pair(macros.len()),
        width: 0,
        height: 0,
        penalties: Penalties::default(),
        placement: None,
        outline_width,
        outline_height,
        dbu_per_micron,
        fixed_bboxes: Vec::new(),
        weights: SoftWeights::default(),
        normalization: Normalization::default(),
        probabilities,
        action: None,
        hard_probabilities: None,
        cost_history: Vec::new(),
    };
    // Upstream `findFixedMacros`, which walks the positive sequence.
    state.fixed_bboxes = state
        .sp
        .pos
        .iter()
        .filter(|&&id| state.macros[id].fixed)
        .map(|&id| state.macros[id].bbox())
        .collect();
    let (width, height) = pack_floorplan(&mut state.macros, &state.sp);
    state.width = width;
    state.height = height;
    state.cal_penalty();
    state
}

// ---------------------------------------------------------------- normalisation and temperature

/// One accepted sample of `initialize`'s sweep.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub width: i32,
    pub height: i32,
    pub area: f32,
    pub penalties: Penalties,
}
