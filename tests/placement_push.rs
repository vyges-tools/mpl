// SPDX-License-Identifier: Apache-2.0
//! Whether the boundary push runs at all, and which macro clusters it considers.

use vyges_mpl::cluster::ClusterType;
use vyges_mpl::placement::{
    fetch_macro_clusters, has_single_centralized_macro_array, push_decision, soft_macro_area,
    NoPush,
};

/// ⛔ **The pusher reads `Cluster::getClusterType()`, which is NOT the same question as
/// `isIOCluster()` or `isFixedMacro()`.** Those are separate flags set by `setAs*` methods that
/// never touch `type_`; an IO cluster is therefore `Mixed` (the member's default) and a fixed macro
/// cluster is `HardMacro` (typed alongside the movable ones in `clusterMacros`). Every fixture here
/// is in that vocabulary.
///
/// A small tree: id -> (type, children).
fn tree() -> Vec<(ClusterType, Vec<usize>)> {
    vec![
        // 0: root, mixed
        (ClusterType::Mixed, vec![1, 2, 3]),
        // 1: a macro cluster
        (ClusterType::HardMacro, vec![]),
        // 2: a mixed cluster holding another macro cluster
        (ClusterType::Mixed, vec![4]),
        // 3: a std cell cluster hiding a macro cluster
        (ClusterType::StdCell, vec![5]),
        (ClusterType::HardMacro, vec![]),
        (ClusterType::HardMacro, vec![]),
    ]
}

/// 🔑 **Depth-first in child order**, descending through mixed clusters.
#[test]
fn macro_clusters_are_fetched_depth_first_through_mixed_clusters() {
    let t = tree();
    let type_of = |i: usize| t[i].0;
    let children_of = |i: usize| t[i].1.clone();
    assert_eq!(fetch_macro_clusters(0, &type_of, &children_of), vec![1, 4]);
}

/// ⚠️ **A standard-cell cluster is NOT descended into**, so a macro cluster underneath one is
/// never fetched. Nothing builds that shape today; the restriction is the reference's.
#[test]
fn a_macro_cluster_under_a_std_cell_cluster_is_never_fetched() {
    let t = tree();
    let type_of = |i: usize| t[i].0;
    let children_of = |i: usize| t[i].1.clone();
    let got = fetch_macro_clusters(0, &type_of, &children_of);
    assert!(!got.contains(&5), "cluster 5 is under a std cell cluster: {got:?}");
}

// ---------------------------------------------------------------- the centralized array test

/// 🔑 **The arrangement `singleArraySingleStdCellCluster` produces**: one macro array, and a
/// standard-cell cluster shrunk to nothing. The push then declines — the array is already placed.
#[test]
fn one_array_beside_a_shrunk_cell_cluster_is_centralized() {
    let children = [(ClusterType::HardMacro, 40_000i64), (ClusterType::StdCell, 0)];
    assert!(has_single_centralized_macro_array(&children));
}

/// ⛔ **A standard-cell cluster is judged by its SOFT MACRO's area.** A cluster that was NOT shrunk
/// has a non-zero one and fails the test — which is what makes this the mirror of the shrinking
/// step rather than a general "is there one array" question.
#[test]
fn a_cell_cluster_that_was_not_shrunk_fails_the_test() {
    let children = [(ClusterType::HardMacro, 40_000i64), (ClusterType::StdCell, 1)];
    assert!(!has_single_centralized_macro_array(&children), "one square unit is enough to fail it");
}

/// ⚠️ **Any MIXED cluster fails it immediately.**
#[test]
fn a_mixed_cluster_fails_it_at_once() {
    let children = [(ClusterType::Mixed, 0i64), (ClusterType::HardMacro, 1)];
    assert!(!has_single_centralized_macro_array(&children));
}

/// ⚠️ Two macro clusters is not a single array.
#[test]
fn two_macro_clusters_fail_it() {
    let children = [(ClusterType::HardMacro, 1i64), (ClusterType::HardMacro, 1)];
    assert!(!has_single_centralized_macro_array(&children));
}

