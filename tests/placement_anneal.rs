// SPDX-License-Identifier: Apache-2.0
//! One parent, one annealing run — the composition the driver's callback makes.

use vyges_mpl::anneal::{
    ActionProbabilities, Normalization, Penalties, SaParameters, Search, ShapeCurve, SoftMacro,
    SoftWeights,
};
use vyges_mpl::placement::{
    anneal_one_run, AreaKind, Enhancements, ParentProblem, PlacementInputs, ReshapeInput, Root,
};

fn cell_cluster(area: i64, cells: i32) -> ReshapeInput {
    ReshapeInput {
        kind: Some(AreaKind::StdCellCluster),
        cluster_area: area,
        cluster_std_cell_area: area,
        num_std_cell: cells,
        tilings: Vec::new(),
    }
}

fn macro_cluster(w: i32, h: i32) -> ReshapeInput {
    ReshapeInput {
        kind: Some(AreaKind::HardMacroCluster),
        cluster_area: w as i64 * h as i64,
        cluster_std_cell_area: 0,
        num_std_cell: 0,
        tilings: vec![(w, h)],
    }
}

fn problem() -> ParentProblem {
    let macros = vec![
        SoftMacro { width: 200, height: 200, area: 40_000, is_macro_cluster: true, ..Default::default() },
        SoftMacro { width: 300, height: 300, area: 90_000, ..Default::default() },
    ];
    ParentProblem {
        macros,
        reshape: vec![macro_cluster(200, 200), cell_cluster(90_000, 5_000)],
        number_of_sequence_pair_macros: 2,
        inputs: PlacementInputs {
            attributes: vec![Default::default(); 2],
            root: Root { x: 0, y: 0, width: 2000, height: 2000 },
            weights: SoftWeights::placement_defaults(),
            ..Default::default()
        },
        outline: (2000, 2000),
        dbu_per_micron: 2000,
        fixed_bboxes: Vec::new(),
        tiny_threshold: 0,
        min_ar: 0.33,
        force_centralization: false,
    }
}

fn params() -> SaParameters {
    // ⚠️ A short search — these cases exercise the composition, not the annealer.
    SaParameters { max_num_step: 5, num_perturb_per_step: 8, ..Default::default() }
}

/// 🔑 A parent whose macros fit its outline anneals to a valid solution.
#[test]
fn a_parent_that_fits_anneals_to_a_valid_solution() {
    let got = anneal_one_run(
        &problem(),
        0.25,
        1234,
        &params(),
        ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
        SoftWeights::placement_defaults(),
    );
    assert!(got.is_some(), "a 2000 x 2000 outline holds these");
    assert_eq!(got.unwrap().len(), 2);
}

/// 🔑 **The enhancements are part of the RUN, not a post-process.** Centralization shifts the whole
/// floorplan into the middle of its outline, so a packing that fits with slack does not stay in the
/// corner the packer put it in.
///
/// ⚠️ Weighted so centralization cannot cost more — with only the area term live the cost does not
/// change under a translation, and an equal cost KEEPS the move. A fixture under the real placement
/// weights would sometimes revert and prove nothing.
#[test]
fn the_enhancements_move_the_floorplan_off_the_corner() {
    let only_area = SoftWeights {
        area: 1.0,
        outline: 0.0,
        wirelength: 0.0,
        guidance: 0.0,
        fence: 0.0,
        boundary: 0.0,
        soft_blockage: 0.0,
        fixed_macros: 0.0,
        notch: 0.0,
    };
    let got = anneal_one_run(
        &problem(),
        0.25,
        3,
        &params(),
        ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
        only_area,
    )
    .expect("it fits");
    assert!(
        got.iter().all(|m| m.x != 0 && m.y != 0),
        "the packer leaves something at the origin; centralization moves it: {got:?}"
    );
}

/// ⚠️ **The same seed gives the same answer**, which is what makes a run reproducible at all.
#[test]
fn the_same_seed_gives_the_same_placement() {
    let p = params();
    let probs = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    let a = anneal_one_run(&problem(), 0.25, 99, &p, probs, SoftWeights::placement_defaults());
    let b = anneal_one_run(&problem(), 0.25, 99, &p, probs, SoftWeights::placement_defaults());
    assert_eq!(a, b);
}

/// ⛔ **Each macro-placement run uses a DIFFERENT seed**, so two runs of the same problem explore
/// differently — unlike the tiling search, where every run shares one.
#[test]
fn a_different_seed_explores_differently() {
    let p = SaParameters { max_num_step: 40, num_perturb_per_step: 40, ..Default::default() };
    let probs = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    let a = anneal_one_run(&problem(), 0.25, 1, &p, probs, SoftWeights::placement_defaults());
    let b = anneal_one_run(&problem(), 0.25, 2, &p, probs, SoftWeights::placement_defaults());
    assert!(a.is_some() && b.is_some());
    assert_ne!(a, b, "two seeds, two trajectories");
}

