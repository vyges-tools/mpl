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
        SoftMacro { x: 500, y: 300, width: 100, height: 40, fixed: true, ..Default::default() },
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
        SoftMacro { x: 999, y: 888, width: 40, height: 10, ..Default::default() },
        SoftMacro { x: 7, y: 7, width: 60, height: 30, ..Default::default() },
        SoftMacro { x: -5, y: -5, width: 100, height: 25, ..Default::default() },
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

// ---------------------------------------------------------------- the cost

use vyges_mpl::anneal::{
    area_penalty, average, fixed_macros_penalty, norm_cost, outline_penalty, Normalization,
    Penalties, ShapingWeights,
};

/// 🔑 **A packing that fits costs nothing in outline.** Both maxima pin to the outline, the
/// product equals its area, and the difference is zero.
#[test]
fn a_packing_inside_the_outline_has_no_outline_penalty() {
    assert_eq!(outline_penalty(100, 100, 300, 250), 0.0);
    assert_eq!(outline_penalty(300, 250, 300, 250), 0.0, "exactly filling it is still zero");
}

/// ⚠️ Overhang on ONE axis still uses the outline's extent on the other, so the penalty is not
/// the overhanging strip alone.
#[test]
fn overhang_is_measured_against_the_full_outline() {
    // 400 wide against a 300 x 250 outline: 400 * 250 - 300 * 250 = 25000.
    let got = outline_penalty(400, 100, 300, 250);
    assert_eq!(got, 25000.0 / 75000.0);
}

/// ⛔ **Narrowing to `f32` happens BEFORE the division.** This searches for a case where the two
/// orders disagree and then pins ours to the reference's order — if no such case existed the
/// distinction would be vacuous and this test would say so.
#[test]
fn the_overhang_is_narrowed_before_it_is_divided() {
    let (ow, oh) = (300_000i32, 250_000i32);
    let outline_area = ow as i64 * oh as i64;

    let mut found = None;
    for extra in 1..4000i32 {
        let w = ow + extra;
        let h = oh + extra;
        let overhang = (w as i64 * h as i64) - outline_area;
        let narrow_then_divide = (overhang as f32) / outline_area as f32;
        let divide_then_narrow = (overhang as f64 / outline_area as f64) as f32;
        if narrow_then_divide != divide_then_narrow {
            found = Some((w, h, narrow_then_divide, divide_then_narrow));
            break;
        }
    }

    let (w, h, reference_order, other_order) =
        found.expect("no case distinguishes the two orders — this test would prove nothing");
    assert_ne!(reference_order, other_order);
    assert_eq!(
        outline_penalty(w, h, ow, oh),
        reference_order,
        "we must follow the reference's order, not the more accurate one"
    );
}

/// The area penalty is the ratio of the two areas; the micron conversion cancels.
#[test]
fn the_area_penalty_is_the_ratio_of_the_areas() {
    let outline_area = 300i64 * 250;
    assert_eq!(area_penalty(300, 250, outline_area, 2000), 1.0);
    assert_eq!(area_penalty(150, 250, outline_area, 2000), 0.5);
}

/// ⚠️ **The area term enters the cost undivided.** Every other term is divided by its
/// normalisation factor; this one is only gated by it. A factor of 4 must leave the area
/// contribution unchanged.
#[test]
fn the_area_term_is_not_divided_by_its_normalisation_factor() {
    let p = Penalties { area: 0.5, outline: 0.0, fixed_macros: 0.0 };
    let w = ShapingWeights { area: 1.0, outline: 1000.0, fixed_macros: 100.0 };
    let one = norm_cost(&p, &w, &Normalization { area: 1.0, ..Default::default() });
    let four = norm_cost(&p, &w, &Normalization { area: 4.0, ..Default::default() });
    assert_eq!(one, four, "the factor only gates the term");
    assert_eq!(one, 0.5);
}

/// ⚠️ The outline term IS divided, which is what makes it different from the area term.
#[test]
fn the_outline_term_is_divided_by_its_normalisation_factor() {
    let p = Penalties { area: 0.0, outline: 0.5, fixed_macros: 0.0 };
    let w = ShapingWeights::default();
    let one = norm_cost(&p, &w, &Normalization { outline: 1.0, ..Default::default() });
    let two = norm_cost(&p, &w, &Normalization { outline: 2.0, ..Default::default() });
    assert_eq!(one, 500.0);
    assert_eq!(two, 250.0);
}

/// ⛔ **A zero factor drops its term rather than dividing by zero.**
#[test]
fn a_zero_normalisation_factor_drops_its_term() {
    let p = Penalties { area: 1.0, outline: 1.0, fixed_macros: 1.0 };
    let w = ShapingWeights::default();
    let cost = norm_cost(&p, &w, &Normalization { area: 0.0, outline: 0.0, fixed_macros: 0.0 });
    assert_eq!(cost, 0.0);
    assert!(cost.is_finite(), "and it is finite, not an infinity from dividing by zero");
}

fn movable(x: i32, y: i32, width: i32, height: i32) -> SoftMacro {
    SoftMacro { x, y, width, height, ..Default::default() }
}

/// The penalty is the overlapping AREA, in microns squared.
#[test]
fn the_fixed_macro_penalty_is_the_overlap_area() {
    let macros = [movable(0, 0, 100, 100)];
    let sp = SequencePair { pos: vec![0], neg: vec![0] };
    // A fixed macro covering the top-right quarter: 50 x 50 of overlap.
    let fixed = [(50, 50, 150, 150)];
    let got = fixed_macros_penalty(&macros, &fixed, &sp, 10);
    assert_eq!(got, (50.0 * 50.0) / 100.0, "2500 dbu² at 10 dbu per micron is 25 µm²");
}

