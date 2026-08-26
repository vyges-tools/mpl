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
