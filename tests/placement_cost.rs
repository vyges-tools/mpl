// SPDX-License-Identifier: Apache-2.0
//! Wiring the six placement-only cost terms into the annealer's `calPenalty`.

use vyges_mpl::anneal::{
    init_sequence_pair, ActionProbabilities, Normalization, Penalties, Search, SoftMacro,
    SoftWeights,
};
use vyges_mpl::placement::{AreaKind, MacroAttributes, PlacementInputs, Root};

fn macro_cluster(x: i32, y: i32, w: i32, h: i32) -> SoftMacro {
    SoftMacro {
        x,
        y,
        width: w,
        height: h,
        fixed: false,
        area: w as i64 * h as i64,
        is_macro_cluster: true,
    }
}

fn attributes(num_macro: i32) -> MacroAttributes {
    MacroAttributes {
        kind: Some(AreaKind::HardMacroCluster),
        num_macro,
        cluster_macro_area: 1000,
        cluster_area: 1000,
        ..Default::default()
    }
}

/// A search over the given macros, with no placement context.
fn search(macros: Vec<SoftMacro>, outline: (i32, i32)) -> Search {
    let n = macros.len();
    let mut s = Search {
        curves: Vec::new(),
        macros,
        sp: init_sequence_pair(n),
        width: 0,
        height: 0,
        penalties: Penalties::default(),
        placement: None,
        outline_width: outline.0,
        outline_height: outline.1,
        dbu_per_micron: 10,
        fixed_bboxes: Vec::new(),
        weights: SoftWeights::placement_defaults(),
        normalization: Normalization::default(),
        probabilities: ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
        action: None,
        hard_probabilities: None,
        cost_history: Vec::new(),
    };
    // The macros are already where the fixture put them; record the packing they imply.
    s.width = s.macros.iter().map(|m| m.x + m.width).max().unwrap_or(0);
    s.height = s.macros.iter().map(|m| m.y + m.height).max().unwrap_or(0);
    s
}

fn inputs(count: usize, weights: SoftWeights) -> PlacementInputs {
    PlacementInputs {
        attributes: vec![attributes(1); count],
        root: Root { x: 0, y: 0, width: 1000, height: 1000 },
        weights,
        ..Default::default()
    }
}

/// ℹ️ **Without a placement context the six extra terms are never computed**, which is exactly
/// what a tiling run needs — and why the shaping path pays nothing for their existence.
#[test]
fn a_search_with_no_placement_context_scores_only_the_shaping_terms() {
    let mut s = search(vec![macro_cluster(0, 0, 400, 400)], (1000, 1000));
    s.cal_penalty();
    assert_eq!(s.penalties.wirelength, 0.0);
    assert_eq!(s.penalties.guidance, 0.0);
    assert_eq!(s.penalties.fence, 0.0);
    assert_eq!(s.penalties.boundary, 0.0);
    assert_eq!(s.penalties.soft_blockage, 0.0);
    assert_eq!(s.penalties.notch, 0.0);
}

/// ⚠️ **Each term lands in its own field.** Lighting one weight at a time is the only way to be
/// sure two of them are not crossed.
#[test]
fn each_placement_term_lands_in_its_own_field() {
    // A macro cluster in the middle of the die: the boundary term is the one with something to say.
    let mut s = search(vec![macro_cluster(450, 450, 100, 100)], (1000, 1000));
    let mut only_boundary = SoftWeights::default();
    only_boundary.boundary = 50.0;
    s.placement = Some(Box::new(inputs(1, only_boundary)));
    s.cal_penalty();
    assert!(s.penalties.boundary > 0.0, "the boundary term fired");
    assert_eq!(s.penalties.notch, 0.0, "and nothing else did");
    assert_eq!(s.penalties.soft_blockage, 0.0);

    // A soft blockage under the same macro: now only that term fires.
    let mut s = search(vec![macro_cluster(450, 450, 100, 100)], (1000, 1000));
    let mut only_blockage = SoftWeights::default();
    only_blockage.soft_blockage = 10.0;
    let mut ins = inputs(1, only_blockage);
    ins.soft_blockages = vec![(400, 400, 600, 600)];
    s.placement = Some(Box::new(ins));
    s.cal_penalty();
    assert!(s.penalties.soft_blockage > 0.0);
    assert_eq!(s.penalties.boundary, 0.0);

    // A guide the macro sits far outside of.
    let mut s = search(vec![macro_cluster(900, 900, 100, 100)], (1000, 1000));
    let mut only_guidance = SoftWeights::default();
    only_guidance.guidance = 10.0;
    let mut ins = inputs(1, only_guidance);
    ins.guides = vec![(0, (0, 0, 100, 100))];
    s.placement = Some(Box::new(ins));
    s.cal_penalty();
    assert!(s.penalties.guidance > 0.0);
    assert_eq!(s.penalties.fence, 0.0);

    // A fence the macro sits far outside of.
    let mut s = search(vec![macro_cluster(900, 900, 100, 100)], (1000, 1000));
    let mut only_fence = SoftWeights::default();
    only_fence.fence = 10.0;
    let mut ins = inputs(1, only_fence);
    ins.fences = vec![(0, (0, 0, 400, 400))];
    s.placement = Some(Box::new(ins));
    s.cal_penalty();
    assert!(s.penalties.fence > 0.0);
    assert_eq!(s.penalties.guidance, 0.0);
}