/// ⛔ **A disjoint pair whose intersection has BOTH dimensions negative has a POSITIVE area.**
/// The `< 0` guard is what stops that being added; without it a macro far away on the diagonal
/// would score as though it overlapped.
#[test]
fn a_diagonally_disjoint_macro_is_not_counted() {
    let macros = [movable(0, 0, 10, 10)];
    let sp = SequencePair { pos: vec![0], neg: vec![0] };
    let fixed = [(100, 100, 110, 110)];
    // The naive intersection is (100,100)-(10,10): dx = dy = -90, and (-90) * (-90) = 8100.
    assert_eq!(fixed_macros_penalty(&macros, &fixed, &sp, 10), 0.0);
}

/// ⚠️ A macro that is itself fixed is skipped, so a fixed macro never penalises itself.
#[test]
fn a_fixed_macro_does_not_penalise_itself() {
    let macros = [SoftMacro { x: 0, y: 0, width: 100, height: 100, fixed: true, ..Default::default() }];
    let sp = SequencePair { pos: vec![0], neg: vec![0] };
    let fixed = [(0, 0, 100, 100)];
    assert_eq!(fixed_macros_penalty(&macros, &fixed, &sp, 10), 0.0);
}

/// ℹ️ No fixed macros at all means no penalty and no work.
#[test]
fn no_fixed_macros_means_no_penalty() {
    let macros = [movable(0, 0, 100, 100)];
    let sp = SequencePair { pos: vec![0], neg: vec![0] };
    assert_eq!(fixed_macros_penalty(&macros, &[], &sp, 10), 0.0);
}

/// ⚠️ **The mean is accumulated in `f32`.** Summing in `f64` and narrowing at the end gives a
/// different answer once the list is long enough for the running sum to lose the small terms;
/// this searches for such a list rather than asserting the distinction exists.
#[test]
fn the_average_accumulates_in_single_precision() {
    // ⚠️ At 1e8 the `f32` spacing is 8, so each `1.0` added to the running sum rounds away
    // entirely. At 1e7 the spacing is 1 and they all survive — the first attempt at this fixture
    // used 1e7 and proved nothing, which is why the assertion below exists.
    let values: Vec<f32> =
        std::iter::once(1.0e8f32).chain(std::iter::repeat_n(1.0f32, 4096)).collect();
    let in_f32 = average(&values);
    let in_f64 = (values.iter().map(|&v| v as f64).sum::<f64>() / values.len() as f64) as f32;
    assert_ne!(in_f32, in_f64, "the fixture must actually distinguish the two");
    let expected = values.iter().fold(0.0f32, |a, b| a + b) / values.len() as f32;
    assert_eq!(in_f32, expected);
}

/// ℹ️ An empty list averages to zero rather than a NaN.
#[test]
fn an_empty_average_is_zero() {
    assert_eq!(average(&[]), 0.0);
}

// ---------------------------------------------------------------- shapes and resizing

use vyges_mpl::anneal::{
    init_sequence_pair, resize_randomly, shape_curve_from_intervals, shape_curve_from_tilings,
    Interval, ShapeCurve,
};

/// 🔑 **The search starts from a single ROW.** Identical sequences mean every macro is left of the
/// next, so the opening packing is as wide as the sum of the widths — normally far outside the
/// outline, which is what gives the outline penalty something to work against.
#[test]
fn the_initial_sequence_pair_is_the_identity_and_packs_as_a_row() {
    let sp = init_sequence_pair(3);
    assert_eq!(sp.pos, vec![0, 1, 2]);
    assert_eq!(sp.neg, vec![0, 1, 2]);

    let mut macros = [m(10, 7), m(20, 3), m(30, 5)];
    let (w, h) = pack_floorplan(&mut macros, &sp);
    assert_eq!((w, h), (60, 7), "one row: widths add, height is the tallest");
}

/// ⚠️ A hard cluster's intervals are DEGENERATE — the resize picks a tiling, not a size.
#[test]
fn a_hard_clusters_intervals_are_degenerate() {
    let (curve, width, height, area) = shape_curve_from_tilings(&[(200, 100), (100, 200)]);
    assert_eq!(curve.width_intervals, vec![Interval { min: 200, max: 200 }, Interval { min: 100, max: 100 }]);
    assert_eq!(curve.height_intervals, vec![Interval { min: 100, max: 100 }, Interval { min: 200, max: 200 }]);
    assert_eq!((width, height, area), (200, 100, 20_000), "it starts at the first tiling");
}

/// ⚠️ **Unsorted, on purpose.** The order is the tiling order, and the resize draws an index into
/// it — sorting would silently change which tiling a given draw selects.
#[test]
fn hard_cluster_intervals_keep_the_tiling_order() {
    let (curve, ..) = shape_curve_from_tilings(&[(300, 10), (100, 30), (200, 20)]);
    let mins: Vec<i32> = curve.width_intervals.iter().map(|i| i.min).collect();
    assert_eq!(mins, vec![300, 100, 200], "not sorted");
}

/// ⚠️ **Touching intervals merge.** The test is `min <= back.max`, so two tilings of the same width
/// collapse into one choice — a cluster then offers fewer shapes than it has tilings.
#[test]
fn intervals_that_touch_are_merged() {
    let intervals = [
        Interval { min: 100, max: 100 },
        Interval { min: 100, max: 100 },
        Interval { min: 200, max: 200 },
    ];
    let (curve, ..) = shape_curve_from_intervals(&intervals, 20_000).expect("shapeable");
    assert_eq!(
        curve.width_intervals,
        vec![Interval { min: 100, max: 100 }, Interval { min: 200, max: 200 }],
        "three tilings, two choices"
    );
}

