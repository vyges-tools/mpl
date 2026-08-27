// SPDX-License-Identifier: Apache-2.0
//! Composing the placement stage: which clusters are visited, in what order, and what stops it.

use vyges_mpl::anneal::{SoftMacro, SoftWeights};
use vyges_mpl::placement::{
    place_children, placement_setup, utilization_list, AreaKind, ParentOutcome, PlacementTree,
};

/// A small tree: id -> (kind, is_fixed_macro, children).
type Node = (AreaKind, bool, Vec<i32>);

fn tree_of(nodes: &[Node]) -> impl Fn(i32) -> Node + '_ {
    move |i: i32| nodes[i as usize].clone()
}

fn macros(n: usize) -> Vec<SoftMacro> {
    vec![SoftMacro::default(); n]
}

/// 🔑 **The recursion happens AFTER the parent is placed** — a child's outline is the shape its
/// parent just chose, so placing the child first would place it in a box that does not exist yet.
#[test]
fn a_parent_is_placed_before_its_children() {
    let nodes: Vec<Node> = vec![
        (AreaKind::MixedCluster, false, vec![1]),
        (AreaKind::MixedCluster, false, vec![2]),
        (AreaKind::StdCellCluster, false, vec![]),
    ];
    let get = tree_of(&nodes);
    let utils = utilization_list(0.25, 10);
    let tree = PlacementTree {
        kind: &|i| get(i).0,
        is_fixed_macro: &|i| get(i).1,
        is_leaf: &|i| get(i).2.is_empty(),
        children: &|i| get(i).2.clone(),
        utilizations: &utils,
        num_threads: 10,
    };

    let mut order = Vec::new();
    let visits = place_children(&tree, 0, &mut |c, _, _| {
        order.push(c);
        Some(macros(1))
    });

    // ⚠️ Ten calls per parent, not one: with ten threads the WHOLE batch anneals before any of it
    // is judged, so every utilization is tried even once one has succeeded. What matters here is
    // that all of the root's runs precede all of the child's.
    let first_child_call = order.iter().position(|&c| c == 1).expect("the child was placed");
    assert!(order[..first_child_call].iter().all(|&c| c == 0), "{order:?}");
    assert!(order[first_child_call..].iter().all(|&c| c == 1), "{order:?}");
    let visited: Vec<i32> = visits.iter().map(|v| v.cluster).collect();
    assert_eq!(visited, vec![0, 1, 2]);
}

/// ⚠️ **Every child is visited, including the ones that do nothing.** An IO cluster and a leaf
/// standard-cell cluster both reach the guards and return there — they are not filtered out.
#[test]
fn clusters_that_do_nothing_are_still_visited() {
    let nodes: Vec<Node> = vec![
        (AreaKind::MixedCluster, false, vec![1, 2, 3, 4]),
        (AreaKind::IoCluster, false, vec![]),
        (AreaKind::StdCellCluster, false, vec![]),
        (AreaKind::HardMacroCluster, false, vec![]),
        (AreaKind::HardMacroCluster, true, vec![]),
    ];
    let get = tree_of(&nodes);
    let utils = utilization_list(0.25, 10);
    let tree = PlacementTree {
        kind: &|i| get(i).0,
        is_fixed_macro: &|i| get(i).1,
        is_leaf: &|i| get(i).2.is_empty(),
        children: &|i| get(i).2.clone(),
        utilizations: &utils,
        num_threads: 10,
    };

    let visits = place_children(&tree, 0, &mut |_, _, _| Some(macros(1)));
    assert_eq!(visits.len(), 5);
    assert!(matches!(visits[1].outcome, ParentOutcome::Leaf), "the IO cluster");
    assert!(matches!(visits[2].outcome, ParentOutcome::Leaf), "the leaf cell cluster");
    assert!(matches!(visits[3].outcome, ParentOutcome::MacroCluster));
    assert!(matches!(visits[4].outcome, ParentOutcome::FixedMacroCluster));
}

