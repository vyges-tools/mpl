// SPDX-License-Identifier: Apache-2.0
//! Configuring the ten runs of macro placement, and choosing between them.

use vyges_mpl::anneal::SoftWeights;
use vyges_mpl::placement::{
    best_macro_run, macro_array_setup, macro_perturbations_per_step, macro_placement_probabilities,
    macro_run_seed, macro_run_weights, HardActionProbabilities,
};

/// The command's own defaults for the four hard-macro actions.
fn defaults(masters: usize, macros: usize) -> HardActionProbabilities {
    macro_placement_probabilities(0.2, 0.2, 0.2, 0.2, masters, macros)
}

/// ⚠️ Whatever the inputs, the four come out as a distribution.
#[test]
fn the_four_probabilities_normalise() {
    for (m, n) in [(1usize, 8usize), (4, 8), (7, 8), (8, 8)] {
        let p = defaults(m, n);
        let sum = p.pos_swap + p.neg_swap + p.double_swap + p.exchange;
        assert!((sum - 1.0).abs() < 1e-6, "masters {m} of {n}: sum {sum}");
    }
}

/// 🔑 **Exchange is scaled by master sharing.** Every macro having its own master makes the factor
/// exactly zero and switches exchange OFF; sharing one master drives it up.
#[test]
fn exchange_is_switched_off_when_no_master_is_shared() {
    let all_distinct = defaults(8, 8);
    assert_eq!(all_distinct.exchange, 0.0, "1 - 8/8 is zero");

    let all_shared = defaults(1, 8);
    assert!(all_shared.exchange > 0.0);

    // More sharing, more exchange.
    assert!(defaults(2, 8).exchange > defaults(6, 8).exchange);
}

/// ⛔ **The single swaps are scaled by TEN and the double swap is not**, so a hard-macro run is
/// pushed hard towards single swaps and the double swap becomes rare.
#[test]
fn the_double_swap_is_ten_times_rarer_than_a_single_one() {
    // With no master sharing, exchange is zero, so only the three swaps remain.
    let p = defaults(8, 8);
    assert_eq!(p.exchange, 0.0);
    assert!((p.pos_swap / p.double_swap - 10.0).abs() < 1e-4, "{p:?}");
    assert!((p.pos_swap - p.neg_swap).abs() < 1e-7, "the two single swaps are equal");

    // ⚠️ **The ratio alone is not enough.** It is blind to the shared denominator, so scaling the
    // double swap inside the SUM but not in the result leaves the ratio at ten while every
    // probability shifts. A mutation proved exactly that — hence the absolute values.
    // Sum = 0.2*10 + 0.2*10 + 0.2 + 0 = 4.2.
    assert_eq!(p.pos_swap.to_bits(), 0.476_190_5f32.to_bits(), "2.0 / 4.2");
    assert_eq!(p.double_swap.to_bits(), 0.047_619_052f32.to_bits(), "0.2 / 4.2");
}

/// ⚠️ The raw inputs are equal; only the scaling makes them differ. A reader who assumes equal
/// inputs mean equal probabilities gets this backwards.
#[test]
fn equal_inputs_do_not_give_equal_probabilities() {
    let p = defaults(8, 8);
    assert_ne!(p.pos_swap, p.double_swap);
}

// ---------------------------------------------------------------- perturbations

/// ⛔ **The floor is a TENTH of the configured count, by integer division.** A cluster with fewer
/// macros than that still gets the floor.
#[test]
fn a_small_cluster_gets_the_floor_not_its_macro_count() {
    assert_eq!(macro_perturbations_per_step(500, 2, false), 50, "not 2");
    assert_eq!(macro_perturbations_per_step(500, 50, false), 50, "exactly at the floor");
}

/// ⚠️ **Past the floor the count tracks the problem size** — one perturbation per macro.
#[test]
fn a_large_cluster_is_perturbed_once_per_macro() {
    assert_eq!(macro_perturbations_per_step(500, 51, false), 51);
    assert_eq!(macro_perturbations_per_step(500, 400, false), 400);
}