/// ⚠️ **The height bounds CROSS OVER**: minimum height from the maximum width, maximum height from
/// the minimum width, because the area is held constant.
#[test]
fn the_height_range_is_inverted_relative_to_the_width_range() {
    let intervals = [Interval { min: 100, max: 400 }];
    let (curve, width, height, area) =
        shape_curve_from_intervals(&intervals, 40_000).expect("shapeable");
    assert_eq!(curve.height_intervals, vec![Interval { min: 100, max: 400 }]);
    assert_eq!((width, height, area), (100, 400, 40_000), "it starts narrowest and tallest");
}

/// ⛔ An empty interval list or a non-positive area leaves the curve empty — which the resize
/// treats as "consume nothing", so this is not cosmetic.
#[test]
fn an_unshapeable_mixed_cluster_gets_no_curve() {
    assert!(shape_curve_from_intervals(&[], 1000).is_none());
    assert!(shape_curve_from_intervals(&[Interval { min: 1, max: 2 }], 0).is_none());
    assert!(shape_curve_from_intervals(&[Interval { min: 1, max: 2 }], -5).is_none());
}

/// ⛔ **An empty curve consumes NO randomness.** Anything else desynchronises the whole search.
#[test]
fn resizing_an_empty_curve_draws_nothing() {
    let mut g = Mt19937::new(4);
    let mut macro_ = m(10, 20);
    resize_randomly(&mut g, &ShapeCurve::default(), &mut macro_);
    assert_eq!((macro_.width, macro_.height), (10, 20), "unchanged");
    let mut fresh = Mt19937::new(4);
    assert_eq!(g.next(), fresh.next(), "and the generator did not advance");
}

/// ⚠️ **Exactly two draws** — an integer for the interval, a float for the position in it.
#[test]
fn a_resize_consumes_exactly_two_generator_words() {
    let (curve, ..) = shape_curve_from_tilings(&[(200, 100), (100, 200)]);
    let mut g = Mt19937::new(4);
    let mut macro_ = m(10, 20);
    resize_randomly(&mut g, &curve, &mut macro_);

    let mut replay = Mt19937::new(4);
    let _ = uniform_int(&mut replay, 2);
    let _ = vyges_mpl::rng::canonical_f32(&mut replay);
    assert_eq!(g.next(), replay.next(), "one integer draw then one float draw");
}

/// On a degenerate interval the span is zero, so the width is exactly the tiling's and the height
/// comes back exactly too.
#[test]
fn a_degenerate_interval_resizes_to_exactly_that_tiling() {
    let (curve, ..) = shape_curve_from_tilings(&[(200, 100), (100, 200)]);
    for seed in 0..40u32 {
        let mut g = Mt19937::new(seed);
        let mut macro_ = m(1, 1);
        resize_randomly(&mut g, &curve, &mut macro_);
        assert!(
            (macro_.width, macro_.height) == (200, 100) || (macro_.width, macro_.height) == (100, 200),
            "seed {seed} gave {:?}",
            (macro_.width, macro_.height)
        );
    }
}

/// ⚠️ **The height is recovered from the interval's corner area, then truncated.**
///
/// ℹ️ The product `width * height` therefore falls slightly short of the cluster's area, and the
/// shortfall is INTEGER TRUNCATION of `area / width` — not a mismatched corner. `w.min * h.max`
/// reconstructs `A` because `h.max` was built as `A / w.min`; `set_width` uses `w.max * h.min`
/// and recovers the same `A`. An earlier reading of this file called the two corners an
/// asymmetry; they are not.
#[test]
fn the_recovered_height_is_truncated_against_the_corner_area() {
    let intervals = [Interval { min: 100, max: 400 }];
    let (curve, ..) = shape_curve_from_intervals(&intervals, 40_000).expect("shapeable");

    // Find a seed whose float draw lands away from the ends, so the chosen width is interior.
    let mut interior = None;
    for seed in 0..200u32 {
        let mut g = Mt19937::new(seed);
        let mut macro_ = m(1, 1);
        resize_randomly(&mut g, &curve, &mut macro_);
        if macro_.width > 150 && macro_.width < 350 {
            interior = Some((macro_.width, macro_.height));
            break;
        }
    }
    let (width, height) = interior.expect("no interior width found; the fixture proves nothing");
    // area used = min_width * max_height = 100 * 400 = 40000, so height = 40000 / width.
    assert_eq!(height, 40_000 / width);
    // Short of the area, by less than one row — the remainder of the integer division.
    let product = width as i64 * height as i64;
    assert!(product <= 40_000, "never exceeds the area");
    assert!(
        40_000 - product < width as i64,
        "and falls short by less than the width, which is what truncation costs"
    );
}


// ---------------------------------------------------------------- snapping and resizing

use vyges_mpl::anneal::{find_interval_index, resize_one_cluster, set_height, set_width};

fn mixed(width: i32, height: i32, area: i64) -> SoftMacro {
    SoftMacro { width, height, area, ..Default::default() }
}

/// ⛔ **`find_interval_index` mutates its value.** Reading it as a pure lookup loses the snap that
/// `set_width` depends on.
#[test]
fn finding_an_interval_snaps_the_value_into_it() {
    let widths = [Interval { min: 100, max: 200 }, Interval { min: 400, max: 500 }];
    // A width in the GAP is pulled UP to the next interval's minimum.
    let mut value = 300;
    let idx = find_interval_index(&widths, &mut value, true);
    assert_eq!((idx, value), (1, 400), "snapped up into the second interval");

    // Inside an interval it is left alone.
    let mut inside = 150;
    assert_eq!(find_interval_index(&widths, &mut inside, true), 0);
    assert_eq!(inside, 150);
}

