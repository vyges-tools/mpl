// SPDX-License-Identifier: Apache-2.0
//! Committing the clustering data to the database as nested groups.

use vyges_mpl::placement::{create_groups, AreaKind, GroupCluster};

fn cluster(name: &str, kind: AreaKind) -> GroupCluster {
    GroupCluster {
        name: name.into(),
        kind,
        leaf_std_cells: Vec::new(),
        leaf_macros: Vec::new(),
        module_insts: Vec::new(),
        children: Vec::new(),
    }
}

/// 🔑 **The recursion sits BETWEEN the two claiming phases**, so a child claims its instances
/// before the parent sweeps its modules. Sweeping first would put every descendant's instances in
/// the ancestor's group.
#[test]
fn a_child_claims_its_instances_before_the_parent_sweeps() {
    let mut root = cluster("root", AreaKind::MixedCluster);
    // The root's module holds instances 1 and 2; the child names 1 explicitly.
    root.module_insts = vec![(1, false), (2, false)];
    root.children = vec![1];

    let mut child = cluster("child", AreaKind::StdCellCluster);
    child.leaf_std_cells = vec![1];

    let got = create_groups(&[root, child], 0);
    assert_eq!(got[0], ("root".to_string(), vec![2]), "the root kept only what was left");
    assert_eq!(got[1], ("child".to_string(), vec![1]));
}

/// ⛔ **An IO cluster gets NO group at all**, and its subtree is not visited — the early return
/// happens before the group is created.
#[test]
fn an_io_cluster_gets_no_group_and_is_not_descended_into() {
    let mut root = cluster("root", AreaKind::MixedCluster);
    root.children = vec![1];

    let mut io = cluster("io", AreaKind::IoCluster);
    io.leaf_std_cells = vec![5];
    io.children = vec![2];

    let mut buried = cluster("buried", AreaKind::StdCellCluster);
    buried.leaf_std_cells = vec![6];

    let got = create_groups(&[root, io, buried], 0);
    let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["root"], "neither the IO cluster nor its child");
}

/// ⛔ **A standard-cell cluster does not pick up a macro from a MODULE**, but it will keep one it
/// names in its own macro list.
#[test]
fn a_std_cell_cluster_skips_macros_from_modules_only() {
    let mut from_module = cluster("cells", AreaKind::StdCellCluster);
    from_module.module_insts = vec![(1, true), (2, false)];
    let got = create_groups(&[from_module], 0);
    assert_eq!(got[0].1, vec![2], "the block was skipped");

    let mut named = cluster("cells", AreaKind::StdCellCluster);
    named.leaf_macros = vec![1];
    let got = create_groups(&[named], 0);
    assert_eq!(got[0].1, vec![1], "but a macro it names directly is kept");
}

/// ⚠️ A mixed cluster's module sweep does keep macros.
#[test]
fn a_mixed_cluster_keeps_macros_from_modules() {
    let mut mixed = cluster("mixed", AreaKind::MixedCluster);
    mixed.module_insts = vec![(1, true), (2, false)];
    let got = create_groups(&[mixed], 0);
    assert_eq!(got[0].1, vec![1, 2]);
}

/// ⚠️ **An instance is claimed at most once.** Two clusters naming the same instance: the first to
/// reach it wins.
#[test]
fn an_instance_is_claimed_only_once() {
    let mut root = cluster("root", AreaKind::MixedCluster);
    root.leaf_std_cells = vec![9];
    root.children = vec![1];

    let mut child = cluster("child", AreaKind::StdCellCluster);
    child.leaf_std_cells = vec![9];

    let got = create_groups(&[root, child], 0);
    assert_eq!(got[0].1, vec![9], "the root reached it first");
    assert!(got[1].1.is_empty());
}

/// ⚠️ Standard cells are claimed before macros, within one cluster.
#[test]
fn cells_are_claimed_before_macros() {
    let mut c = cluster("c", AreaKind::MixedCluster);
    c.leaf_std_cells = vec![3];
    c.leaf_macros = vec![1];
    let got = create_groups(&[c], 0);
    assert_eq!(got[0].1, vec![3, 1], "cells first, whatever the instance numbers");
}

/// ⚠️ Groups appear in depth-first order, mirroring the cluster tree.
#[test]
fn groups_mirror_the_cluster_tree_depth_first() {
    let mut root = cluster("root", AreaKind::MixedCluster);
    root.children = vec![1, 3];
    let mut a = cluster("a", AreaKind::MixedCluster);
    a.children = vec![2];
    let b = cluster("a_child", AreaKind::StdCellCluster);
    let c = cluster("c", AreaKind::StdCellCluster);

    let got = create_groups(&[root, a, b, c], 0);
    let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["root", "a", "a_child", "c"]);
}

/// ℹ️ A cluster with nothing in it still gets a group.
#[test]
fn an_empty_cluster_still_gets_a_group() {
    let got = create_groups(&[cluster("empty", AreaKind::MixedCluster)], 0);
    assert_eq!(got, vec![("empty".to_string(), Vec::new())]);
}