/// 🔑 **A large macro ARRAY gets the FULL count**, not a tenth — upstream says large arrays need
/// more steps to converge.
#[test]
fn a_large_macro_array_gets_the_full_count() {
    assert_eq!(macro_perturbations_per_step(500, 51, true), 500);
    // A SMALL array is not excepted — it takes the floor like anything else.
    assert_eq!(macro_perturbations_per_step(500, 2, true), 50);
}

/// ⚠️ **A configured count below ten floors to zero, which makes EVERY cluster "large"** — so the
/// count stops being a floor at all and every cluster is perturbed once per macro. The comparison
/// is against the floor, not against the configured value, so the two collapse together.
/// ℹ️ Unreachable today: the count is a class constant of `500`. Written down because the floor
/// reads like a minimum and stops behaving as one.
#[test]
fn a_floor_of_zero_makes_every_cluster_large() {
    assert_eq!(macro_perturbations_per_step(9, 2, false), 2, "its macro count, not the floor");
    assert_eq!(macro_perturbations_per_step(9, 2, true), 9, "and an array takes the full count");
    // A cluster with no macros at all is not larger than a zero floor.
    assert_eq!(macro_perturbations_per_step(9, 0, false), 0);
}

// ---------------------------------------------------------------- the macro-array branch

/// 🔑 **An array with no empty space does nothing but exchange** — there are no shapes to explore,
/// only an arrangement to reorder.
#[test]
fn a_full_array_only_exchanges() {
    let got = macro_array_setup(defaults(1, 8), true, false);
    assert_eq!(
        got.probabilities,
        HardActionProbabilities { pos_swap: 0.0, neg_swap: 0.0, double_swap: 0.0, exchange: 1.0 }
    );
    assert!(got.invalid_states_allowed);
}

/// ⛔ **An array WITH empty space disallows invalid states** — the only place in the engine that
/// flag is ever set — and leaves the probabilities alone.
#[test]
fn an_array_with_empty_space_disallows_invalid_states() {
    let base = defaults(1, 8);
    let got = macro_array_setup(base, true, true);
    assert!(!got.invalid_states_allowed);
    assert_eq!(got.probabilities, base, "untouched");
}

/// ⚠️ A cluster that is not an array is left entirely alone.
#[test]
fn a_non_array_cluster_is_untouched() {
    let base = defaults(3, 8);
    let got = macro_array_setup(base, false, true);
    assert_eq!(got.probabilities, base);
    assert!(got.invalid_states_allowed, "even though empty space was reported");
}

// ---------------------------------------------------------------- the run ramp

/// 🔑 **Ten runs is a RAMP, not ten samples.** Each run squeezes the outline harder and cares less
/// about wire length than the last.
#[test]
fn each_run_is_a_harder_version_of_the_last() {
    let base = SoftWeights { outline: 100.0, wirelength: 100.0, ..SoftWeights::placement_defaults() };

    let first = macro_run_weights(base, 0);
    assert_eq!(first.outline, 1000.0, "already ten times the base");
    assert_eq!(first.wirelength, 100.0, "and wirelength untouched on the first run");

    let last = macro_run_weights(base, 9);
    assert_eq!(last.outline, 10_000.0);
    assert_eq!(last.wirelength, 10.0);
}

/// ⚠️ Nothing but outline and wirelength moves.
#[test]
fn the_ramp_touches_only_two_weights() {
    let base = SoftWeights::placement_defaults();
    let ramped = macro_run_weights(base, 4);
    assert_eq!(ramped.area, base.area);
    assert_eq!(ramped.guidance, base.guidance);
    assert_eq!(ramped.fence, base.fence);
    assert_eq!(ramped.boundary, base.boundary);
    assert_eq!(ramped.notch, base.notch);
}

/// ⛔ **Each run gets a different seed**, unlike the tiling search where every run shares one.
#[test]
fn each_run_gets_its_own_seed() {
    assert_eq!(macro_run_seed(1000, 0), 1000);
    assert_eq!(macro_run_seed(1000, 9), 1009);
    let seeds: Vec<u32> = (0..10).map(|i| macro_run_seed(7, i)).collect();
    let unique: std::collections::BTreeSet<u32> = seeds.iter().copied().collect();
    assert_eq!(unique.len(), 10, "all ten differ");
}

// ---------------------------------------------------------------- choosing a run