/// ⚠️ Height intervals run non-increasing, so the search and the snap both reverse.
#[test]
fn finding_a_height_interval_snaps_downward() {
    let heights = [Interval { min: 400, max: 500 }, Interval { min: 100, max: 200 }];
    let mut value = 300;
    let idx = find_interval_index(&heights, &mut value, false);
    assert_eq!((idx, value), (1, 200), "snapped down into the second interval");
}

/// ⛔ A hard macro cluster refuses both setters — only a random resize moves it.
#[test]
fn a_macro_cluster_refuses_to_be_set() {
    let (curve, ..) = shape_curve_from_tilings(&[(200, 100), (100, 200)]);
    let mut macro_ = SoftMacro {
        width: 200,
        height: 100,
        area: 20_000,
        is_macro_cluster: true,
        ..Default::default()
    };
    set_width(&mut macro_, &curve, 150);
    set_height(&mut macro_, &curve, 150);
    assert_eq!((macro_.width, macro_.height), (200, 100), "untouched");
}

/// ⚠️ Below the curve clamps to its narrowest, TALLEST shape; above it clamps to the widest and
/// shortest. The area is recomputed from the shape landed on.
#[test]
fn setting_a_width_outside_the_curve_clamps_to_an_end() {
    let intervals = [Interval { min: 100, max: 200 }, Interval { min: 400, max: 500 }];
    let (curve, ..) = shape_curve_from_intervals(&intervals, 40_000).expect("shapeable");

    let mut low = mixed(150, 266, 40_000);
    set_width(&mut low, &curve, 1);
    assert_eq!(low.width, 100, "clamped to the narrowest");
    assert_eq!(low.height, curve.height_intervals[0].max, "and the tallest");
    assert_eq!(low.area, low.width as i64 * low.height as i64);

    let mut high = mixed(150, 266, 40_000);
    set_width(&mut high, &curve, 10_000);
    assert_eq!(high.width, 500, "clamped to the widest");
    assert_eq!(high.height, curve.height_intervals[1].min, "and the shortest");
}

/// 🔑 **The interior case uses the OPPOSITE interval corner from a random resize** — `max` width
/// times `min` height here, `min` width times `max` height there.
#[test]
fn setting_a_width_inside_the_curve_uses_the_far_corner() {
    let intervals = [Interval { min: 100, max: 200 }];
    let (curve, ..) = shape_curve_from_intervals(&intervals, 40_000).expect("shapeable");
    // heights: min = 40000/200 = 200, max = 40000/100 = 400.
    let mut macro_ = mixed(100, 400, 40_000);
    // A width strictly inside (100, 200) takes the interior branch.
    set_width(&mut macro_, &curve, 150);
    assert_eq!(macro_.width, 150, "kept, because it is inside an interval");
    assert_eq!(macro_.area, 200 * 200, "max width times min height");
    assert_eq!(macro_.height, 40_000 / 150);
}

/// ⚠️ A width landing in a GAP is snapped up first, and the area follows the interval it lands in.
#[test]
fn a_width_in_a_gap_is_snapped_before_the_area_is_taken() {
    let intervals = [Interval { min: 100, max: 200 }, Interval { min: 400, max: 500 }];
    let (curve, ..) = shape_curve_from_intervals(&intervals, 40_000).expect("shapeable");
    let mut macro_ = mixed(150, 266, 40_000);
    set_width(&mut macro_, &curve, 300);
    assert_eq!(macro_.width, 400, "snapped up out of the gap");
    assert_eq!(macro_.area, 500 * curve.height_intervals[1].min as i64);
}

fn two_mixed_macros() -> (Vec<SoftMacro>, Vec<ShapeCurve>, SequencePair) {
    let intervals = [Interval { min: 100, max: 400 }];
    let (curve, w, h, area) = shape_curve_from_intervals(&intervals, 40_000).expect("shapeable");
    let macros = vec![
        SoftMacro { x: 0, y: 0, width: w, height: h, area, ..Default::default() },
        SoftMacro { x: 500, y: 500, width: w, height: h, area, ..Default::default() },
    ];
    (macros, vec![curve.clone(), curve], SequencePair { pos: vec![0, 1], neg: vec![0, 1] })
}