/// ⛔ **A parent whose macros cannot fit reports INVALID** rather than returning a bad placement —
/// which is what makes the caller try the next utilization.
#[test]
fn a_parent_that_cannot_fit_reports_invalid() {
    let mut p = problem();
    p.outline = (100, 100);
    p.inputs.root = Root { x: 0, y: 0, width: 100, height: 100 };
    let got = anneal_one_run(
        &p,
        0.25,
        7,
        &params(),
        ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
        SoftWeights::placement_defaults(),
    );
    assert!(got.is_none(), "a 100 x 100 outline cannot hold a 200 x 200 macro");
}

/// ⚠️ **A lower utilization inflates the cell cluster**, so the same outline can stop holding it.
#[test]
fn the_utilization_changes_what_fits() {
    let mut p = problem();
    p.outline = (700, 700);
    p.inputs.root = Root { x: 0, y: 0, width: 700, height: 700 };
    let probs = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);

    let tight = anneal_one_run(&p, 1.0, 5, &params(), probs, SoftWeights::placement_defaults());
    let loose = anneal_one_run(&p, 0.05, 5, &params(), probs, SoftWeights::placement_defaults());
    assert!(tight.is_some(), "at full utilization it fits");
    assert!(loose.is_none(), "inflated twentyfold it does not");
}

/// ⛔ **The reshaped SHAPE CURVE is what a resize moves along.** Without it the annealer can change
/// a cluster's position but never its shape, and a fixture that only checks what FITS cannot see
/// that — a mutation proved it. This one makes resize the only move.
#[test]
fn a_resize_moves_along_the_reshaped_curve() {
    let p = problem();
    // The shape `apply_utilization` gives the cell cluster, before any annealing.
    let reshaped = vyges_mpl::placement::apply_utilization(&p.reshape, 0, false, 0.25, 0.33);
    let cell = reshaped.iter().find(|r| r.id == 1).expect("the cell cluster was reshaped");
    let (_, start_width, _, _) =
        vyges_mpl::anneal::shape_curve_from_intervals(&cell.intervals, cell.area)
            .expect("a curve");

    // Resize only, and long enough to take one.
    let resize_only = ActionProbabilities::normalized(0.0, 0.0, 0.0, 0.0, 1.0);
    let long = SaParameters { max_num_step: 60, num_perturb_per_step: 20, ..Default::default() };
    let got = anneal_one_run(&p, 0.25, 11, &long, resize_only, SoftWeights::placement_defaults())
        .expect("it fits");

    assert_ne!(got[1].width, start_width, "the cluster was never reshaped");
    assert!(
        got[1].width >= cell.intervals[0].min && got[1].width <= cell.intervals[0].max,
        "and it stayed on the curve: {} not in {:?}",
        got[1].width,
        cell.intervals[0]
    );
}

// ---------------------------------------------------------------- the enhancements seam

/// ⛔ **`notch_thresholds` reports what `calNotchPenalty` LEFT BEHIND**, not what a constructor was
/// given. With the notch term live it overwrote both from the outline — crossed, so `h` comes from
/// the HEIGHT. With the term dark it never ran and the constructor's `10` units stand.
#[test]
fn the_alignment_thresholds_come_from_the_notch_pass() {
    let base = Search {
        macros: Vec::new(),
        curves: Vec::new(),
        sp: Default::default(),
        width: 0,
        height: 0,
        penalties: Penalties::default(),
        placement: None,
        outline_width: 2000,
        outline_height: 500,
        dbu_per_micron: 2000,
        fixed_bboxes: Vec::new(),
        weights: SoftWeights::placement_defaults(),
        normalization: Normalization::default(),
        probabilities: ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
        action: None,
    };
    assert_eq!(
        Enhancements::notch_thresholds(&base),
        (50, 200),
        "h from the HEIGHT, v from the WIDTH"
    );

    let dark = Search { weights: SoftWeights { notch: 0.0, ..base.weights }, ..base.clone() };
    assert_eq!(
        Enhancements::notch_thresholds(&dark),
        (10, 10),
        "the constructor's values, which no command overrides"
    );
}

/// ⚠️ The seam reports the sequence pair's order, not every macro — the terminals appended after it
/// are outside what the enhancements may move.
#[test]
fn the_seam_reports_only_the_sequence_pair() {
    let mut s = Search {
        macros: vec![SoftMacro::default(); 5],
        curves: vec![ShapeCurve::default(); 5],
        sp: vyges_mpl::anneal::init_sequence_pair(3),
        width: 0,
        height: 0,
        penalties: Penalties::default(),
        placement: None,
        outline_width: 100,
        outline_height: 100,
        dbu_per_micron: 10,
        fixed_bboxes: Vec::new(),
        weights: SoftWeights::default(),
        normalization: Normalization::default(),
        probabilities: ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
        action: None,
    };
    assert_eq!(Enhancements::order(&s).len(), 3, "not 5");
    assert_eq!(Enhancements::macros(&s).len(), 5, "but all five are there to be indexed");
    let _ = Enhancements::macros_mut(&mut s);

    // ⚠️ **It is the POSITIVE sequence.** The two start identical, so a fixture built from a fresh
    // sequence pair cannot tell them apart — a mutation swapping them proved that. Perturb one.
    s.sp.pos = vec![2, 0, 1];
    s.sp.neg = vec![0, 1, 2];
    assert_eq!(Enhancements::order(&s), &[2, 0, 1], "the positive one");
}
