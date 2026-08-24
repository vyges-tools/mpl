// SPDX-License-Identifier: Apache-2.0
//! The pipeline is the spec, executable. These tests pin that claim.
//!
//! ⚠️ None of these need a database. `Plan::sequence` is pure, and a verdict about stage ORDER is
//! exactly the kind of thing that should be testable with values.
use vyges_mpl::pipeline::{Plan, StageId, ORDER};

/// ⚠️ **This is the drift gate.** Upstream `HierRTLMP::run` calls exactly these ten stages in
/// exactly this order. Transcribed here by hand, deliberately: a test that reads the same
/// constant it is checking proves nothing.
const UPSTREAM_RUN_ORDER: &[(u8, &str)] = &[
    (1, "runMultilevelAutoclustering"),
    (2, "commitClusteringDataToDb"),
    (3, "resetSAParameters"),
    (4, "runCoarseShaping"),
    (5, "runHierarchicalMacroPlacement"),
    (6, "pushMacrosToCoreBoundaries"),
    (7, "updateMacrosOnDb"),
    (8, "generateTemporaryStdCellsPlacement"),
    (9, "commitMacroPlacementToDb"),
    (10, "computeWireLength"),
];

#[test]
fn the_pipeline_matches_the_spec_table() {
    assert_eq!(ORDER.len(), UPSTREAM_RUN_ORDER.len(), "stage count");
    for (stage, (num, name)) in ORDER.iter().zip(UPSTREAM_RUN_ORDER) {
        assert_eq!(*stage as u8, *num, "{stage} is stage {num} of upstream run()");
        assert_eq!(stage.upstream_name(), *name, "upstream name for stage {num}");
    }
}

#[test]
fn every_stage_appears_exactly_once() {
    let mut seen: Vec<StageId> = ORDER.to_vec();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), ORDER.len(), "no stage is listed twice");
}

#[test]
fn the_default_plan_is_upstreams_order() {
    assert_eq!(Plan::default().sequence(), ORDER.to_vec());
}

#[test]
fn stop_after_truncates_for_the_per_stage_oracle() {
    let plan = Plan { stop_after: Some(StageId::CoarseShaping), ..Default::default() };
    let seq = plan.sequence();
    assert_eq!(*seq.last().unwrap(), StageId::CoarseShaping);
    assert_eq!(seq.len(), 4, "stages 1..=4");
    assert!(!seq.contains(&StageId::CommitMacroPlacement));
}

#[test]
fn only_keeps_upstreams_relative_order_not_the_order_asked_for() {
    // Passing them backwards must not reorder them: `only` selects, `swap` reorders.
    let plan = Plan {
        only: Some(vec![StageId::CommitMacroPlacement, StageId::CoarseShaping]),
        ..Default::default()
    };
    assert_eq!(
        plan.sequence(),
        vec![StageId::CoarseShaping, StageId::CommitMacroPlacement]
    );
}

#[test]
fn swap_exchanges_two_stages_in_place() {
    let plan = Plan {
        swap: Some((StageId::CoarseShaping, StageId::HierarchicalMacroPlacement)),
        ..Default::default()
    };
    let seq = plan.sequence();
    assert_eq!(seq[3], StageId::HierarchicalMacroPlacement, "4th slot now holds the 5th stage");
    assert_eq!(seq[4], StageId::CoarseShaping);
    assert_eq!(seq.len(), ORDER.len(), "swapping does not add or drop a stage");
}

#[test]
fn repeat_duplicates_in_place_and_composes_with_stop_after() {
    let plan = Plan {
        repeat: Some((StageId::CoarseShaping, 3)),
        stop_after: Some(StageId::CoarseShaping),
        ..Default::default()
    };
    let seq = plan.sequence();
    // ⚠️ `stop_after` truncates after the LAST occurrence, or repeat+stop would cancel out and
    // an idempotence probe would silently run the stage once.
    assert_eq!(
        seq.iter().filter(|&&s| s == StageId::CoarseShaping).count(),
        3,
        "all three repetitions survive the truncation"
    );
    assert_eq!(*seq.last().unwrap(), StageId::CoarseShaping);
}

#[test]
fn an_unknown_stage_in_a_knob_is_ignored_rather_than_panicking() {
    // `only` naming a stage twice, and `swap` naming one not in `only`, must not panic —
    // a plan comes from a CLI flag and a bad plan should be a no-op, not a crash.
    let plan = Plan {
        only: Some(vec![StageId::CoarseShaping]),
        swap: Some((StageId::CoarseShaping, StageId::ComputeWireLength)),
        ..Default::default()
    };
    assert_eq!(plan.sequence(), vec![StageId::CoarseShaping]);
}
