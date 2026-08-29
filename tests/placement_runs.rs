// SPDX-License-Identifier: Apache-2.0
//! Choosing which utilization to place at, and moving the result into the die's coordinates.

use vyges_mpl::placement::{
    no_valid_solution_error, select_run, to_real_locations, utilization_list, NoValidSolution,
};

/// 🔑 **The reference's own numbers**, read off its `fine_shaping` debug channel at the pin for
/// `fixed_macros1` with the command's default `-target_util 0.25`.
///
/// ⚠️ **The last entry is `1.0000001`, not `1.0`.** That one digit is the whole point of matching
/// the arithmetic rather than the intent: the ramp is built from a root and a power, and it
/// overshoots. A cleaner implementation that ends on exactly `1.0` is a different program, and
/// these values size every macro in `applyUtilization`.
const REFERENCE: [f32; 10] = [
    0.25,
    0.29163226,
    0.3401975,
    0.3968503,
    0.46293738,
    0.54002994,
    0.6299606,
    0.73486733,
    0.8572441,
    1.0000001,
];

#[test]
fn the_utilization_list_matches_the_reference_bit_for_bit() {
    assert_eq!(utilization_list(0.25, 10), REFERENCE);
}

/// ⛔ **A SECOND reference capture, and it is the one that pins WHICH division is in `f32`.**
/// At `0.25` — and at `0.3`, and at most plausible targets — dividing `1.0 / target` in `f32` and
/// in `f64` give the same ten values, so neither reading can be ruled out. `0.32` separates them,
/// and the reference agrees with the `f32` division. Read off `fine_shaping` at the pin with
/// `-target_util 0.32`.
///
/// ℹ️ A brute-force sweep over three million targets in `[0.01, 0.999]` found **99,889** that
/// distinguish the two — about one in thirty. So the agreement at `0.25` is a coincidence of that
/// value, not a property of the arithmetic; four hand-picked targets all agreed and would have
/// "confirmed" either reading.
const REFERENCE_AT_0_32: [f32; 10] = [
    0.32,
    0.3631895,
    0.41220817,
    0.46784276,
    0.5309862,
    0.6026519,
    0.6839902,
    0.7763064,
    0.8810823,
    0.9999995,
];

#[test]
fn the_division_happens_in_f32_and_the_reference_says_so() {
    assert_eq!(utilization_list(0.32, 10), REFERENCE_AT_0_32);

    // The same ramp with the division widened to `f64` — what the natural reading produces.
    let widened: Vec<f32> = {
        let base = 1.0f64 / 0.32f32 as f64;
        let ratio = base.powf(1.0 / 9.0) as f32;
        (0..10).map(|i| (0.32f64 * (ratio as f64).powf(i as f64)) as f32).collect()
    };
    assert_ne!(widened, REFERENCE_AT_0_32.to_vec(), "the fixture separates the two readings");
}

/// ⚠️ **The division is in `f32` and the power in `f64`**, and the ramp at the default target must
/// match the reference bit for bit.
///
/// ⛔ **This test used to assert the NEGATIVE — that an all-`f32` ramp gives DIFFERENT digits — and
/// that claim is FALSE on x86-64.** CI caught it on the first public run: at target `0.25` the
/// all-`f32` ramp produces exactly the reference values on a GitHub runner, while differing on
/// macOS/ARM and on the correlation box. The distinction is real at other targets — see
/// `the_division_happens_in_f32_and_the_reference_says_so`, which separates them at `0.32` — but
/// it is NOT observable at `0.25`.
///
/// 🔑 **A negative assertion about floating-point is a claim about the PLATFORM, not about the
/// transcription.** What actually guards the rule is the POSITIVE: our list equals the reference's.
/// That holds everywhere, and it is what this test now asserts.
#[test]
fn the_ramp_at_the_default_target_matches_the_reference() {
    assert_eq!(utilization_list(0.25, 10), REFERENCE);
}

/// ⚠️ The ramp starts at the target the user asked for and climbs.
#[test]
fn the_ramp_starts_at_the_target_and_climbs_to_one() {
    let list = utilization_list(0.5, 10);
    assert_eq!(list[0], 0.5);
    assert!(list.windows(2).all(|w| w[1] > w[0]), "monotonic");
    assert!((list[9] - 1.0).abs() < 1e-5, "it lands on 1.0, give or take the overshoot");
}

/// ⛔ **A target of zero produces `[0.0, NaN, NaN, ...]`**, and upstream does not guard it. The
/// ratio is infinite; the first entry is `0 * inf^0`, and `inf^0` is `1` by IEEE, so it comes out
/// as a clean zero — every later entry is `0 * inf`, which is NaN. A ramp that begins plausibly
/// and then goes undefined is worse than one that is undefined throughout, which is why this is
/// written down rather than left to be rediscovered.
#[test]
fn a_zero_target_gives_a_zero_then_nothing_but_nans() {
    let list = utilization_list(0.0, 10);
    assert_eq!(list[0], 0.0, "inf to the power zero is one");
    assert!(list[1..].iter().all(|v| v.is_nan()), "{list:?}");
}

// ---------------------------------------------------------------- choosing a run

/// 🔑 **The first valid run wins, in index order.** Later runs are tried only because earlier ones
/// failed, never because they might be better.
#[test]
fn the_first_valid_run_wins() {
    let list = utilization_list(0.25, 10);
    let mut attempted = Vec::new();
    let got = select_run(
        &list,
        10,
        &mut |_| true,
        &mut |i, _| {
            attempted.push(i);
            i >= 2
        },
    )
    .expect("run 2 was valid");
    assert_eq!(got.index, 2);
    assert_eq!(got.utilization, list[2]);
    assert!(got.utilization_was_adjusted, "not the utilization that was asked for");
    assert_eq!(attempted, (0..10).collect::<Vec<_>>(), "the whole batch anneals before it is read");
}

