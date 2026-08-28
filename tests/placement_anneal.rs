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
        names: Vec::new(),
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
    assert_eq!(got.unwrap().macros.len(), 2);
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
        got.macros.iter().all(|m| m.x != 0 && m.y != 0),
        "the packer leaves something at the origin; centralization moves it: {:?}", got.macros
    );
}

/// ⚠️ **The same seed gives the same answer**, which is what makes a run reproducible at all.
#[test]
fn the_same_seed_gives_the_same_placement() {
    let p = params();
    let probs = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    let a = anneal_one_run(&problem(), 0.25, 99, &p, probs, SoftWeights::placement_defaults());
    let b = anneal_one_run(&problem(), 0.25, 99, &p, probs, SoftWeights::placement_defaults());
    assert_eq!(a.map(|s| s.macros), b.map(|s| s.macros));
}

/// ⛔ **Each macro-placement run uses a DIFFERENT seed**, so two runs of the same problem explore
/// differently — unlike the tiling search, where every run shares one.
#[test]
fn a_different_seed_explores_differently() {
    // ⚠️ A SHORT search on purpose. `initialize` and `fast_sa` now run exactly the count they are
    // handed, so a long one saturates this two-macro problem and both seeds land on the same
    // optimum — which would make the assertion below fail for a reason that has nothing to do
    // with seeding. Four is what this test effectively ran when the core derived its own count.
    let p = SaParameters { max_num_step: 40, num_perturb_per_step: 4, ..Default::default() };
    let probs = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    let a = anneal_one_run(&problem(), 0.25, 1, &p, probs, SoftWeights::placement_defaults());
    let b = anneal_one_run(&problem(), 0.25, 2, &p, probs, SoftWeights::placement_defaults());
    assert!(a.is_some() && b.is_some());
    assert_ne!(a.map(|s| s.macros), b.map(|s| s.macros), "two seeds, two trajectories");
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

    assert_ne!(got.macros[1].width, start_width, "the cluster was never reshaped");
    assert!(
        got.macros[1].width >= cell.intervals[0].min && got.macros[1].width <= cell.intervals[0].max,
        "and it stayed on the curve: {} not in {:?}",
        got.macros[1].width,
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
        cost_history: Vec::new(),
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
        cost_history: Vec::new(),
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

/// ⛔ **Cluster placement's perturbation count is `max(macros, num_perturb_per_step)`** — the full
/// configured count as a floor, not coarse shaping's tenth. Upstream applies it at the call site.
///
/// 🔑 **Two assertions that can only both hold if the rule is applied.** With EIGHT macros, asking
/// for one perturbation and asking for eight must give the SAME placement (the rule floors both to
/// eight), while asking for five hundred must give a different one (the rule leaves it at five
/// hundred). Drop the adjustment and the first pair runs one against eight and diverges.
///
/// ⚠️ A two-macro problem cannot express this — one perturbation and two settle identically, so
/// the equivalence holds whether or not the rule is applied. The fixture has to be big enough for
/// the count to change the answer, and the second assertion is what proves it is.
#[test]
fn cluster_placement_perturbs_on_the_full_configured_count() {
    use vyges_mpl::cluster::{Cluster, ClusterType};
    use vyges_mpl::placement::{run_hierarchical_macro_placement, ParentOutcome};

    const N: usize = 8;
    let problem_n = || {
        let mut macros = Vec::new();
        let mut reshape = Vec::new();
        for i in 0..N {
            let w = 150 + (i as i32 % 3) * 50;
            macros.push(SoftMacro {
                width: w,
                height: w,
                area: (w as i64) * (w as i64),
                is_macro_cluster: i % 2 == 0,
                ..Default::default()
            });
            reshape.push(macro_cluster(w, w));
        }
        ParentProblem {
            macros,
            reshape,
            number_of_sequence_pair_macros: N,
            inputs: PlacementInputs {
                attributes: vec![Default::default(); N],
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
            names: Vec::new(),
        }
    };

    let run = |n: i32| {
        let mut root = Cluster::new(0, "root");
        root.cluster_type = ClusterType::Mixed;
        for id in 1..=2 {
            let mut child = Cluster::new(id, format!("child{id}"));
            child.cluster_type = ClusterType::StdCell;
            root.children.push(child);
        }
        let p = SaParameters { max_num_step: 5, num_perturb_per_step: n, ..Default::default() };
        run_hierarchical_macro_placement(
            &root,
            &[0.25],
            1,
            &p,
            ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
            SoftWeights::placement_defaults(),
            0,
            &mut |_| Some((problem_n(), 0)),
        )
    };

    let one = run(1);
    assert!(
        one.iter().any(|v| matches!(v.outcome, ParentOutcome::Placed { .. })),
        "the fixture must actually place something, or the comparisons below are vacuous"
    );
    assert_eq!(one, run(N as i32), "one and eight both floor to the eight-macro count");
    assert_ne!(one, run(500), "five hundred is above the floor and must NOT coincide");
}

/// 🔑 **With a placement context the sweep fills the placement factors too.** Before `initialize`
/// sampled all nine terms, six of them stayed at `Normalization::default()`'s `1.0` and every
/// placement cost reached the total undamped — the reference divides by a measured average.
///
/// ⚠️ A term that is zero on every sample still floors to exactly `1.0`, and that is not a
/// failure to sample: no fence is declared here, so the fence average is genuinely zero.
#[test]
fn a_placement_sweep_fills_the_placement_normalisation_factors() {
    let p = SaParameters { max_num_step: 5, num_perturb_per_step: 30, ..Default::default() };
    let probs = ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2);
    // ⚠️ The stock fixture declares no nets, and its two macros never overflow the outline, so
    // every placement term is zero on every sample and all nine would floor to 1.0 — proving
    // nothing. One bundled net makes the wirelength term real.
    let mut prob = problem();
    prob.inputs.nets = vec![vyges_mpl::placement::BundledNet { source: 0, target: 1, weight: 1.0 }];
    let s = anneal_one_run(&prob, 0.25, 1, &p, probs, SoftWeights::placement_defaults())
        .expect("the fixture places");

    let n = &s.normalization;
    assert_ne!(n.wirelength, 1.0, "one net, so the wirelength average is measured, not floored");
    assert_eq!(n.fence, 1.0, "no fence is declared, so its average is zero and floors to one");
    assert_eq!(n.guidance, 1.0, "likewise no guide");
}

/// 🔑 **The driver fills the dead space before handing the macros back.** Upstream calls
/// `best_sa->fillDeadSpace()` between choosing the winning run and reporting it, so every consumer
/// downstream — the summary, the children's shapes, the DEF — sees the GROWN geometry, never the
/// annealer's.
///
/// ⚠️ The fixture's second child is a standard-cell cluster, which is one of the two kinds the
/// filler grows; the first is a macro cluster, which it must leave alone.
#[test]
fn the_driver_fills_dead_space_before_returning_the_placement() {
    use vyges_mpl::cluster::{Cluster, ClusterType};
    use vyges_mpl::placement::{run_hierarchical_macro_placement, ParentOutcome};

    let mut root = Cluster::new(0, "root");
    root.cluster_type = ClusterType::Mixed;
    let mut child = Cluster::new(1, "child");
    child.cluster_type = ClusterType::StdCell;
    root.children.push(child);

    let p = SaParameters { max_num_step: 5, num_perturb_per_step: 8, ..Default::default() };
    let visits = run_hierarchical_macro_placement(
        &root,
        &[0.25],
        1,
        &p,
        ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
        SoftWeights::placement_defaults(),
        0,
        &mut |_| Some((problem(), 0)),
    );

    let macros = visits
        .iter()
        .find_map(|v| match &v.outcome {
            ParentOutcome::Placed { macros, .. } => Some(macros),
            _ => None,
        })
        .expect("the root is placed");

    // ⚠️ The threshold has to clear the size the ANNEALER alone produces, or the test passes
    // whether or not the fill ran. `apply_utilization` inflates this 90,000-unit cell cluster by
    // 1/0.25, so unfilled it is 600x600 — 360,000. Filled it takes the outline's spare width and
    // measures 2000x1423. One million separates the two with room either side.
    let cell = &macros[1];
    assert!(
        cell.width as i64 * cell.height as i64 > 1_000_000,
        "the cell cluster grew into the dead space, got {}x{}",
        cell.width,
        cell.height
    );
    assert_eq!(
        cell.area,
        cell.width as i64 * cell.height as i64,
        "and its area followed the grown shape"
    );
}

/// ⛔ **A macro cluster's shape curve is its TILING LIST, and nothing else supplies it.** The
/// utilization pass skips hard-macro clusters entirely, so if the curve is not set here it stays
/// empty — and an empty curve makes a `resize` action on that macro a no-op where upstream
/// reshapes it. The random walk then diverges from the first resize onwards.
///
/// 🔑 **The final placement of a small design still converges**, so the penalty VALUES match and
/// only the normalisation factors — averages over the walk — reveal it. Measured on `guides1`:
/// with the curve set, eight of the nine factors and the total cost become byte-exact against the
/// reference; without it, six of them are wrong by up to 13%.
///
/// ⚠️ `setShapes(tilings)` also MOVES the macro onto its first tiling; it is not only a constraint
/// list.
#[test]
fn a_macro_cluster_is_given_its_tilings_as_its_shape_curve() {
    let p = SaParameters { max_num_step: 5, num_perturb_per_step: 8, ..Default::default() };
    let probs = ActionProbabilities::placement_defaults();

    let mut prob = problem();
    // Two tilings for the macro cluster, the first of which is NOT its current shape.
    prob.reshape[0].tilings = vec![(400, 100), (100, 400)];
    let s = anneal_one_run(&prob, 0.25, 1, &p, probs, SoftWeights::placement_defaults())
        .expect("the fixture places");

    assert_eq!(
        s.curves[0].width_intervals.len(),
        2,
        "both tilings became shape-curve intervals"
    );
    assert_eq!(
        (s.curves[0].width_intervals[0].min, s.curves[0].width_intervals[0].max),
        (400, 400),
        "a tiling is a POINT interval, not a range"
    );
}