/// ⛔ **The BEST cost wins, not the first valid one** — the opposite of cluster placement.
#[test]
fn the_cheapest_valid_run_wins() {
    let costs = [(true, 9.0), (true, 3.0), (true, 5.0)];
    assert_eq!(best_macro_run(&costs), Some(1));
}

/// ⚠️ **`<`, strictly, so a tie goes to the LOWEST run id** — the run that was least squeezed.
#[test]
fn a_tie_goes_to_the_earlier_run() {
    let costs = [(true, 3.0), (true, 3.0), (true, 3.0)];
    assert_eq!(best_macro_run(&costs), Some(0));
}

/// ⚠️ An invalid run is never a candidate, however cheap.
#[test]
fn an_invalid_run_is_never_chosen() {
    let costs = [(false, 0.001), (true, 9.0)];
    assert_eq!(best_macro_run(&costs), Some(1));
}

/// ⚠️ Nothing valid at all is the failure the caller reports.
#[test]
fn no_valid_run_is_none() {
    assert_eq!(best_macro_run(&[(false, 1.0), (false, 2.0)]), None);
    assert_eq!(best_macro_run(&[]), None);
}

// ---------------------------------------------------------------- fences, guides, and the grid

use vyges_mpl::placement::{array_sequence_pair, clip_region_to_outline, macro_fences_and_guides};

const OUT: (i32, i32, i32, i32) = (1000, 1000, 3000, 3000);

/// 🔑 A fence overlapping the outline is clipped to it and rebased onto it.
#[test]
fn a_fence_is_clipped_and_rebased() {
    assert_eq!(clip_region_to_outline((0, 0, 2000, 2000), OUT), (0, 0, 1000, 1000));
    assert_eq!(clip_region_to_outline((1500, 1500, 2500, 2500), OUT), (500, 500, 1500, 1500));
}

/// ⛔ **A fence that MISSES the outline becomes a degenerate box at a NEGATIVE position** — the
/// reference's `intersection` writes a zero rect on a miss, and the rebase then shifts it by the
/// outline's origin. It is not dropped, unlike on the cluster path.
#[test]
fn a_missing_fence_becomes_a_degenerate_box_at_a_negative_position() {
    assert_eq!(clip_region_to_outline((0, 0, 500, 500), OUT), (-1000, -1000, -1000, -1000));
}

/// ⚠️ **Clipping is INCLUSIVE**, so a fence touching the outline along a line survives as a
/// zero-width box at a real position — a different thing from the miss above.
#[test]
fn a_touching_fence_survives_as_a_zero_width_box() {
    // Its right edge is exactly the outline's left edge.
    assert_eq!(clip_region_to_outline((0, 1500, 1000, 2500), OUT), (0, 500, 0, 1500));
}

/// ⛔ **There is NO area test**, unlike the cluster path — every macro that HAS a fence gets an
/// entry, degenerate or not, and every entry counts towards the fence term's divisor.
#[test]
fn every_macro_with_a_fence_gets_an_entry() {
    let fence_of = |i: usize| match i {
        0 => Some((1500, 1500, 2500, 2500)),
        2 => Some((0, 0, 500, 500)), // misses the outline entirely
        _ => None,
    };
    let guide_of = |_: usize| None;
    let (fences, guides) = macro_fences_and_guides(&fence_of, &guide_of, 4, OUT);

    assert_eq!(fences.len(), 2, "the missing one is recorded too, not dropped");
    assert_eq!(fences[0], (0usize, (500, 500, 1500, 1500)));
    assert_eq!(fences[1], (2usize, (-1000, -1000, -1000, -1000)));
    assert!(guides.is_empty());
}

/// ⚠️ Fences and guides are looked up independently and kept separate.
#[test]
fn fences_and_guides_are_independent() {
    let fence_of = |i: usize| (i == 1).then_some((1500, 1500, 2500, 2500));
    let guide_of = |i: usize| (i == 3).then_some((1000, 1000, 2000, 2000));
    let (fences, guides) = macro_fences_and_guides(&fence_of, &guide_of, 4, OUT);
    assert_eq!(fences, vec![(1usize, (500, 500, 1500, 1500))]);
    assert_eq!(guides, vec![(3usize, (0, 0, 1000, 1000))]);
}