/// ⚠️ **The count test is inside the loop**, so the second macro cluster fails it at once — before
/// a later child that would also have failed it is even seen.
#[test]
fn the_second_macro_cluster_fails_it_before_later_children_are_seen() {
    let children = [
        (ClusterType::HardMacro, 1i64),
        (ClusterType::HardMacro, 1),
        (ClusterType::StdCell, 999),
    ];
    assert!(!has_single_centralized_macro_array(&children));
}

/// ⛔ **AN IO CLUSTER IS A `Mixed` CLUSTER AND FAILS THE GUARD OUTRIGHT.** `setAsIOBundle`,
/// `setAsIOPadCluster` and `setAsClusterOfUnplacedIOPins` set a flag and a soft macro and leave
/// `type_` at its `MixedCluster` default, so the reference's `switch` takes its Mixed case and
/// returns false.
///
/// ⚠️ This is why the guard almost never fires in practice: one unplaced IO pin, one IO pad or one
/// IO bundle is enough. Reading `isIOCluster()` here and skipping the child declined the push on
/// **27 of 34** designs while every earlier gate stayed green.
#[test]
fn an_io_cluster_fails_the_guard_because_its_type_is_mixed() {
    let children = [
        (ClusterType::Mixed, 0i64),
        (ClusterType::HardMacro, 1),
        (ClusterType::StdCell, 0),
    ];
    assert!(!has_single_centralized_macro_array(&children), "the IO cluster is a Mixed child");
}

/// ⛔ **A FIXED macro cluster is a `HardMacro` and IS COUNTED.** `setAsFixedMacro` only sets a
/// flag; `clusterMacros` types the fixed macro clusters `HardMacroCluster` a few lines after the
/// movable ones. So one movable plus one fixed macro cluster is a count of two.
#[test]
fn a_fixed_macro_cluster_counts_towards_the_two() {
    let children = [(ClusterType::HardMacro, 999i64), (ClusterType::HardMacro, 1)];
    assert!(!has_single_centralized_macro_array(&children), "fixed or not, it is the second");
}

/// ⛔ **`SoftMacro::getArea` reports `0` for an area of `1`** — `area_ > 1 ? area_ : 0`. The guard
/// reads the accessor, so a one-unit cluster is "shrunk away" as far as it is concerned.
#[test]
fn a_one_unit_soft_macro_reports_as_zero() {
    assert_eq!(soft_macro_area(1), 0);
    assert_eq!(soft_macro_area(0), 0);
    assert_eq!(soft_macro_area(2), 2, "above the threshold it is reported as it stands");
}

/// ℹ️ A root with no children passes vacuously — zero arrays counts as one.
#[test]
fn an_empty_root_passes_vacuously() {
    assert!(has_single_centralized_macro_array(&[]));
}

// ---------------------------------------------------------------- the two guards

/// ⚠️ A design that is nothing but macros is never pushed.
#[test]
fn an_all_macro_design_is_not_pushed() {
    assert_eq!(
        push_decision(ClusterType::HardMacro, &[]),
        Err(NoPush::DesignIsAllMacros)
    );
}

/// ⚠️ The all-macros test comes FIRST, so it wins even when the array test would also have fired.
#[test]
fn the_all_macro_guard_is_checked_first() {
    let children = [(ClusterType::HardMacro, 1i64), (ClusterType::StdCell, 0)];
    assert_eq!(
        push_decision(ClusterType::HardMacro, &children),
        Err(NoPush::DesignIsAllMacros),
        "not SingleCentralizedMacroArray"
    );
}

/// 🔑 An ordinary design with two macro clusters is pushed.
#[test]
fn an_ordinary_design_is_pushed() {
    let children = [
        (ClusterType::HardMacro, 1i64),
        (ClusterType::HardMacro, 1),
        (ClusterType::StdCell, 999),
    ];
    assert_eq!(push_decision(ClusterType::Mixed, &children), Ok(()));
}

// ---------------------------------------------------------------- the push itself

use vyges_mpl::halo::Boundary;
use vyges_mpl::placement::{
    distance_to_close_boundaries, move_towards_boundary, push_macro_cluster, PushAttempt,
};