/// ⛔ **A parent that cannot be placed STOPS the walk there** — upstream's MPL-40 / MPL-8 throws,
/// so nothing below it runs.
#[test]
fn a_parent_that_cannot_be_placed_stops_the_walk() {
    let nodes: Vec<Node> = vec![
        (AreaKind::MixedCluster, false, vec![1]),
        (AreaKind::MixedCluster, false, vec![2]),
        (AreaKind::StdCellCluster, false, vec![]),
    ];
    let get = tree_of(&nodes);
    let utils = utilization_list(0.25, 10);
    let tree = PlacementTree {
        kind: &|i| get(i).0,
        is_fixed_macro: &|i| get(i).1,
        is_leaf: &|i| get(i).2.is_empty(),
        children: &|i| get(i).2.clone(),
        utilizations: &utils,
        num_threads: 10,
    };

    // The root places; its child never finds a valid solution.
    let visits = place_children(&tree, 0, &mut |c, _, _| (c == 0).then(|| macros(1)));

    assert_eq!(visits.len(), 2, "cluster 2 was never reached");
    match &visits[1].outcome {
        ParentOutcome::NoValidSolution(e) => {
            assert_eq!(e.code, 8, "MPL-8 below the root, not MPL-40");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// ⚠️ **The ROOT failing is MPL-40**, which blames the core utilization — something the user can
/// act on. Below it, MPL-8 asks for a bug report.
#[test]
fn the_root_failing_is_a_different_code() {
    let nodes: Vec<Node> = vec![(AreaKind::MixedCluster, false, vec![])];
    let get = tree_of(&nodes);
    let utils = utilization_list(0.25, 10);
    let tree = PlacementTree {
        kind: &|i| get(i).0,
        is_fixed_macro: &|i| get(i).1,
        // ⚠️ Deliberately NOT a leaf, so it reaches the placement path.
        is_leaf: &|_| false,
        children: &|i| get(i).2.clone(),
        utilizations: &utils,
        num_threads: 10,
    };
    let visits = place_children(&tree, 0, &mut |_, _, _| None);
    match &visits[0].outcome {
        ParentOutcome::NoValidSolution(e) => assert_eq!(e.code, 40),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// ⚠️ The winning run is recorded, including whether the utilization had to be adjusted.
#[test]
fn the_winning_run_is_recorded() {
    let nodes: Vec<Node> = vec![(AreaKind::MixedCluster, false, vec![])];
    let get = tree_of(&nodes);
    let utils = utilization_list(0.25, 10);
    let tree = PlacementTree {
        kind: &|i| get(i).0,
        is_fixed_macro: &|i| get(i).1,
        is_leaf: &|_| false,
        children: &|i| get(i).2.clone(),
        utilizations: &utils,
        num_threads: 10,
    };
    // Only the third utilization works.
    let visits = place_children(&tree, 0, &mut |_, i, _| (i == 2).then(|| macros(3)));
    match &visits[0].outcome {
        ParentOutcome::Placed { run, macros } => {
            assert_eq!(run.index, 2);
            assert!(run.utilization_was_adjusted, "not the utilization that was asked for");
            assert_eq!(macros.len(), 3);
        }
        other => panic!("expected a placement, got {other:?}"),
    }
}

// ---------------------------------------------------------------- the setup steps

/// ⚠️ **`adjustSoftBlockageWeight` runs before the tiny-cluster threshold**, and both before any
/// placement — so every annealer in the design is built with the same adjusted weight.
#[test]
fn the_soft_blockage_weight_is_adjusted_once_up_front() {
    let base = SoftWeights::placement_defaults();
    assert_eq!(base.soft_blockage, 10.0, "the command's value");

    let (adjusted, tiny) = placement_setup(1, base, 500_000);
    assert_eq!(adjusted.soft_blockage, 50.0, "half the outline weight, on a single-level tree");
    assert_eq!(tiny, 500);

    // A deeper tree leaves it alone.
    let (deeper, _) = placement_setup(2, base, 500_000);
    assert_eq!(deeper.soft_blockage, 10.0);
}

/// ⚠️ Nothing else moves.
#[test]
fn the_setup_touches_only_the_soft_blockage_weight() {
    let base = SoftWeights::placement_defaults();
    let (adjusted, _) = placement_setup(1, base, 0);
    assert_eq!(adjusted.outline, base.outline);
    assert_eq!(adjusted.fence, base.fence);
    assert_eq!(adjusted.notch, base.notch);
    assert_eq!(adjusted.boundary, base.boundary);
}