// ---------------------------------------------------------------- the array grid

/// 🔑 **The positive sequence is `0..n`; the negative walks the grid column by column, downwards
/// within each column.** That pairing is what encodes "laid out as a grid".
#[test]
fn the_grid_is_encoded_column_by_column_downwards() {
    // A 2 x 2 array of 100 x 100 macros in a 200 x 200 cluster.
    let got = array_sequence_pair(4, 200, 200, 100, 100);
    assert_eq!(got.pos, vec![0, 1, 2, 3]);
    // i=1: (2*1)-1=1, (2*1)-2=0.  i=2: (2*2)-1=3, (2*2)-2=2.
    assert_eq!(got.neg, vec![1, 0, 3, 2]);
    assert!(!got.has_empty_space);
}

/// ⛔ **`std::round` is applied to an INTEGER division that has already truncated.** A cluster 2.9
/// macros wide gives TWO columns, not three — reading it as "the ratio, rounded" gives a different
/// grid for every cluster whose width is not an exact multiple.
#[test]
fn the_column_count_truncates_rather_than_rounding() {
    // 290 / 100 = 2 by integer division. Rounding 2.9 would give 3.
    let got = array_sequence_pair(6, 290, 100, 100, 100);
    // One row, two columns: i=1 -> 0, i=2 -> 1.
    assert_eq!(got.neg, vec![0, 1], "two columns, not three");
    assert!(!got.has_empty_space, "and no gap is reported, because the third column is not tried");
}

/// ⚠️ **A grid position past the last macro sets `has_empty_space`**, and that flag is what makes
/// the caller disallow invalid states.
#[test]
fn a_gap_in_the_grid_is_reported() {
    // A 2 x 2 grid holding only three macros.
    let got = array_sequence_pair(3, 200, 200, 100, 100);
    assert!(got.has_empty_space);
    assert_eq!(got.neg, vec![1, 0, 2], "the missing position is skipped, not padded");
}

/// ⛔ **The flag reports a SLOT WITH NO MACRO, never a MACRO WITH NO SLOT.** A grid too small for
/// the macros it holds reports no empty space at all — and leaves a negative sequence SHORTER than
/// the positive one, so the two are not permutations of each other.
///
/// ⚠️ Reading `has_empty_space` as "the grid and the macros disagree" gets this exactly backwards
/// on the half that matters: an undersized grid is the case that produces a malformed sequence
/// pair, and it is the case the flag stays silent about.
#[test]
fn an_undersized_grid_reports_no_empty_space_and_a_short_sequence() {
    let got = array_sequence_pair(4, 100, 100, 100, 100);
    assert_eq!(got.pos.len(), 4);
    assert_eq!(got.neg, vec![0], "a one-by-one grid holding four macros");
    assert!(!got.has_empty_space, "silent, though three macros have nowhere to go");
}

/// ℹ️ A cluster smaller than one macro yields no grid at all, and reports no gap — the loops never
/// run.
#[test]
fn a_cluster_narrower_than_one_macro_yields_no_grid() {
    let got = array_sequence_pair(4, 50, 50, 100, 100);
    assert!(got.neg.is_empty());
    assert!(!got.has_empty_space);
    assert_eq!(got.pos, vec![0, 1, 2, 3], "the positive sequence is still every macro");
}

// ---------------------------------------------------------------- the hard-macro core

use vyges_mpl::anneal::{Action, Normalization, Penalties};
use vyges_mpl::placement::{hard_norm_cost, hard_sampled_extent, init_temperature, norm_floor};

/// 🔑 **Boundary, soft blockage, fixed macros and notch do not exist for a hard-macro run** — they
/// are the soft core's own members, not the base class's.
#[test]
fn the_hard_cost_has_only_five_terms() {
    let all_ones = Penalties {
        area: 1.0,
        outline: 1.0,
        wirelength: 1.0,
        guidance: 1.0,
        fence: 1.0,
        boundary: 1.0,
        soft_blockage: 1.0,
        fixed_macros: 1.0,
        notch: 1.0,
    };
    let w = SoftWeights::placement_defaults();
    let got = hard_norm_cost(&all_ones, &w, &Normalization::default());
    assert_eq!(got, w.area + w.outline + w.wirelength + w.guidance + w.fence);
}