const CORE: (i32, i32, i32, i32) = (0, 0, 1000, 1000);

/// 🔑 **At most one horizontal and one vertical boundary**, each the nearer of its pair.
#[test]
fn a_cluster_is_never_pushed_both_ways_on_one_axis() {
    // Near the bottom-left corner; macros 200 x 200.
    let got = distance_to_close_boundaries((50, 30, 250, 230), CORE, 200, 200);
    assert_eq!(got, vec![(Boundary::B, 30), (Boundary::L, 50)]);
}

/// ⚠️ **The threshold is the MACRO's own dimension**, not the cluster's — a cluster further away
/// than one macro is wide is left alone, which is what stops the push dragging it across the die.
#[test]
fn a_cluster_further_than_one_macro_is_left_alone() {
    // 300 from the left, macros only 200 wide.
    let got = distance_to_close_boundaries((300, 300, 500, 500), CORE, 200, 200);
    assert!(got.is_empty(), "{got:?}");

    // Widen the macro past the distance and it qualifies.
    let got = distance_to_close_boundaries((300, 300, 500, 500), CORE, 301, 301);
    assert_eq!(got.len(), 2);
}

/// ⛔ **The horizontal test uses the macro's WIDTH and the vertical its HEIGHT** — unlike the notch
/// thresholds, which are crossed. A SQUARE macro cannot tell the two readings apart, and a mutation
/// proved that every fixture here was square.
#[test]
fn each_axis_is_measured_against_its_own_macro_dimension() {
    // Macros 200 wide and 400 tall. The cluster is 300 from the left — further than one macro
    // WIDTH, so no horizontal push; but nearer than one macro HEIGHT, so the crossed reading
    // would push it.
    // Vertically it is centred at 450 from either edge, further than 400, so nothing there either.
    let got = distance_to_close_boundaries((300, 450, 500, 550), CORE, 200, 400);
    assert!(got.is_empty(), "the crossed reading would have pushed it left: {got:?}");
}

/// ⛔ **A tie goes to RIGHT, and to TOP.** A centred cluster is pushed towards the far edges.
#[test]
fn a_tie_goes_right_and_top() {
    // Exactly centred: 400 from every edge.
    let got = distance_to_close_boundaries((400, 400, 600, 600), CORE, 500, 500);
    assert_eq!(got, vec![(Boundary::T, 400), (Boundary::R, 400)]);
}

/// ⚠️ **Both distances are `abs`**, so a cluster already OUTSIDE the core reads as close to the
/// boundary it has passed — and is pushed further out rather than back in.
#[test]
fn a_cluster_outside_the_core_is_pushed_further_out() {
    // Its left edge is 50 past the core's left edge.
    let got = distance_to_close_boundaries((-50, 400, 150, 600), CORE, 200, 200);
    assert_eq!(got, vec![(Boundary::L, 50)], "50, not -50");
}

/// ⚠️ **The result comes out in enum order — B, L, T, R** — not in the order the two were decided.
#[test]
fn the_boundaries_come_out_in_enum_order() {
    let got = distance_to_close_boundaries((50, 30, 250, 230), CORE, 200, 200);
    let order: Vec<Boundary> = got.iter().map(|(b, _)| *b).collect();
    assert_eq!(order, vec![Boundary::B, Boundary::L], "B before L");

    // Top-right corner: T before R.
    let got = distance_to_close_boundaries((750, 770, 950, 970), CORE, 200, 200);
    let order: Vec<Boundary> = got.iter().map(|(b, _)| *b).collect();
    assert_eq!(order, vec![Boundary::T, Boundary::R]);

    // ⛔ **Top-LEFT is the corner that proves it is a SORT and not a reversal.** In the other three
    // corners the two happen to agree — the vertical boundary sorts ahead of the horizontal one and
    // is decided second, so reversing the decision order gives the same answer. Here `L` (1) sorts
    // ahead of `T` (2) while still being decided first, and only a sort keeps it there. A mutation
    // that reversed instead of sorting went straight through the other three.
    let got = distance_to_close_boundaries((50, 750, 250, 950), CORE, 200, 200);
    assert_eq!(got, vec![(Boundary::L, 50), (Boundary::T, 50)], "L before T");
}