/// ⚠️ **The first run is not an adjustment**, so it raises no MPL-55.
#[test]
fn the_asked_for_utilization_is_not_an_adjustment() {
    let list = utilization_list(0.25, 10);
    let got = select_run(&list, 10, &mut |_| true, &mut |_, _| true).expect("run 0");
    assert_eq!(got.index, 0);
    assert!(!got.utilization_was_adjusted);
}

/// 🔑 **The winner does not depend on the thread count** — only the amount of wasted work does.
/// That is what makes running the batch sequentially a transcription rather than an approximation.
#[test]
fn the_winner_is_the_same_at_every_thread_count() {
    let list = utilization_list(0.25, 10);
    for threads in [1usize, 2, 3, 7, 10, 64] {
        let got = select_run(&list, threads, &mut |_| true, &mut |i, _| i >= 4)
            .expect("run 4 was valid");
        assert_eq!(got.index, 4, "at {threads} threads");
    }
}

/// ⚠️ **One thread stops as soon as it finds a winner**; ten anneal the whole batch first. Same
/// winner, different work — and the work is what a thread count is for.
#[test]
fn a_single_thread_does_less_work_for_the_same_answer() {
    let list = utilization_list(0.25, 10);
    let mut solo = Vec::new();
    select_run(&list, 1, &mut |_| true, &mut |i, _| {
        solo.push(i);
        i >= 1
    })
    .unwrap();
    assert_eq!(solo, vec![0, 1], "it stopped as soon as run 1 succeeded");

    let mut batched = Vec::new();
    select_run(&list, 10, &mut |_| true, &mut |i, _| {
        batched.push(i);
        i >= 1
    })
    .unwrap();
    assert_eq!(batched.len(), 10, "all ten annealed before any was read");
}

/// ⛔ **A run skipped for an invalid utilization still CONSUMES its slot.** `run_id` advances
/// before the test and the batch size is subtracted whether or not an annealer was built, so a
/// design with unusable utilizations gets fewer attempts, not the same ten shifted along.
#[test]
fn a_skipped_utilization_still_spends_its_slot() {
    let list = utilization_list(0.25, 10);
    // One thread, so each slot is its own batch: runs 0..2 are unusable and never anneal.
    let mut attempted = Vec::new();
    let got = select_run(
        &list,
        1,
        &mut |u| u > list[2],
        &mut |i, _| {
            attempted.push(i);
            true
        },
    )
    .expect("run 3 was the first usable one");
    assert_eq!(got.index, 3);
    assert_eq!(attempted, vec![3], "the first three were never annealed at all");
}

/// ⚠️ With every utilization unusable, nothing anneals and the loop still terminates.
#[test]
fn every_utilization_unusable_is_a_refusal_not_a_hang() {
    let list = utilization_list(0.25, 10);
    let mut attempted = 0;
    let got = select_run(
        &list,
        3,
        &mut |_| false,
        &mut |_, _| {
            attempted += 1;
            true
        },
    );
    assert_eq!(got, Err(NoValidSolution));
    assert_eq!(attempted, 0, "nothing was annealed");
}

/// ⚠️ Every run annealing to an invalid solution is the failure MPL-40 and MPL-8 report.
#[test]
fn no_valid_solution_anywhere_is_a_refusal() {
    let list = utilization_list(0.25, 10);
    let mut attempted = 0;
    let got = select_run(
        &list,
        4,
        &mut |_| true,
        &mut |_, _| {
            attempted += 1;
            false
        },
    );
    assert_eq!(got, Err(NoValidSolution));
    assert_eq!(attempted, 10, "all ten were tried");
}

/// ⚠️ **Two different codes for the same condition**: at the root the user can act on it, below it
/// upstream is asking for a bug report.
#[test]
fn the_root_and_a_child_fail_with_different_codes() {
    let root = no_valid_solution_error(true, 0, "root");
    assert_eq!(root.code, 40);
    assert!(root.message.contains("Core utilization"), "{}", root.message);

    let child = no_valid_solution_error(false, 7, "MACRO_3");
    assert_eq!(child.code, 8);
    assert!(child.message.contains("report this internal error"), "{}", child.message);
    assert!(child.message.contains("(7): MACRO_3"), "it names the cluster: {}", child.message);
}

// ---------------------------------------------------------------- into the die's coordinates

/// ⚠️ Every child is shifted by the parent's origin.
#[test]
fn children_are_moved_by_the_parents_origin() {
    let mut children = [(0, 0), (100, 200), (-50, 25)];
    to_real_locations(&mut children, (1000, 2000));
    assert_eq!(children, [(1000, 2000), (1100, 2200), (950, 2025)]);
}

/// ⛔ **The offsets are `float` upstream while the coordinates are `int`**, so a coordinate above
/// 2^24 cannot survive the round trip exactly. 2^24 is 8.4 mm at 2000 units per micron, which a
/// real die reaches — this is not a theoretical limit.
#[test]
fn a_large_coordinate_loses_precision_in_the_round_trip() {
    // 2^24 = 16_777_216. One more than that is not representable as an f32.
    let mut children = [(16_777_217, 0)];
    to_real_locations(&mut children, (0, 0));
    assert_eq!(children[0].0, 16_777_216, "the odd unit was lost, exactly as upstream loses it");

    // Below the limit nothing is lost.
    let mut small = [(16_777_215, 0)];
    to_real_locations(&mut small, (0, 0));
    assert_eq!(small[0].0, 16_777_215);
}