/// ⚠️ The four terms it does not have are ignored rather than weighted at zero — changing them
/// changes nothing.
#[test]
fn the_four_soft_only_penalties_are_ignored() {
    let base = Penalties { area: 0.5, outline: 0.25, ..Default::default() };
    let loud = Penalties { boundary: 99.0, notch: 99.0, soft_blockage: 99.0, fixed_macros: 99.0, ..base };
    let w = SoftWeights::placement_defaults();
    let n = Normalization::default();
    assert_eq!(hard_norm_cost(&base, &w, &n), hard_norm_cost(&loud, &w, &n));
}

/// ⛔ **THREE thresholds for FOUR actions** — exchange is the `else` and takes everything left
/// over, including any slack the normalisation left behind.
#[test]
fn exchange_takes_everything_past_the_third_threshold() {
    let p = HardActionProbabilities {
        pos_swap: 0.25,
        neg_swap: 0.25,
        double_swap: 0.25,
        exchange: 0.25,
    };
    assert_eq!(p.action_for(0.0), Action::SwapPositive);
    assert_eq!(p.action_for(0.25), Action::SwapPositive, "inclusive at the threshold");
    assert_eq!(p.action_for(0.26), Action::SwapNegative);
    assert_eq!(p.action_for(0.5), Action::SwapNegative);
    assert_eq!(p.action_for(0.51), Action::SwapBoth);
    assert_eq!(p.action_for(0.75), Action::SwapBoth);
    assert_eq!(p.action_for(0.76), Action::Exchange);
    assert_eq!(p.action_for(1.0), Action::Exchange);
}

/// ⛔ **There is no resize.** A draw past every threshold is an exchange, not a fifth action —
/// which is where the soft core sends it.
#[test]
fn a_draw_past_every_threshold_is_an_exchange() {
    // Probabilities that deliberately do not reach 1.0.
    let p = HardActionProbabilities {
        pos_swap: 0.1,
        neg_swap: 0.1,
        double_swap: 0.1,
        exchange: 0.1,
    };
    assert_eq!(p.action_for(0.99), Action::Exchange, "the slack goes to exchange");
}

// ---------------------------------------------------------------- initialisation

/// ⚠️ **Not a clamp to something small — the factor becomes exactly `1.0`.** So a penalty that is
/// almost always zero reaches the cost undamped on the rare step where it is not.
#[test]
fn a_tiny_normalisation_factor_becomes_exactly_one() {
    assert_eq!(norm_floor(0.0), 1.0);
    assert_eq!(norm_floor(1e-5), 1.0);
    assert_eq!(norm_floor(1e-4), 1.0, "inclusive at the threshold");
    assert_eq!(norm_floor(1.1e-4), 1.1e-4, "just above it, and kept");
    assert_eq!(norm_floor(500.0), 500.0);
}

/// 🔑 **The temperature comes from the mean absolute step-to-step CHANGE, not the spread.** Two
/// runs with the same set of costs in a different order get different temperatures.
#[test]
fn the_temperature_measures_change_not_spread() {
    let smooth = init_temperature(&[0.0, 1.0, 2.0, 3.0], 0.9);
    let jagged = init_temperature(&[0.0, 3.0, 1.0, 2.0], 0.9);
    assert!(jagged > smooth, "same range, more movement: {jagged} vs {smooth}");

    // Smooth: deltas 1,1,1 -> mean 1. -1/ln(0.9).
    assert!((smooth - (-1.0 / 0.9f32.ln())).abs() < 1e-6);
}

/// ⚠️ **Fewer than two samples, or no change at all, gives exactly `1.0`** — and a sweep that
/// recorded nothing lands here.
#[test]
fn a_still_or_empty_sweep_gives_a_temperature_of_one() {
    assert_eq!(init_temperature(&[], 0.9), 1.0);
    assert_eq!(init_temperature(&[5.0], 0.9), 1.0);
    assert_eq!(init_temperature(&[2.0, 2.0, 2.0], 0.9), 1.0, "no change at all");
}

