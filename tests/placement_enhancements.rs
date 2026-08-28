// SPDX-License-Identifier: Apache-2.0
//! The two enhancements that run after the annealer has finished: centralization, and — only if
//! centralizing made things worse — macro cluster alignment.

use vyges_mpl::anneal::SoftMacro;
use vyges_mpl::placement::{
    align_macro_clusters, alignment_thresholds, attempt_centralization,
    attempt_macro_cluster_alignment, centralization_offset, cluster_locations, move_floorplan,
    run_enhancements, set_cluster_locations, ClusterCountMismatch, Enhancements,
};

fn sm(x: i32, y: i32, w: i32, h: i32) -> SoftMacro {
    SoftMacro { x, y, width: w, height: h, fixed: false, area: w as i64 * h as i64, is_macro_cluster: false }
}

fn macro_cluster(x: i32, y: i32, w: i32, h: i32) -> SoftMacro {
    SoftMacro { is_macro_cluster: true, ..sm(x, y, w, h) }
}

/// A stand-in for the annealer's state. ⚠️ `cal_penalty` pops the next scripted cost, so a test
/// controls the accept/revert decision AND pins how many times the penalties are recomputed.
struct Stub {
    macros: Vec<SoftMacro>,
    order: Vec<usize>,
    outline: (i32, i32),
    packing: (i32, i32),
    outline_penalty: f32,
    valid: bool,
    notch: (i32, i32),
    cost: f32,
    scripted: Vec<f32>,
    penalty_calls: usize,
}

impl Stub {
    fn new(macros: Vec<SoftMacro>, outline: (i32, i32), packing: (i32, i32)) -> Self {
        let order = (0..macros.len()).collect();
        Self {
            macros,
            order,
            outline,
            packing,
            outline_penalty: 0.0,
            valid: true,
            notch: (100, 100),
            cost: 1.0,
            scripted: Vec::new(),
            penalty_calls: 0,
        }
    }

    fn positions(&self) -> Vec<(i32, i32)> {
        self.macros.iter().map(|m| (m.x, m.y)).collect()
    }
}

impl Enhancements for Stub {
    fn macros(&self) -> &[SoftMacro] {
        &self.macros
    }
    fn macros_mut(&mut self) -> &mut [SoftMacro] {
        &mut self.macros
    }
    fn order(&self) -> &[usize] {
        &self.order
    }
    fn outline(&self) -> (i32, i32) {
        self.outline
    }
    fn packing(&self) -> (i32, i32) {
        self.packing
    }
    fn outline_penalty(&self) -> f32 {
        self.outline_penalty
    }
    fn is_valid(&self) -> bool {
        self.valid
    }
    fn notch_thresholds(&self) -> (i32, i32) {
        self.notch
    }
    fn cal_penalty(&mut self) {
        self.penalty_calls += 1;
        if !self.scripted.is_empty() {
            self.cost = self.scripted.remove(0);
        }
    }
    fn norm_cost(&self) -> f32 {
        self.cost
    }
}

// ---------------------------------------------------------------- locations

/// ⚠️ **Indexed by MACRO ID, not by position in the sequence.** Writing it positionally scrambles
/// every location the moment the annealer has swapped anything.
#[test]
fn locations_are_indexed_by_macro_id() {
    let macros = vec![sm(10, 10, 1, 1), sm(20, 20, 1, 1), sm(30, 30, 1, 1)];
    let shuffled = [2usize, 0, 1];
    assert_eq!(cluster_locations(&macros, &shuffled), vec![(10, 10), (20, 20), (30, 30)]);
}

/// ⚠️ A list that does not match the sequence pair is upstream's MPL-52.
#[test]
fn a_mismatched_location_list_is_refused() {
    let mut macros = vec![sm(0, 0, 1, 1), sm(0, 0, 1, 1)];
    let order = [0usize, 1];
    assert_eq!(
        set_cluster_locations(&mut macros, &order, &[(5, 5)]),
        Err(ClusterCountMismatch)
    );
    assert!(set_cluster_locations(&mut macros, &order, &[(5, 5), (6, 6)]).is_ok());
    assert_eq!((macros[0].x, macros[1].y), (5, 6));
}