/// ⚠️ `L` and `B` move negative, `R` and `T` positive.
#[test]
fn the_direction_comes_from_the_boundary_not_the_sign() {
    let b = (100, 100, 200, 200);
    assert_eq!(move_towards_boundary(b, Boundary::L, 50), (50, 100, 150, 200));
    assert_eq!(move_towards_boundary(b, Boundary::R, 50), (150, 100, 250, 200));
    assert_eq!(move_towards_boundary(b, Boundary::B, 50), (100, 50, 200, 150));
    assert_eq!(move_towards_boundary(b, Boundary::T, 50), (100, 150, 200, 250));
}

/// 🔑 **The two pushes COMPOSE** — the second is applied to the box the first left behind.
#[test]
fn the_two_pushes_compose() {
    let (moved, attempts) = push_macro_cluster(
        (50, 30, 250, 230),
        &[(Boundary::B, 30), (Boundary::L, 50)],
        &|_| None,
    );
    assert_eq!(moved, (0, 0, 200, 200), "into the corner");
    assert!(attempts.iter().all(|a| a.committed));
}

/// ⚠️ **A reverted push leaves the box exactly as it was**, and does not stop the next one.
///
/// ⛔ **The revert has to come FIRST for this to prove anything.** With the revert second, a
/// version that gave up after the first failure would behave identically — a mutation proved that
/// the original fixture here did exactly that.
#[test]
fn a_reverted_push_does_not_block_the_next() {
    // Anything moved DOWN overlaps; moving left does not. The bottom push is tried first.
    let obstacle_for = |b: (i32, i32, i32, i32)| {
        if b.1 < 30 { Some(PushObstacle::HardMacro(0)) } else { None }
    };
    let (moved, attempts) = push_macro_cluster(
        (50, 30, 250, 230),
        &[(Boundary::B, 30), (Boundary::L, 50)],
        &obstacle_for,
    );
    assert_eq!(moved, (0, 30, 200, 230), "the LEFT push still happened after the bottom failed");
    assert_eq!(
        attempts,
        vec![
            PushAttempt {
                boundary: Boundary::B,
                distance: 30,
                committed: false,
                // ⚠️ The obstacle is CARRIED, because the reference names it in the revert line.
                obstacle: Some(PushObstacle::HardMacro(0)),
            },
            PushAttempt { boundary: Boundary::L, distance: 50, committed: true, obstacle: None },
        ]
    );
}

/// ⚠️ **A distance of zero is skipped, not attempted** — the cluster is already on that boundary,
/// and no trace line is emitted for it.
#[test]
fn a_zero_distance_is_skipped_entirely() {
    let (moved, attempts) = push_macro_cluster(
        (0, 30, 200, 230),
        &[(Boundary::B, 30), (Boundary::L, 0)],
        &|_| None,
    );
    assert_eq!(moved, (0, 0, 200, 200));
    assert_eq!(attempts.len(), 1, "only the bottom push was attempted");
    assert_eq!(attempts[0].boundary, Boundary::B);
}

/// ⚠️ **The trace records ATTEMPTS, not commits** — upstream prints its line before the overlap
/// test, so its log says "Moved X" for pushes it then undoes. Anyone scoring against the
/// `boundary_push` channel is scoring attempts.
#[test]
fn a_reverted_push_is_still_an_attempt() {
    let (_, attempts) =
        push_macro_cluster((50, 50, 250, 250), &[(Boundary::L, 50)], &|_| {
            Some(PushObstacle::IoBlockage(0))
        });
    assert_eq!(attempts.len(), 1, "recorded even though it was undone");
    assert!(!attempts[0].committed);
}

// ---------------------------------------------------------------- obstruction

use vyges_mpl::placement::{boxes_overlap, move_hard_macro, push_obstacle, PushObstacle};