/// ⛔ **The hard core stores its sampled widths as `float`; the soft core stores them as `int`.**
/// Above 2^24 database units — 8.4 mm at 2000 units per micron, which a real die reaches — the
/// round trip is lossy, and the replayed width feeds the area penalty and so the temperature.
#[test]
fn a_hard_cores_sampled_width_makes_a_lossy_round_trip() {
    assert_eq!(hard_sampled_extent(16_777_215), 16_777_215, "below the limit, exact");
    assert_eq!(hard_sampled_extent(16_777_217), 16_777_216, "one unit lost");
    assert_eq!(hard_sampled_extent(20_000_001), 20_000_000, "10 mm at 2000 units per micron");
}

// ---------------------------------------------------------------- the hard-macro netlist

use vyges_mpl::placement::{
    build_bundled_nets_for_macros, hard_terminal_cluster_ids, BundledNet, UnmappedCluster,
};

/// 🔑 **Terminals are created in ascending CLUSTER-ID order, not connection order** — upstream
/// gathers the ids into a `std::set`, which both sorts and deduplicates. Iterating the connections
/// directly would give every terminal a different id.
#[test]
fn terminals_are_created_in_ascending_cluster_id_order() {
    let connected = [9, 3, 7, 3, 9];
    let got = hard_terminal_cluster_ids(&connected, &|_| false);
    assert_eq!(got, vec![3, 7, 9], "sorted, and deduplicated");
}

/// ⚠️ A cluster already among the macros being placed is not also a terminal.
#[test]
fn a_cluster_already_being_placed_is_not_a_terminal() {
    let connected = [1, 2, 3, 4];
    let got = hard_terminal_cluster_ids(&connected, &|id| id == 2 || id == 4);
    assert_eq!(got, vec![1, 3]);
}

/// ⛔ **Every connection is emitted TWICE, once from each end** — the `>` id filter that halves
/// them on the cluster path is simply absent here.
#[test]
fn every_connection_is_emitted_from_both_ends() {
    let clusters = [(1i32, vec![(2i32, 4.0f32)]), (2, vec![(1, 4.0)])];
    let macro_of = |id: i32| Some((id - 1) as usize);
    let got = build_bundled_nets_for_macros(&clusters, &macro_of).unwrap();
    assert_eq!(
        got,
        vec![
            BundledNet { source: 0, target: 1, weight: 4.0 },
            BundledNet { source: 1, target: 0, weight: 4.0 },
        ],
        "both directions, not one"
    );
}

/// ⛔ **No virtual connections at all**, unlike the cluster path which emits them first at weight
/// ten. Nothing here can produce one.
#[test]
fn there_are_no_virtual_connections() {
    let clusters = [(1i32, Vec::new())];
    let got = build_bundled_nets_for_macros(&clusters, &|id| Some(id as usize)).unwrap();
    assert!(got.is_empty(), "a cluster with no connections contributes nothing");
}

/// ⚠️ **A self-connection survives**, since nothing compares the two ids.
#[test]
fn a_self_connection_survives() {
    let clusters = [(1i32, vec![(1i32, 2.0f32)])];
    let got = build_bundled_nets_for_macros(&clusters, &|id| Some(id as usize)).unwrap();
    assert_eq!(got, vec![BundledNet { source: 1, target: 1, weight: 2.0 }]);
}

/// ⚠️ The emission order is cluster order, then connection order within each cluster.
#[test]
fn nets_follow_cluster_order_then_connection_order() {
    let clusters = [(0i32, vec![(1i32, 1.0f32), (2, 2.0)]), (1, vec![(0, 1.0)])];
    let got = build_bundled_nets_for_macros(&clusters, &|id| Some(id as usize)).unwrap();
    let weights: Vec<f32> = got.iter().map(|n| n.weight).collect();
    assert_eq!(weights, vec![1.0, 2.0, 1.0]);
}

/// ⛔ Upstream indexes with `std::map::at`, so a cluster missing from the macro map THROWS.
#[test]
fn a_cluster_missing_from_the_macro_map_is_an_error() {
    let clusters = [(1i32, vec![(99i32, 1.0f32)])];
    let got = build_bundled_nets_for_macros(&clusters, &|id| (id != 99).then_some(id as usize));
    assert_eq!(got, Err(UnmappedCluster(99)));
}