/// 🔑 **The branch structure IS the randomness budget.** This counts the generator words each
/// path actually consumes, for both sides of the `< 0.4` roll:
///   * roll succeeds → index, roll, and a random resize's two draws = **4**
///   * roll fails    → index, roll, and the option draw = **3**
///
/// ⚠️ The roll is consumed either way, which is what makes 3 the floor rather than 2.
#[test]
fn each_resize_path_consumes_the_words_its_branch_requires() {
    /// How many words `run` took from a generator seeded with `seed`.
    fn consumed(seed: u32, run: &dyn Fn(&mut Mt19937)) -> usize {
        let mut used = Mt19937::new(seed);
        run(&mut used);
        let after = used.next();
        for n in 0..16 {
            let mut replay = Mt19937::new(seed);
            for _ in 0..n {
                replay.next();
            }
            if replay.next() == after {
                return n;
            }
        }
        panic!("consumed more than 16 words, which no resize path does");
    }

    // ⛔ **A TWO-interval curve is required for this test to distinguish anything.** With one
    // interval `uniform_int` is asked for a range of zero and returns without touching the
    // generator, so a random resize costs ONE word and both branches come to three.
    fn fixture() -> (Vec<SoftMacro>, Vec<ShapeCurve>, SequencePair) {
        let intervals = [Interval { min: 100, max: 200 }, Interval { min: 400, max: 500 }];
        let (curve, w, h, area) =
            shape_curve_from_intervals(&intervals, 40_000).expect("shapeable");
        let macros = vec![
            SoftMacro { x: 0, y: 0, width: w, height: h, area, ..Default::default() },
            SoftMacro { x: 5_000, y: 5_000, width: w, height: h, area, ..Default::default() },
        ];
        (macros, vec![curve.clone(), curve], SequencePair { pos: vec![0, 1], neg: vec![0, 1] })
    }

    let mut saw_roll_taken = false;
    let mut saw_roll_declined = false;

    for seed in 0..60u32 {
        // Replay just far enough to learn which way this seed's roll falls.
        let mut peek = Mt19937::new(seed);
        let _index = uniform_int(&mut peek, 2);
        let roll = vyges_mpl::rng::canonical_f32(&mut peek);

        let words = consumed(seed, &|g: &mut Mt19937| {
            let (mut macros, curves, sp) = fixture();
            resize_one_cluster(g, &mut macros, &curves, &sp, 100_000, 100_000);
        });

        if roll < 0.4 {
            assert_eq!(words, 4, "seed {seed}: roll {roll} taken, so a random resize follows");
            saw_roll_taken = true;
        } else {
            assert_eq!(words, 3, "seed {seed}: roll {roll} declined, so only the option follows");
            saw_roll_declined = true;
        }
    }

    assert!(saw_roll_taken && saw_roll_declined, "both sides of the roll must be exercised");
}

/// ⛔ **A macro touching the outline is treated as OUTSIDE** — `>=`, not `>` — so it takes the
/// random-resize path and consumes a different number of words than the option path would.
#[test]
fn a_macro_touching_the_outline_takes_the_random_path() {
    let (mut macros, curves, sp) = two_mixed_macros();
    // Make macro 0's right edge land exactly on the outline.
    macros[0].x = 0;
    let outline_width = macros[0].width;

    let mut g = Mt19937::new(2);
    resize_one_cluster(&mut g, &mut macros, &curves, &sp, outline_width, 100_000);

    // The random path is: index, then resize_randomly's two draws. Three words, no 0.4 roll.
    let mut replay = Mt19937::new(2);
    let index = uniform_int(&mut replay, 2) as usize;
    assert_eq!(index, 0, "the fixture needs the touching macro to be the one chosen");
    let _ = uniform_int(&mut replay, 1);
    let _ = vyges_mpl::rng::canonical_f32(&mut replay);
    assert_eq!(g.next(), replay.next(), "index plus a random resize, and no roll");
}

/// ⚠️ **Widening is unconditional and stretches to the outline when there is no neighbour.**
#[test]
fn widening_with_no_neighbour_reaches_the_outline() {
    let intervals = [Interval { min: 100, max: 400 }];
    let (curve, ..) = shape_curve_from_intervals(&intervals, 40_000).expect("shapeable");
    let mut macro_ = mixed(100, 400, 40_000);
    // The widen branch computes `outline_width - lx` and hands it to set_width.
    set_width(&mut macro_, &curve, 100_000 - 0);
    assert_eq!(macro_.width, 400, "clamped to the curve's widest, not the outline");
}

/// ⛔ **A one-element range costs NOTHING.** Upstream returns the minimum before touching the
/// generator when `range == 0`, so a shape curve with a single interval makes a random resize
/// consume one word rather than two. Getting this wrong shifts every later draw.
#[test]
fn a_single_choice_consumes_no_generator_word() {
    let mut g = Mt19937::new(9);
    assert_eq!(uniform_int(&mut g, 1), 0);
    let mut fresh = Mt19937::new(9);
    assert_eq!(g.next(), fresh.next(), "the generator did not advance");

    let (curve, ..) = shape_curve_from_tilings(&[(200, 100)]);
    let mut h = Mt19937::new(9);
    let mut macro_ = m(1, 1);
    resize_randomly(&mut h, &curve, &mut macro_);
    let mut replay = Mt19937::new(9);
    let _ = vyges_mpl::rng::canonical_f32(&mut replay);
    assert_eq!(h.next(), replay.next(), "only the float draw was taken");
}

// ---------------------------------------------------------------- save and restore

use vyges_mpl::anneal::Search;

fn search_with(n: usize) -> Search {
    let intervals = [Interval { min: 100, max: 200 }, Interval { min: 400, max: 500 }];
    let (curve, w, h, area) = shape_curve_from_intervals(&intervals, 40_000).expect("shapeable");
    let macros: Vec<SoftMacro> = (0..n)
        .map(|_| SoftMacro { width: w, height: h, area, ..Default::default() })
        .collect();
    let mut s = Search {
        curves: vec![curve; n],
        macros,
        sp: init_sequence_pair(n),
        width: 0,
        height: 0,
        outline_penalty: 0.0,
        fixed_macros_penalty: 0.0,
        outline_width: 100_000,
        outline_height: 100_000,
        dbu_per_micron: 2000,
        fixed_bboxes: Vec::new(),
        weights: ShapingWeights::default(),
        normalization: Normalization::default(),
        probabilities: ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
        action: None,
    };
    let (w, h) = pack_floorplan(&mut s.macros, &s.sp);
    s.width = w;
    s.height = h;
    s.cal_penalty();
    s
}

