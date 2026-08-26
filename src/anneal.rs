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