/// ⛔ **CORRECTED — a FIXED macro is NOT shifted.** This asserted the opposite, reasoning that
/// `moveFloorplan` has no `isFixed` test. It has none because it does not need one: it assigns
/// through `setX`/`setY`, and the guard is in those. A blockage's soft macro is fixed and inside
/// the sequence pair, and centralizing leaves it exactly where the die put it.
///
/// ⚠️ The sixth place this same misreading was written down. Every one of them read a CALL SITE
/// and never the setter it assigns through.
#[test]
fn a_fixed_macro_is_not_shifted_with_the_rest() {
    let mut blockage = sm(100, 100, 50, 50);
    blockage.fixed = true;
    let mut macros = vec![blockage, sm(200, 200, 50, 50)];
    move_floorplan(&mut macros, &[0, 1], (10, 20));
    assert_eq!((macros[0].x, macros[0].y), (100, 100), "the blockage stayed put");
    assert_eq!((macros[1].x, macros[1].y), (210, 220), "everything else shifted");
}

/// ⚠️ **Anything outside the sequence pair stays put** — the IO clusters and fixed terminals
/// appended after the clusters are never in it.
#[test]
fn a_macro_outside_the_sequence_pair_is_untouched() {
    let mut macros = vec![sm(0, 0, 1, 1), sm(500, 500, 1, 1)];
    move_floorplan(&mut macros, &[0], (10, 10));
    assert_eq!((macros[1].x, macros[1].y), (500, 500), "the terminal did not move");
}

// ---------------------------------------------------------------- centralization

/// ⚠️ **Truncating integer division** — an odd slack leaves the spare unit at the top and right.
#[test]
fn the_centralization_offset_truncates() {
    assert_eq!(centralization_offset((1000, 1000), (400, 400)), (300, 300));
    assert_eq!(centralization_offset((1001, 1001), (400, 400)), (300, 300), "the spare unit");
    assert_eq!(centralization_offset((1000, 1000), (1000, 1000)), (0, 0));
}

/// ⛔ **A floorplan that overflows its outline is not centralized at all** — and that early return
/// is NOT a revert, so it costs the alignment step too.
#[test]
fn an_overflowing_floorplan_gets_neither_enhancement() {
    let mut s = Stub::new(vec![sm(0, 0, 400, 400)], (1000, 1000), (1200, 1200));
    s.outline_penalty = 0.5;
    let before = s.positions();
    assert!(!attempt_centralization(&mut s, 1.0, false), "not reverted — never attempted");
    assert_eq!(s.positions(), before);
    assert_eq!(s.penalty_calls, 0, "nothing was even rescored");
}

/// 🔑 **A centralization that improves the cost stands**, and reports no revert — so alignment is
/// never reached.
#[test]
fn a_cheaper_centralization_is_kept() {
    let mut s = Stub::new(vec![sm(0, 0, 400, 400)], (1000, 1000), (400, 400));
    s.scripted = vec![0.5];
    assert!(!attempt_centralization(&mut s, 1.0, false));
    assert_eq!(s.positions(), vec![(300, 300)], "moved to the middle and left there");
    assert_eq!(s.penalty_calls, 1, "scored once, not rescored");
}

/// ⚠️ **`> pre_cost`, strictly** — an exactly equal cost keeps the centralized floorplan.
#[test]
fn an_equally_costly_centralization_is_kept() {
    let mut s = Stub::new(vec![sm(0, 0, 400, 400)], (1000, 1000), (400, 400));
    s.scripted = vec![1.0];
    assert!(!attempt_centralization(&mut s, 1.0, false));
    assert_eq!(s.positions(), vec![(300, 300)]);
}

/// 🔑 **A costlier centralization is undone exactly**, and the revert is what unlocks alignment.
#[test]
fn a_costlier_centralization_is_reverted_exactly() {
    let mut s = Stub::new(vec![sm(10, 20, 400, 400), sm(500, 600, 100, 100)], (1000, 1000), (400, 400));
    let before = s.positions();
    s.scripted = vec![2.0, 1.0];
    assert!(attempt_centralization(&mut s, 1.0, false), "reverted");
    assert_eq!(s.positions(), before, "back to where it started");
    assert_eq!(s.penalty_calls, 2, "scored after the move and again after the revert");
}

/// ⚠️ **Forcing keeps a centralization that costs more**, and still reports no revert — so a
/// forced run gets no alignment either.
#[test]
fn forcing_keeps_a_costlier_centralization() {
    let mut s = Stub::new(vec![sm(0, 0, 400, 400)], (1000, 1000), (400, 400));
    s.scripted = vec![2.0];
    assert!(!attempt_centralization(&mut s, 1.0, true));
    assert_eq!(s.positions(), vec![(300, 300)], "kept despite costing more");
    assert_eq!(s.penalty_calls, 1);
}

