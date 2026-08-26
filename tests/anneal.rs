// SPDX-License-Identifier: Apache-2.0
//! The sequence-pair packer.
//!
//! 🔑 **A sequence pair encodes relations, not positions.** For two macros `a` and `b`: `a` before
//! `b` in BOTH sequences means `a` is left of `b`; `a` before `b` in the positive sequence and
//! after it in the negative means `a` is ABOVE `b`. Every test here is that rule, checked through
//! the packer rather than asserted about it.

use vyges_mpl::anneal::{pack_floorplan, SequencePair, SoftMacro};

fn m(width: i32, height: i32) -> SoftMacro {
    SoftMacro { width, height, ..Default::default() }
}

fn sp(pos: &[usize], neg: &[usize]) -> SequencePair {
    SequencePair { pos: pos.to_vec(), neg: neg.to_vec() }
}

/// Same order in both sequences — side by side, so the widths ADD and the height is the taller.
#[test]
fn the_same_order_in_both_sequences_places_left_to_right() {
    let mut macros = [m(100, 40), m(30, 70)];
    let (w, h) = pack_floorplan(&mut macros, &sp(&[0, 1], &[0, 1]));
    assert_eq!((w, h), (130, 70));
    assert_eq!((macros[0].x, macros[0].y), (0, 0));
    assert_eq!(macros[1].x, 100, "the second starts where the first ends");
    assert_eq!(macros[1].y, 0, "and they share the bottom edge");
}

/// Reversed in the negative sequence — stacked, so the heights ADD and the width is the wider.
///
/// ⚠️ The macro that comes FIRST in the positive sequence ends up on TOP, which is the opposite of
/// the reading most people expect from "first".
#[test]
fn reversing_the_negative_sequence_stacks_them() {
    let mut macros = [m(100, 40), m(30, 70)];
    let (w, h) = pack_floorplan(&mut macros, &sp(&[0, 1], &[1, 0]));
    assert_eq!((w, h), (100, 110));
    assert_eq!(macros[1].y, 0, "the second in the positive sequence is underneath");
    assert_eq!(macros[0].y, 70, "and the first sits on top of it");
}

/// ⚠️ **A fixed macro is not moved, but still displaces.** The packer skips the assignment and
/// then pushes the accumulated edge along with the position the macro already had, so its
/// neighbours are placed around it.
#[test]
fn a_fixed_macro_keeps_its_position_and_still_displaces_its_neighbour() {
    let mut macros = [
        SoftMacro { x: 500, y: 300, width: 100, height: 40, fixed: true },
        m(30, 70),
    ];
    let (w, _) = pack_floorplan(&mut macros, &sp(&[0, 1], &[0, 1]));
    assert_eq!((macros[0].x, macros[0].y), (500, 300), "it did not move");
    assert_eq!(macros[1].x, 600, "and the next macro starts past its right edge");
    assert_eq!(w, 630);
}

/// Three in a row, to exercise the propagation across more than one slot.
#[test]
fn three_in_sequence_accumulate_across_every_slot() {
    let mut macros = [m(10, 5), m(20, 5), m(30, 5)];
    let (w, h) = pack_floorplan(&mut macros, &sp(&[0, 1, 2], &[0, 1, 2]));
    assert_eq!((w, h), (60, 5));
    assert_eq!([macros[0].x, macros[1].x, macros[2].x], [0, 10, 30]);
}

/// 🔑 **An L-shape: the two rules at once.** `0` is left of `1`; `2` is above both. The bounding
/// box has to come from the taller stack, not from either rule alone.
#[test]
fn a_mixed_pair_places_in_two_dimensions() {
    // pos = [2, 0, 1], neg = [0, 1, 2]:
    //   2 is after 0 and 1 in the positive sequence read forwards? No — 2 is FIRST in pos and
    //   LAST in neg, so 2 is above both 0 and 1, which are side by side.
    let mut macros = [m(40, 10), m(60, 10), m(100, 25)];
    let (w, h) = pack_floorplan(&mut macros, &sp(&[2, 0, 1], &[0, 1, 2]));
    assert_eq!((macros[0].x, macros[0].y), (0, 0));
    assert_eq!((macros[1].x, macros[1].y), (40, 0));
    assert_eq!(macros[2].y, 10, "the third sits on the row below it");
    assert_eq!((w, h), (100, 35));
}

/// ℹ️ An empty pair has nothing to pack. Upstream reads past the end of an empty vector here; a
/// cluster with nothing in it is never shaped, so neither behaviour is ever reached.
#[test]
fn an_empty_sequence_pair_packs_to_nothing() {
    let mut macros: [SoftMacro; 0] = [];
    assert_eq!(pack_floorplan(&mut macros, &sp(&[], &[])), (0, 0));
}

/// ⚠️ One macro: the bounding box is the macro, and it lands at the origin.
#[test]
fn a_single_macro_lands_at_the_origin() {
    let mut macros = [m(70, 90)];
    assert_eq!(pack_floorplan(&mut macros, &sp(&[0], &[0])), (70, 90));
    assert_eq!((macros[0].x, macros[0].y), (0, 0));
}