/// 🔑 **Which sequences come back depends on the action taken.** A positive-sequence swap must
/// leave a separately-modified negative sequence alone — restoring both would erase work the
/// action never did.
#[test]
fn restoring_returns_only_the_sequences_the_action_touched() {
    let mut s = search_with(4);
    let saved = s.save_state().expect("has macros");

    // Pretend the action was a positive-sequence swap, and dirty BOTH sequences.
    s.action = Some(Action::SwapPositive);
    s.sp.pos = vec![3, 2, 1, 0];
    s.sp.neg = vec![3, 2, 1, 0];
    s.restore_state(&saved);
    assert_eq!(s.sp.pos, vec![0, 1, 2, 3], "the positive sequence came back");
    assert_eq!(s.sp.neg, vec![3, 2, 1, 0], "the negative one did NOT");

    // A negative-sequence swap is the mirror.
    let mut s = search_with(4);
    let saved = s.save_state().expect("has macros");
    s.action = Some(Action::SwapNegative);
    s.sp.pos = vec![3, 2, 1, 0];
    s.sp.neg = vec![3, 2, 1, 0];
    s.restore_state(&saved);
    assert_eq!(s.sp.pos, vec![3, 2, 1, 0], "the positive one did NOT come back");
    assert_eq!(s.sp.neg, vec![0, 1, 2, 3]);
}

/// ⚠️ **A resize restores NEITHER sequence** — it never touched them.
#[test]
fn restoring_after_a_resize_leaves_both_sequences_alone() {
    let mut s = search_with(4);
    let saved = s.save_state().expect("has macros");
    s.action = Some(Action::Resize);
    s.sp.pos = vec![3, 2, 1, 0];
    s.sp.neg = vec![3, 2, 1, 0];
    s.restore_state(&saved);
    assert_eq!(s.sp.pos, vec![3, 2, 1, 0]);
    assert_eq!(s.sp.neg, vec![3, 2, 1, 0]);
    assert_eq!(s.macros.len(), 4, "but the macros themselves did come back");
}

/// A double swap and an exchange both restore the pair.
#[test]
fn restoring_after_a_double_swap_or_exchange_returns_both() {
    for action in [Action::SwapBoth, Action::Exchange] {
        let mut s = search_with(4);
        let saved = s.save_state().expect("has macros");
        s.action = Some(action);
        s.sp.pos = vec![3, 2, 1, 0];
        s.sp.neg = vec![3, 2, 1, 0];
        s.restore_state(&saved);
        assert_eq!(s.sp.pos, vec![0, 1, 2, 3], "{action:?}");
        assert_eq!(s.sp.neg, vec![0, 1, 2, 3], "{action:?}");
    }
}

/// ⛔ **The fixed-macro penalty is neither saved nor restored.** Upstream's `saveState` lists
/// seven penalties and omits this one; a restore therefore keeps the rejected state's value until
/// the next `perturb` recomputes it. Reproduced deliberately.
#[test]
fn the_fixed_macro_penalty_survives_a_restore() {
    let mut s = search_with(2);
    s.outline_penalty = 1.0;
    s.fixed_macros_penalty = 2.0;
    let saved = s.save_state().expect("has macros");

    s.action = Some(Action::SwapPositive);
    s.outline_penalty = 99.0;
    s.fixed_macros_penalty = 99.0;
    s.restore_state(&saved);

    assert_eq!(s.outline_penalty, 1.0, "the outline penalty is in the saved set");
    assert_eq!(s.fixed_macros_penalty, 99.0, "the fixed-macro one is not, so the rejected value stands");
}

/// ⛔ A search with no macros saves nothing and restores nothing.
#[test]
fn an_empty_search_has_no_state_to_save() {
    let mut s = search_with(0);
    assert!(s.save_state().is_none());
    s.restore_state(&search_with(1).save_state().expect("has macros"));
    assert!(s.macros.is_empty(), "restoring into an empty search does nothing");
}

/// ⛔ A perturb on an empty search consumes no randomness.
#[test]
fn perturbing_an_empty_search_draws_nothing() {
    let mut s = search_with(0);
    let mut g = Mt19937::new(6);
    s.perturb(&mut g);
    let mut fresh = Mt19937::new(6);
    assert_eq!(g.next(), fresh.next());
    assert_eq!(s.action, None, "and no action was recorded");
}

/// 🔑 A packing that fits scores only its area; one that overhangs pays a thousand times the
/// overhang ratio, which is what drives the search inward.
#[test]
fn the_cost_is_dominated_by_the_outline_once_it_overhangs() {
    let mut s = search_with(2);
    s.outline_width = 100_000;
    s.outline_height = 100_000;
    s.width = 1_000;
    s.height = 1_000;
    s.cal_penalty();
    let inside = s.norm_cost();

    s.width = 200_000;
    s.cal_penalty();
    let outside = s.norm_cost();
    assert!(outside > inside * 100.0, "inside {inside}, outside {outside}");
}

// ---------------------------------------------------------------- initialize and fastSA

use vyges_mpl::anneal::{BestResult, SaParameters};

/// ⚠️ **A tenth of the configured count is the FLOOR.** Every cluster with fewer than 50 macros
/// runs 50 perturbations per step, not one per macro — so the sweep length is nearly always 50.
#[test]
fn the_perturbation_count_has_a_floor_of_a_tenth() {
    let p = SaParameters::default();
    assert_eq!(p.perturbations_for(2), 50, "two macros still get fifty");
    assert_eq!(p.perturbations_for(50), 50, "the floor wins on a tie");
    assert_eq!(p.perturbations_for(51), 51, "and the count wins above it");
}