// ---------------------------------------------------------------- alignment

/// ⛔ **The thresholds are crossed relative to their names**: `h` governs X and is floored by a
/// tenth of the outline's HEIGHT; `v` governs Y and is floored by a tenth of the WIDTH.
#[test]
fn the_alignment_thresholds_are_crossed() {
    // A 2000 x 400 outline: h is floored at 40, v at 200.
    let none: [SoftMacro; 0] = [];
    assert_eq!(alignment_thresholds(none.iter(), (2000, 400), (10_000, 10_000)), (40, 200));
}

/// 🔑 **Floored by the smallest macro cluster on the board** — its width for `h`, its height for
/// `v`. One thin macro cluster stops everything else aligning.
#[test]
fn the_smallest_macro_cluster_floors_both_thresholds() {
    let macros = [macro_cluster(0, 0, 30, 500), macro_cluster(0, 0, 900, 25)];
    let got = alignment_thresholds(macros.iter(), (2000, 400), (10_000, 10_000));
    assert_eq!(got, (30, 25), "the narrowest width and the shortest height, each below the floor");
}

/// ⚠️ **With the notch term dark the thresholds are still the constructor's `10` units**, which is
/// small enough that nothing aligns.
#[test]
fn a_dark_notch_term_leaves_the_thresholds_tiny() {
    let none: [SoftMacro; 0] = [];
    assert_eq!(alignment_thresholds(none.iter(), (2000, 400), (10, 10)), (10, 10));
}

/// 🔑 A macro cluster within a threshold of an edge is pushed onto it; one further in is not.
#[test]
fn a_nearby_macro_cluster_snaps_to_the_edge() {
    let mut macros = vec![
        macro_cluster(30, 30, 100, 100),
        macro_cluster(400, 400, 100, 100),
        macro_cluster(880, 880, 100, 100),
    ];
    let order = [0usize, 1, 2];
    align_macro_clusters(&mut macros, &order, (1000, 1000), (50, 50));
    assert_eq!((macros[0].x, macros[0].y), (0, 0), "snapped to the origin corner");
    assert_eq!((macros[1].x, macros[1].y), (400, 400), "too far in to snap");
    assert_eq!((macros[2].x, macros[2].y), (900, 900), "snapped to the far corner");
}

/// ⚠️ **Strictly less than the threshold**, on the near coordinate and on the far gap alike.
#[test]
fn a_macro_cluster_exactly_a_threshold_away_does_not_snap() {
    let mut macros = vec![macro_cluster(50, 50, 100, 100)];
    align_macro_clusters(&mut macros, &[0], (1000, 1000), (50, 50));
    assert_eq!((macros[0].x, macros[0].y), (50, 50));
}

/// ⚠️ **`else if`, so the left test wins.** A macro cluster wider than the outline satisfies both
/// and goes LEFT.
#[test]
fn a_cluster_wider_than_the_outline_snaps_left() {
    let mut macros = vec![macro_cluster(5, 5, 2000, 2000)];
    align_macro_clusters(&mut macros, &[0], (1000, 1000), (50, 50));
    assert_eq!((macros[0].x, macros[0].y), (0, 0), "not to 1000 - 2000");
}

/// ⛔ **Only macro clusters are aligned** — a standard-cell or mixed cluster sitting in the same
/// corner is left where the annealer put it.
#[test]
fn only_macro_clusters_are_aligned() {
    let mut macros = vec![sm(30, 30, 100, 100)];
    align_macro_clusters(&mut macros, &[0], (1000, 1000), (50, 50));
    assert_eq!((macros[0].x, macros[0].y), (30, 30));
}

/// ⚠️ **An invalid floorplan is left alone** — alignment polishes a solution, it does not rescue
/// one.
#[test]
fn an_invalid_floorplan_is_not_aligned() {
    let mut s = Stub::new(vec![macro_cluster(30, 30, 100, 100)], (1000, 1000), (400, 400));
    s.valid = false;
    assert!(!attempt_macro_cluster_alignment(&mut s));
    assert_eq!(s.positions(), vec![(30, 30)]);
    assert_eq!(s.penalty_calls, 0);
}