/// 🔑 **Packing is a pure function of the pair.** Running it twice from different starting
/// positions gives the same answer — which is what lets the annealer restore a state by restoring
/// the sequences alone.
#[test]
fn packing_twice_gives_the_same_result() {
    let pair = sp(&[2, 0, 1], &[1, 2, 0]);
    let mut a = [m(40, 10), m(60, 30), m(100, 25)];
    let first = pack_floorplan(&mut a, &pair);

    let mut b = [
        SoftMacro { x: 999, y: 888, width: 40, height: 10, fixed: false },
        SoftMacro { x: 7, y: 7, width: 60, height: 30, fixed: false },
        SoftMacro { x: -5, y: -5, width: 100, height: 25, fixed: false },
    ];
    let second = pack_floorplan(&mut b, &pair);
    assert_eq!(first, second);
    assert_eq!(a.map(|s| (s.x, s.y)), b.map(|s| (s.x, s.y)));
}

// ---------------------------------------------------------------- the perturbations

use vyges_mpl::anneal::{
    choose_action, double_seq_swap, exchange_macros, generate_random_indices, single_seq_swap,
    Action, ActionProbabilities,
};
use vyges_mpl::rng::{uniform_int, Mt19937};

/// The shares the shaping search is constructed with, before normalisation.
fn shaping_probabilities() -> ActionProbabilities {
    // 0.2 each for the four swaps and 0.0 for resize is what `resetSAParameters` leaves behind on
    // a design with no standard cells; the defaults carry a non-zero resize share.
    ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2)
}

/// ⚠️ **`<=`, so a draw landing exactly on a boundary picks the EARLIER action.** With five equal
/// shares the first boundary is 0.2, and 0.2 must select the first action, not the second.
#[test]
fn a_draw_on_the_boundary_picks_the_earlier_action() {
    let p = shaping_probabilities();
    assert_eq!(p.action_for(0.0), Action::SwapPositive);
    assert_eq!(p.action_for(p.pos_swap), Action::SwapPositive, "the boundary belongs to the first");
    assert_eq!(p.action_for(0.21), Action::SwapNegative);
    assert_eq!(p.action_for(0.99), Action::Resize);
}

/// ⚠️ **Resize is the fall-through**, never a threshold. With the four swap shares at zero every
/// draw reaches it — every draw but one.
///
/// ⛔ **A zero-probability action still fires on a draw of exactly `0.0`.** The dispatch is
/// `draw <= threshold`, and with the first share at zero the test is `0.0 <= 0.0`, which holds.
/// The draw is a float in `[0, 1)` and `0.0` is reachable — the generator emitting the word `0`
/// produces it — so this is a real state, not a theoretical one. Reproduced deliberately: an
/// implementation using `<` would diverge from the reference on that draw and nowhere else, which
/// is precisely the kind of difference no sampling would ever find.
#[test]
fn resize_is_the_fall_through_except_on_an_exact_zero_draw() {
    let p = ActionProbabilities::normalized(0.0, 0.0, 0.0, 0.0, 1.0);
    for draw in [0.5f32, 0.999_999, f32::from_bits(0x3f7fffff)] {
        assert_eq!(p.action_for(draw), Action::Resize, "draw {draw}");
    }
    assert_eq!(
        p.action_for(0.0),
        Action::SwapPositive,
        "0.0 <= 0.0 selects the first action even though its share is zero"
    );
}

/// ⚠️ The reachability of that zero: the generator's word `0` maps to exactly `0.0`.
#[test]
fn a_zero_word_produces_an_exact_zero_draw() {
    assert_eq!(0u32 as f32 / 4_294_967_296.0f32, 0.0);
}

/// ⛔ **No macros, no draw.** The generator must be untouched, or every later step shifts.
#[test]
fn an_empty_macro_list_consumes_no_randomness() {
    let mut g = Mt19937::new(7);
    assert_eq!(choose_action(&mut g, &shaping_probabilities(), 0), None);
    let mut fresh = Mt19937::new(7);
    assert_eq!(g.next(), fresh.next(), "the generator did not advance");
}

/// 🔑 **The indices are always distinct, and the retry costs extra words.** This reproduces the
/// draw sequence by hand from the verified integer distribution, so it pins the retry rather than
/// merely observing that the results differ.
#[test]
fn random_indices_are_distinct_and_retry_on_a_collision() {
    // Find a seed where the first two draws over n=2 collide, so the retry path is exercised.
    let n = 2usize;
    let mut seed = 0u32;
    let (collide, draws) = loop {
        let mut g = Mt19937::new(seed);
        let a = uniform_int(&mut g, n as u32);
        let b = uniform_int(&mut g, n as u32);
        if a == b {
            let mut count = 2;
            let mut g2 = Mt19937::new(seed);
            let first = uniform_int(&mut g2, n as u32);
            let mut second = uniform_int(&mut g2, n as u32);
            while first == second {
                second = uniform_int(&mut g2, n as u32);
                count += 1;
            }
            break (seed, count);
        }
        seed += 1;
        assert!(seed < 10_000, "no colliding seed found, which cannot happen for n = 2");
    };

    let mut g = Mt19937::new(collide);
    let (i, j) = generate_random_indices(&mut g, n);
    assert_ne!(i, j, "the pair is always distinct");

    // The generator must have consumed exactly the number of words the hand replay needed.
    let mut replay = Mt19937::new(collide);
    for _ in 0..draws {
        let _ = uniform_int(&mut replay, n as u32);
    }
    assert_eq!(g.next(), replay.next(), "consumed {draws} words, as the retry requires");
}