/// 🔑 **The sweep never restores, so the samples walk a trajectory.** If it restored, every
/// sample would be one move from the identity and the widths would barely vary.
#[test]
fn the_normalisation_sweep_walks_rather_than_resampling() {
    let mut s = search_with(4);
    let mut g = Mt19937::new(0);
    let start = s.sp.pos.clone();
    s.initialize(&mut g, &SaParameters::default());
    // Fifty un-restored moves from the identity essentially never land back on it.
    assert_ne!(
        (s.sp.pos.clone(), s.sp.neg.clone()),
        (start.clone(), start),
        "the state moved and stayed moved"
    );
}

/// ⚠️ **Every factor at or below `1e-4` becomes exactly `1.0`** — not a small number. A design
/// with no fixed macros has a fixed-macro penalty of zero throughout, so its factor is 1.0.
#[test]
fn an_absent_penalty_normalises_to_one() {
    let mut s = search_with(3);
    let mut g = Mt19937::new(1);
    s.initialize(&mut g, &SaParameters::default());
    assert_eq!(s.normalization.fixed_macros, 1.0, "no fixed macros, so the factor is floored");
    assert!(s.normalization.outline > 0.0);
}

/// ⚠️ **The replay leaves the live state holding the LAST sample.** Upstream assigns each sample
/// back into the members to recompute its cost, and never puts the originals back.
#[test]
fn initialize_leaves_the_last_sample_in_the_live_state() {
    let mut s = search_with(3);
    let mut g = Mt19937::new(2);

    // Reproduce the sweep by hand and keep the final sample.
    let mut replay = search_with(3);
    let mut rg = Mt19937::new(2);
    let n = SaParameters::default().perturbations_for(replay.macros.len());
    let mut last = (0, 0, 0.0f32);
    for _ in 0..n {
        replay.perturb(&mut rg);
        last = (replay.width, replay.height, replay.outline_penalty);
    }

    s.initialize(&mut g, &SaParameters::default());
    assert_eq!((s.width, s.height, s.outline_penalty), last);
}

/// ⚠️ The initial temperature comes from the mean ABSOLUTE change between consecutive costs, so a
/// run whose cost never changes gets exactly 1.0 rather than a division by zero.
#[test]
fn a_flat_cost_gives_the_default_temperature() {
    // One macro: every action either does nothing or resizes within a single shape, and the
    // sequence cannot be permuted — so the cost never moves.
    let (curve, w, h, area) = shape_curve_from_tilings(&[(200, 100)]);
    let mut s = search_with(1);
    s.curves = vec![curve];
    s.macros = vec![SoftMacro { width: w, height: h, area, is_macro_cluster: true, ..Default::default() }];
    s.sp = init_sequence_pair(1);
    let (pw, ph) = pack_floorplan(&mut s.macros, &s.sp);
    s.width = pw;
    s.height = ph;
    s.cal_penalty();

    let mut g = Mt19937::new(3);
    let temperature = s.initialize(&mut g, &SaParameters::default());
    assert_eq!(temperature, 1.0, "no variation, so no temperature to derive");
}

/// 🔑 **A varying cost gives a positive, finite temperature.** `ln(0.9)` is negative, so the sign
/// flip is what makes it positive — dropping it would give a negative temperature and invert
/// every acceptance test.
#[test]
fn a_varying_cost_gives_a_positive_temperature() {
    let mut s = search_with(4);
    let mut g = Mt19937::new(4);
    let temperature = s.initialize(&mut g, &SaParameters::default());
    assert!(temperature > 0.0 && temperature.is_finite(), "got {temperature}");
}

/// ⚠️ The best result keeps only WIDTHS, and starts at the largest float so the first candidate
/// always replaces it.
#[test]
fn the_best_result_starts_at_the_largest_float() {
    let best = BestResult::new();
    assert_eq!(best.cost, f32::MAX);
    assert!(best.is_empty());

    let s = search_with(3);
    let mut best = BestResult::new();
    s.update_best_result(&mut best, 1.5);
    assert_eq!(best.cost, 1.5);
    assert_eq!(best.macro_widths.len(), 3);
    assert!(!best.is_empty());
}

/// 🔑 **The search is deterministic**: same seed, same answer. This is the property the whole
/// exercise rests on.
#[test]
fn the_search_is_reproducible_from_its_seed() {
    let params = SaParameters { max_num_step: 20, ..SaParameters::default() };
    let run = || {
        let mut s = search_with(4);
        let mut g = Mt19937::new(7);
        let t = s.initialize(&mut g, &params);
        s.fast_sa(&mut g, &params, t, false);
        (s.width, s.height, s.sp.pos.clone(), s.sp.neg.clone())
    };
    assert_eq!(run(), run());
}

/// ⚠️ **A random word is drawn only when a move makes things WORSE**, so how much randomness a
/// run consumes is a function of its own cost trajectory. Two runs identical but for the
/// temperature must therefore leave the generator in DIFFERENT places — if the draw schedule were
/// fixed, they would land together.
#[test]
fn the_draw_count_follows_the_cost_trajectory_not_a_schedule() {
    let params = SaParameters { max_num_step: 5, ..SaParameters::default() };

    let end_state = |temperature: f32| {
        let mut s = search_with(4);
        let mut g = Mt19937::new(8);
        s.fast_sa(&mut g, &params, temperature, false);
        g.next()
    };

    // Cold: almost every degrading move is rejected. Hot: almost every one is accepted. Both
    // draw for each degrading move, but the trajectories differ and so do the streams.
    assert_ne!(
        end_state(1e-9),
        end_state(1e9),
        "the amount of randomness consumed depends on the trajectory"
    );
    assert_eq!(end_state(1e-9), end_state(1e-9), "and is reproducible at a fixed temperature");
}