/// 🔑 A notch between two macro clusters is scored through the wiring, not just in isolation.
#[test]
fn the_notch_term_is_scored_through_the_wiring() {
    let macros = vec![macro_cluster(0, 0, 1000, 400), macro_cluster(0, 450, 1000, 550)];
    let mut s = search(macros, (1000, 1000));
    let mut only_notch = SoftWeights::default();
    only_notch.notch = 50.0;
    s.placement = Some(Box::new(inputs(2, only_notch)));
    s.cal_penalty();
    assert!(s.penalties.notch > 0.0, "the 50-unit gap between them");
}

/// ⛔ **The notch term judges validity against the PREVIOUS perturbation's fixed-macro penalty.**
/// `calNotchPenalty` asks `isValid()`, and `calFixedMacrosPenalty` runs after it — so scoring the
/// very same geometry twice gives two different notch penalties. Computing the fixed-macro term
/// first is the obvious tidy-up and is a different program.
#[test]
fn the_notch_term_sees_a_stale_fixed_macro_penalty() {
    let macros = vec![macro_cluster(0, 0, 1000, 400), macro_cluster(0, 450, 1000, 550)];
    let mut s = search(macros, (1000, 1000));
    // A fixed macro that the lower cluster overlaps, so the fixed-macro penalty will be positive.
    s.fixed_bboxes = vec![(0, 0, 100, 100)];
    let mut only_notch = SoftWeights::default();
    only_notch.notch = 50.0;
    s.placement = Some(Box::new(inputs(2, only_notch)));

    assert_eq!(s.penalties.fixed_macros, 0.0, "nothing has been scored yet");
    s.cal_penalty();
    let first = s.penalties.notch;
    assert!(s.penalties.fixed_macros > 0.0, "and now the overlap is known");
    assert!(first > 0.0);
    assert!(first < 1.0, "the gap between the clusters, not the whole outline");

    // Identical geometry, scored again — and the answer changes, because the validity test now
    // sees the penalty the first pass left behind.
    s.cal_penalty();
    assert_eq!(s.penalties.notch, 1.0, "the whole floorplan is now treated as one huge notch");
    assert_ne!(s.penalties.notch, first, "the same geometry scored two different ways");
}

/// ⚠️ **A macro with no cluster behind it obstructs nothing** — the wiring maps a missing kind
/// onto the fixed-macro case, which is upstream's null cluster pointer.
#[test]
fn a_macro_with_no_cluster_obstructs_nothing() {
    let macros = vec![macro_cluster(0, 0, 1000, 400), macro_cluster(0, 450, 1000, 550)];
    let mut s = search(macros, (1000, 1000));
    let mut only_notch = SoftWeights::default();
    only_notch.notch = 50.0;
    let mut ins = inputs(2, only_notch);
    for a in &mut ins.attributes {
        a.kind = None;
    }
    s.placement = Some(Box::new(ins));
    s.cal_penalty();
    assert_eq!(s.penalties.notch, 0.0, "no obstruction, so no notch");
}

/// ⚠️ The context survives a scoring — `cal_penalty` takes it out and must put it back, or every
/// term after the first call would silently go dark.
#[test]
fn the_placement_context_survives_being_scored() {
    let mut s = search(vec![macro_cluster(450, 450, 100, 100)], (1000, 1000));
    let mut only_boundary = SoftWeights::default();
    only_boundary.boundary = 50.0;
    s.placement = Some(Box::new(inputs(1, only_boundary)));
    s.cal_penalty();
    let first = s.penalties.boundary;
    s.cal_penalty();
    assert!(s.placement.is_some(), "still there");
    assert_eq!(s.penalties.boundary, first, "and still scoring");
}

/// ⛔ **The hard core computes FOUR penalties and not the fixed-macro one.**
/// `SACoreHardMacro::calPenalty` calls outline, wirelength, guidance and fence — that is the whole
/// list. The soft core's four extra terms are its own members and are never touched here, and
/// neither is `calFixedMacrosPenalty`, which the soft core calls unconditionally.
///
/// ⚠️ So a hard run leaves all five at the zero they were constructed with. Computing them anyway
/// would be harmless-looking and would change `norm_cost`, since the cost divides by factors that
/// were measured over those same samples.
#[test]
fn a_hard_macro_run_does_not_compute_the_soft_penalties() {
    let macros = vec![macro_cluster(0, 0, 400, 400), macro_cluster(0, 450, 400, 400)];
    let mut s = search(macros, (1000, 1000));
    let mut w = SoftWeights::placement_defaults();
    w.notch = 50.0;
    w.boundary = 50.0;
    let mut ins = inputs(2, w);
    ins.weights = w;
    s.placement = Some(Box::new(ins));
    s.weights = w;
    // ⚠️ A fixed macro present, so the soft core WOULD score a fixed-macro penalty.
    s.fixed_bboxes = vec![(0, 0, 400, 400)];

    s.hard_probabilities = Some(vyges_mpl::placement::HardActionProbabilities {
        pos_swap: 0.25,
        neg_swap: 0.25,
        double_swap: 0.25,
        exchange: 0.25,
    });
    s.cal_penalty();

    assert_eq!(s.penalties.boundary, 0.0, "not a member of the hard core");
    assert_eq!(s.penalties.notch, 0.0, "nor is this");
    assert_eq!(s.penalties.soft_blockage, 0.0);
    assert_eq!(s.penalties.fixed_macros, 0.0, "calPenalty does not call it here");

    // ⚠️ The control: the SAME state as a soft run scores them, or the assertions above would
    // hold for a fixture that simply has nothing to score.
    s.hard_probabilities = None;
    s.cal_penalty();
    assert_ne!(s.penalties.fixed_macros, 0.0, "the soft core does compute it");
}