/// ⛔ **No force override here**, unlike centralization: an alignment that costs more is always
/// undone.
#[test]
fn a_costlier_alignment_is_always_reverted() {
    let mut s = Stub::new(vec![macro_cluster(30, 30, 100, 100)], (1000, 1000), (400, 400));
    s.notch = (50, 50);
    s.scripted = vec![9.0, 1.0];
    assert!(attempt_macro_cluster_alignment(&mut s), "reverted");
    assert_eq!(s.positions(), vec![(30, 30)]);
    assert_eq!(s.penalty_calls, 2);
}

/// ⚠️ A cheaper alignment stands.
#[test]
fn a_cheaper_alignment_is_kept() {
    let mut s = Stub::new(vec![macro_cluster(30, 30, 100, 100)], (1000, 1000), (400, 400));
    s.notch = (50, 50);
    s.scripted = vec![0.5];
    assert!(!attempt_macro_cluster_alignment(&mut s));
    assert_eq!(s.positions(), vec![(0, 0)]);
}

// ---------------------------------------------------------------- the two together

/// 🔑 **Alignment is the consolation prize**: it runs only when centralization was tried and
/// reverted.
#[test]
fn alignment_runs_only_after_a_reverted_centralization() {
    // Centralization costs more, so it reverts and alignment follows and is cheaper.
    let mut s = Stub::new(vec![macro_cluster(30, 30, 100, 100)], (1000, 1000), (100, 100));
    s.notch = (50, 50);
    s.cost = 1.0;
    s.scripted = vec![2.0, 1.0, 0.5];
    run_enhancements(&mut s, false);
    assert_eq!(s.positions(), vec![(0, 0)], "centralization undone, then aligned to the corner");

    // A centralization that sticks means alignment is never attempted.
    let mut s = Stub::new(vec![macro_cluster(30, 30, 100, 100)], (1000, 1000), (100, 100));
    s.notch = (50, 50);
    s.cost = 1.0;
    s.scripted = vec![0.5];
    run_enhancements(&mut s, false);
    assert_eq!(s.positions(), vec![(480, 480)], "left centralized, not aligned");
    assert_eq!(s.penalty_calls, 1);
}

/// ⛔ **An overflowing floorplan gets neither**, because the early return is not a revert.
#[test]
fn an_overflowing_floorplan_is_left_entirely_alone() {
    let mut s = Stub::new(vec![macro_cluster(30, 30, 100, 100)], (1000, 1000), (2000, 2000));
    s.outline_penalty = 1.0;
    s.notch = (50, 50);
    run_enhancements(&mut s, false);
    assert_eq!(s.positions(), vec![(30, 30)]);
    assert_eq!(s.penalty_calls, 0);
}

/// ⛔ **THE `isFixed` GUARD LIVES IN `setX`/`setY`, NOT AT THE CALL SITES.** `moveFloorplan`,
/// `setClustersLocations`, `packFloorplan` and the boundary alignment all assign positions with no
/// fixed test of their own — they do not need one, because the setters refuse silently.
///
/// ⚠️ **Reading a call site and concluding "there is no fixed test" got this wrong four times** in
/// this engine's notes and comments. The alignment then snapped a FIXED macro to the outline edge,
/// moving an input the design does not allow to move: on `fixed_macros1` it put the macro at
/// `outline.dy() - height` instead of its real position, and the whole placement was scored
/// against a floorplan that does not exist.
#[test]
fn a_fixed_macro_is_selected_by_the_alignment_but_never_moved() {
    use vyges_mpl::anneal::SoftMacro;
    // Close enough to the top edge that the alignment would snap it there.
    // ⚠️ Near the LEFT edge and the TOP edge, so both axes are actually exercised. With x at 100
    // against a threshold of 100 the `lx < h_th` test is false and the x assignment never happens
    // at all — the mutation harness caught that as a WRONG TEST.
    let make = |fixed: bool| SoftMacro {
        x: 50,
        y: 800,
        width: 200,
        height: 150,
        fixed,
        area: 200 * 150,
        is_macro_cluster: true,
    };

    let mut movable = [make(false)];
    align_macro_clusters(&mut movable, &[0], (1000, 1000), (100, 100));
    assert_eq!(movable[0].y, 850, "a movable macro snaps to the top edge");
    assert_eq!(movable[0].x, 0, "and to the left edge");

    let mut pinned = [make(true)];
    align_macro_clusters(&mut pinned, &[0], (1000, 1000), (100, 100));
    assert_eq!(pinned[0].y, 800, "a FIXED one is considered and then refused");
    assert_eq!(pinned[0].x, 50, "on both axes");
}