/// A single-sequence swap touches ONE sequence and leaves the other alone.
#[test]
fn single_seq_swap_touches_only_the_named_sequence() {
    let mut g = Mt19937::new(3);
    let mut sp = SequencePair { pos: vec![0, 1, 2], neg: vec![2, 0, 1] };
    let before = sp.neg.clone();
    single_seq_swap(&mut g, &mut sp, true);
    assert_eq!(sp.neg, before, "the negative sequence is untouched");
    assert_ne!(sp.pos, vec![0, 1, 2], "and the positive one moved");
}

/// ⚠️ **`double_seq_swap` swaps POSITIONS, so it generally moves four macros, not two.** That is
/// what distinguishes it from `exchange_macros`, and a fixture where the two sequences hold the
/// same order cannot tell them apart.
#[test]
fn double_seq_swap_swaps_positions_not_macros() {
    let mut g = Mt19937::new(11);
    let mut sp = SequencePair { pos: vec![0, 1, 2, 3], neg: vec![3, 2, 1, 0] };
    let (before_pos, before_neg) = (sp.pos.clone(), sp.neg.clone());
    double_seq_swap(&mut g, &mut sp);

    let moved_pos: Vec<usize> =
        (0..4).filter(|&k| sp.pos[k] != before_pos[k]).map(|k| before_pos[k]).collect();
    let moved_neg: Vec<usize> =
        (0..4).filter(|&k| sp.neg[k] != before_neg[k]).map(|k| before_neg[k]).collect();
    assert_eq!(moved_pos.len(), 2);
    assert_eq!(moved_neg.len(), 2);
    // With the sequences reversed relative to each other, the two macros moved in the negative
    // sequence are NOT the two moved in the positive one.
    assert_ne!(
        {
            let mut a = moved_pos.clone();
            a.sort_unstable();
            a
        },
        {
            let mut b = moved_neg.clone();
            b.sort_unstable();
            b
        },
        "position swapping moved a different pair in each sequence"
    );
}

/// 🔑 **`exchange_macros` moves exactly TWO macros**, tracking them into the negative sequence by
/// identity rather than by index.
#[test]
fn exchange_macros_moves_the_same_two_macros_in_both_sequences() {
    let mut g = Mt19937::new(11);
    let mut sp = SequencePair { pos: vec![0, 1, 2, 3], neg: vec![3, 2, 1, 0] };
    let (before_pos, before_neg) = (sp.pos.clone(), sp.neg.clone());
    exchange_macros(&mut g, &mut sp);

    let mut moved_pos: Vec<usize> =
        (0..4).filter(|&k| sp.pos[k] != before_pos[k]).map(|k| before_pos[k]).collect();
    let mut moved_neg: Vec<usize> =
        (0..4).filter(|&k| sp.neg[k] != before_neg[k]).map(|k| before_neg[k]).collect();
    moved_pos.sort_unstable();
    moved_neg.sort_unstable();
    assert_eq!(moved_pos.len(), 2);
    assert_eq!(moved_pos, moved_neg, "the SAME two macros moved in both sequences");

    // Both remain permutations of the original set.
    let mut p = sp.pos.clone();
    p.sort_unstable();
    let mut q = sp.neg.clone();
    q.sort_unstable();
    assert_eq!(p, vec![0, 1, 2, 3]);
    assert_eq!(q, vec![0, 1, 2, 3]);
}

/// ⛔ A sequence of one cannot be perturbed, and must not draw. Upstream computes `size() - 1` on
/// an unsigned type, so calling through would ask for a range of `SIZE_MAX`.
#[test]
fn a_single_element_sequence_is_left_alone_and_draws_nothing() {
    for positive in [true, false] {
        let mut g = Mt19937::new(5);
        let mut sp = SequencePair { pos: vec![0], neg: vec![0] };
        single_seq_swap(&mut g, &mut sp, positive);
        assert_eq!((sp.pos.clone(), sp.neg.clone()), (vec![0], vec![0]));
        let mut fresh = Mt19937::new(5);
        assert_eq!(g.next(), fresh.next(), "no words consumed");
    }
    let mut g = Mt19937::new(5);
    let mut sp = SequencePair { pos: vec![0], neg: vec![0] };
    double_seq_swap(&mut g, &mut sp);
    exchange_macros(&mut g, &mut sp);
    let mut fresh = Mt19937::new(5);
    assert_eq!(g.next(), fresh.next(), "no words consumed by either");
}