/// ⛔ **`Rect::overlaps` is STRICT where `Rect::intersects` is inclusive** — the same class, the
/// opposite edge rule, and `mpl` uses both. Two boxes that merely TOUCH do not overlap, which
/// matters because touching is exactly the arrangement a boundary push produces.
#[test]
fn touching_boxes_do_not_overlap() {
    let a = (0, 0, 100, 100);
    assert!(!boxes_overlap(a, (100, 0, 200, 100)), "edge to edge");
    assert!(!boxes_overlap(a, (0, 100, 100, 200)), "top to bottom");
    assert!(!boxes_overlap(a, (100, 100, 200, 200)), "corner to corner");
    assert!(boxes_overlap(a, (99, 0, 200, 100)), "one unit of real overlap");
}

/// ⚠️ **Only one axis moves** — the boundary decides which, and the other coordinate is untouched.
#[test]
fn a_hard_macro_moves_on_one_axis_only() {
    let at = (500, 600);
    assert_eq!(move_hard_macro(at, Boundary::L, 50), (450, 600));
    assert_eq!(move_hard_macro(at, Boundary::R, 50), (550, 600));
    assert_eq!(move_hard_macro(at, Boundary::B, 50), (500, 550));
    assert_eq!(move_hard_macro(at, Boundary::T, 50), (500, 650));
}

/// 🔑 **A macro belonging to the cluster being pushed is skipped**, by cluster id — otherwise every
/// push would collide with itself.
#[test]
fn a_cluster_does_not_obstruct_itself() {
    let macros = [(7i32, (0, 0, 100, 100)), (7, (100, 0, 200, 100))];
    assert_eq!(push_obstacle((0, 0, 200, 100), 7, &macros, &[]), None);

    // The same geometry owned by a different cluster does obstruct.
    let other = [(9i32, (0, 0, 100, 100))];
    assert_eq!(
        push_obstacle((0, 0, 200, 100), 7, &other, &[]),
        Some(PushObstacle::HardMacro(0))
    );
}

/// ⛔ **Hard macros are tested FIRST and the test short-circuits**, so a box overlapping both
/// reports only the macro — and the reference's trace prints only that line.
#[test]
fn a_hard_macro_is_reported_before_an_io_blockage() {
    let macros = [(9i32, (0, 0, 100, 100))];
    let blockages = [(0, 0, 100, 100)];
    assert_eq!(
        push_obstacle((0, 0, 50, 50), 7, &macros, &blockages),
        Some(PushObstacle::HardMacro(0)),
        "not the blockage, though both overlap"
    );
}

/// ⚠️ An IO blockage is reported when no macro obstructs.
#[test]
fn an_io_blockage_obstructs_on_its_own() {
    let blockages = [(500, 500, 600, 600), (0, 0, 100, 100)];
    assert_eq!(
        push_obstacle((50, 50, 150, 150), 7, &[], &blockages),
        Some(PushObstacle::IoBlockage(1)),
        "the second one, and it names which"
    );
}

/// ⚠️ Nothing overlapping is no obstacle, and the push commits.
#[test]
fn a_clear_box_has_no_obstacle() {
    let macros = [(9i32, (500, 500, 600, 600))];
    let blockages = [(700, 700, 800, 800)];
    assert_eq!(push_obstacle((0, 0, 100, 100), 7, &macros, &blockages), None);
}

/// 🔑 **The whole push, end to end**: a cluster near the bottom-left corner, blocked from moving
/// down by another cluster's macro but free to move left.
#[test]
fn a_cluster_is_pushed_around_an_obstacle() {
    let cluster_box = (50, 30, 250, 230);
    let boundaries = distance_to_close_boundaries(cluster_box, CORE, 200, 200);
    assert_eq!(boundaries, vec![(Boundary::B, 30), (Boundary::L, 50)]);

    // Another cluster's macro sits directly below.
    let macros = [(9i32, (60, 0, 240, 25))];
    let obstacle_for = |b: (i32, i32, i32, i32)| push_obstacle(b, 7, &macros, &[]);

    let (moved, attempts) = push_macro_cluster(cluster_box, &boundaries, &obstacle_for);
    assert_eq!(moved, (0, 30, 200, 230), "left only");
    assert!(!attempts[0].committed, "the downward push hit the obstacle");
    assert!(attempts[1].committed);
}