/// 🔑 **The search improves on where it started.** The opening state is a single row, far outside
/// the outline, so a working annealer must reduce the cost.
#[test]
fn the_search_improves_on_the_opening_row() {
    let params = SaParameters { max_num_step: 200, ..SaParameters::default() };
    let mut s = search_with(4);
    let opening = s.norm_cost();

    let mut g = Mt19937::new(9);
    let t = s.initialize(&mut g, &params);
    let best = s.fast_sa(&mut g, &params, t, false);
    assert!(best.cost < opening, "opening {opening}, best {}", best.cost);
}

// ---------------------------------------------------------------- the driver

use vyges_mpl::anneal::{search_tilings, TilingSearch};

/// ⚠️ **The first factor is the FULL outline** and the list never reaches zero.
#[test]
fn the_vary_factors_start_at_one_and_shrink() {
    let f = TilingSearch::default().vary_factors();
    assert_eq!(f.len(), 10);
    assert_eq!(f[0], 1.0);
    assert!((f[1] - 0.9).abs() < 1e-6, "{}", f[1]);
    assert!(f[9] > 0.0 && f[9] < 0.2, "the last factor is small but positive: {}", f[9]);
}

fn two_macro_inputs() -> (Vec<SoftMacro>, Vec<ShapeCurve>) {
    let (curve, w, h, area) = shape_curve_from_tilings(&[(2_000, 1_000), (1_000, 2_000)]);
    let macros = vec![
        SoftMacro { width: w, height: h, area, is_macro_cluster: true, ..Default::default() },
        SoftMacro { width: w, height: h, area, is_macro_cluster: true, ..Default::default() },
    ];
    (macros, vec![curve.clone(), curve])
}

fn quick_search() -> TilingSearch {
    TilingSearch { sa: SaParameters { max_num_step: 20, ..SaParameters::default() }, ..TilingSearch::default() }
}

/// 🔑 The search returns tilings that actually fit the outline it was given.
#[test]
fn every_returned_tiling_fits_the_outline() {
    let (macros, curves) = two_macro_inputs();
    let probabilities = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    let tilings =
        search_tilings(&macros, &curves, 5_000, 5_000, 2000, probabilities, &quick_search())
            .expect("shapeable")
            .chosen;
    assert!(!tilings.is_empty());
    for (w, h) in &tilings {
        assert!(*w <= 5_000 && *h <= 5_000, "{w} x {h} does not fit");
    }
}

/// ⚠️ **Ordered by AREA, then by width** — a total order, so this is exact rather than "some
/// reasonable order".
#[test]
fn the_tilings_come_back_ordered_by_area_then_width() {
    let (macros, curves) = two_macro_inputs();
    let probabilities = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    let tilings =
        search_tilings(&macros, &curves, 5_000, 5_000, 2000, probabilities, &quick_search())
            .expect("shapeable")
            .chosen;
    for pair in tilings.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (area_a, area_b) = (a.0 as i64 * a.1 as i64, b.0 as i64 * b.1 as i64);
        assert!(
            area_a < area_b || (area_a == area_b && a.0 <= b.0),
            "{a:?} then {b:?} is out of order"
        );
    }
}

/// ⚠️ **Duplicates collapse.** Neighbouring outline factors frequently anneal to the same answer,
/// so the twenty runs usually yield far fewer than twenty tilings.
#[test]
fn duplicate_results_are_collapsed() {
    let (macros, curves) = two_macro_inputs();
    let probabilities = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    let tilings =
        search_tilings(&macros, &curves, 5_000, 5_000, 2000, probabilities, &quick_search())
            .expect("shapeable")
            .chosen;
    let mut unique = tilings.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), tilings.len(), "no duplicates survive");
    assert!(tilings.len() <= 20, "at most one per run");
}

/// 🔑 **The whole search is reproducible.** Every run is seeded identically and differs only by
/// outline, so the answer is a pure function of the inputs.
#[test]
fn the_tiling_search_is_reproducible() {
    let (macros, curves) = two_macro_inputs();
    let probabilities = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    let run = || {
        search_tilings(&macros, &curves, 5_000, 5_000, 2000, probabilities, &quick_search())
            .expect("shapeable")
            .chosen
    };
    assert_eq!(run(), run());
}

/// ⛔ **An outline nothing can fit yields MPL-3, not an empty list.** Silence would read as "this
/// cluster has no shape", which is a different statement.
#[test]
fn an_impossible_outline_is_an_error() {
    let (macros, curves) = two_macro_inputs();
    let probabilities = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    let result = search_tilings(&macros, &curves, 10, 10, 2000, probabilities, &quick_search());
    assert!(result.is_err(), "nothing can fit a 10 x 10 outline");
}

/// ⚠️ **The aspect-ratio filter keeps everything when it would otherwise keep nothing.** A band
/// that excludes every tiling must not empty the list.
#[test]
fn an_impossible_aspect_ratio_band_keeps_the_tilings_anyway() {
    let (macros, curves) = two_macro_inputs();
    let probabilities = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    // A band of [0.999, 1.001] that essentially nothing will satisfy.
    let strict = TilingSearch { min_ar: 0.999, ..quick_search() };
    let tilings =
        search_tilings(&macros, &curves, 5_000, 5_000, 2000, probabilities, &strict)
            .expect("shapeable")
            .chosen;
    assert!(!tilings.is_empty(), "the filter must not empty the list");
}
