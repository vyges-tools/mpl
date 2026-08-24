// SPDX-License-Identifier: Apache-2.0
//! Building the physical hierarchy. Rules from upstream `ClusteringEngine`.
use vyges_mpl::cluster::ClusterType;
use vyges_mpl::design::{Design, Instance, MasterKind, Module, ModuleMetrics, Rect};
use vyges_mpl::tree::TreeBuilder;

fn inst(name: &str, is_block: bool) -> Instance {
    Instance {
        name: name.into(),
        is_block,
        is_fixed: false,
        bbox: Rect { x_min: 0, y_min: 0, x_max: 10, y_max: 10 },
        master: MasterKind::default(),
        is_ignorable_macro: false,
    }
}

fn m(name: &str, insts: Vec<usize>, children: Vec<usize>) -> Module {
    Module { name: name.into(), hierarchical_name: format!("top/{name}"), insts, children }
}

fn design(instances: Vec<Instance>, modules: Vec<Module>) -> Design {
    Design {
        instances,
        modules,
        top: 0,
        core_area: Rect { x_min: 0, y_min: 0, x_max: 1000, y_max: 1000 },
        die_area: Rect { x_min: 0, y_min: 0, x_max: 1000, y_max: 1000 },
    }
}

fn metrics(std: i32, mac: i32) -> ModuleMetrics {
    ModuleMetrics { num_std_cell: std, num_macro: mac, std_cell_area: 0, macro_area: 0 }
}

// ------------------------------------------------------------------ ids

#[test]
fn ids_are_handed_out_in_creation_order() {
    // ⚠️ Observable: the id order decides tie-breaks and the order the annealer later sees.
    let d = design(vec![inst("a", false)], vec![m("top", vec![0], vec![1]), m("sub", vec![], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0), metrics(1, 0)]);
    let root = b.create_root();
    assert_eq!(root.id, 0);
    assert_eq!(b.create_cluster_for_module(1).unwrap().id, 1);
    assert_eq!(b.next_id(), 2);
}

// ------------------------------------------------------------------ empty modules

#[test]
fn a_module_with_no_instances_gets_no_cluster() {
    // 🔑 "Empty" counts INSTANCES only. A module holding nothing the placer cares about is
    // skipped, and skipping it changes the tree's shape rather than merely its contents.
    let d = design(vec![], vec![m("top", vec![], vec![1]), m("sub", vec![], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(0, 0), metrics(0, 0)]);
    assert!(b.create_cluster_for_module(1).is_none());
    assert_eq!(b.next_id(), 0, "and it does not consume an id");
}

#[test]
fn a_module_with_only_macros_still_gets_a_cluster() {
    let d = design(vec![inst("m", true)], vec![m("top", vec![], vec![1]), m("sub", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(0, 1), metrics(0, 1)]);
    assert!(b.create_cluster_for_module(1).is_some());
}

#[test]
fn a_cluster_is_named_by_the_modules_hierarchical_name() {
    // ⚠️ Not its leaf name: two modules with the same leaf name in different branches would
    // otherwise collide, and a tree keyed by name would silently merge them.
    let d = design(vec![inst("a", false)], vec![m("top", vec![], vec![1]), m("sub", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0), metrics(1, 0)]);
    assert_eq!(b.create_cluster_for_module(1).unwrap().name, "top/sub");
}

// ------------------------------------------------------------------ glue logic

#[test]
fn glue_logic_is_named_after_its_parent_in_parentheses() {
    // The nesting is upstream's, and it is how a real hierarchy dump reads:
    //   ((root)_glue_logic_0_1)_glue_logic_1_0
    let d = design(vec![inst("a", false)], vec![m("top", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0)]);
    assert_eq!(b.create_flat_cluster(0, "root").unwrap().name, "(root)_glue_logic");
}

#[test]
fn a_glue_cluster_with_no_leaves_is_DISCARDED() {
    // 🔑 Upstream builds it, finds it empty and drops it. One that survived would take an id,
    // occupy a slot in the tree and contribute nothing -- and it would shift every later id.
    let d = design(vec![], vec![m("top", vec![], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(0, 0)]);
    assert!(b.create_flat_cluster(0, "root").is_none());
    assert_eq!(b.next_id(), 0, "and it consumes no id");
}

#[test]
fn a_module_of_only_ignored_cells_produces_no_glue_cluster() {
    // Pads, covers and end-caps are skipped, so the cluster comes out empty and is dropped.
    let mut pad = inst("p", false);
    pad.master.is_pad = true;
    let d = design(vec![pad], vec![m("top", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(0, 0)]);
    assert!(b.create_flat_cluster(0, "root").is_none());
}

#[test]
fn glue_leaves_are_filed_by_whether_they_are_macros() {
    let d = design(
        vec![inst("s", false), inst("m", true), inst("s2", false)],
        vec![m("top", vec![0, 1, 2], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(2, 1)]);
    let c = b.create_flat_cluster(0, "root").unwrap();
    assert_eq!(c.leaf_std_cells, vec![0, 2]);
    assert_eq!(c.leaf_macros, vec![1]);
}

#[test]
fn glue_takes_only_the_modules_own_instances_not_its_children() {
    // ⚠️ Descending into children here would double-count them: they get their own clusters.
    let d = design(
        vec![inst("own", false), inst("childs", false)],
        vec![m("top", vec![0], vec![1]), m("sub", vec![1], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(2, 0), metrics(1, 0)]);
    let c = b.create_flat_cluster(0, "root").unwrap();
    assert_eq!(c.leaf_std_cells, vec![0], "only the top module's own instance");
}

// ------------------------------------------------------------------ metrics

#[test]
fn cluster_metrics_count_both_leaves_and_held_modules() {
    // ⚠️ Both, not either. A cluster can hold loose leaves and a module at once.
    let d = design(vec![inst("a", false), inst("m", true)], vec![m("top", vec![], vec![1]), m("sub", vec![], vec![])]);
    let b = TreeBuilder::new(&d, vec![metrics(0, 0), metrics(7, 3)]);
    let mut c = vyges_mpl::cluster::Cluster::new(0, "mixed");
    c.leaf_std_cells = vec![0];
    c.leaf_macros = vec![1];
    c.db_modules = vec![1];
    b.set_cluster_metrics(&mut c);
    assert_eq!(c.metrics.num_std_cell, 8, "1 leaf + 7 from the module");
    assert_eq!(c.metrics.num_macro, 4, "1 leaf + 3 from the module");
}

// ------------------------------------------------------------------ macro-only designs

#[test]
fn each_macro_becomes_its_own_hard_macro_cluster() {
    let d = design(
        vec![inst("m1", true), inst("s", false), inst("m2", true)],
        vec![m("top", vec![0, 1, 2], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 2)]);
    let cs = b.one_cluster_per_macro();
    assert_eq!(cs.len(), 2, "only the macros");
    assert_eq!(cs[0].name, "m1", "named after the instance");
    assert!(cs.iter().all(|c| c.cluster_type == ClusterType::HardMacro));
    assert_eq!(cs[0].leaf_macros, vec![0]);
}

#[test]
fn an_ignored_macro_does_not_become_its_own_cluster() {
    let mut mac = inst("m", true);
    mac.is_ignorable_macro = true;
    mac.is_fixed = true;
    let d = design(vec![mac], vec![m("top", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(0, 1)]);
    assert!(b.one_cluster_per_macro().is_empty());
}

#[test]
fn a_macro_cluster_carries_exactly_one_macro_and_no_cells() {
    // ⚠️ This does NOT test the type mask, though an earlier version claimed to. A cluster built
    // here has metrics {0 std cells, 1 macro}, so `num_std_cell()` reads 0 whether the type
    // masks it or not -- the assertion could not fail. Mutation testing said so: changing the
    // type to Mixed left this green and was caught by the type assertion above instead.
    //
    // The mask itself is tested where it is observable, in tests/cluster.rs.
    let d = design(vec![inst("m", true)], vec![m("top", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(0, 1)]);
    let cs = b.one_cluster_per_macro();
    assert_eq!(cs[0].metrics.num_macro, 1, "one macro");
    assert_eq!(cs[0].metrics.num_std_cell, 0, "and no standard cells, in the metrics themselves");
    assert_eq!(cs[0].leaf_macros.len(), 1);
}

// ------------------------------------------------------------------ break_cluster

use vyges_mpl::cluster::Cluster;

/// A parent cluster holding one module, as `createRoot` / `createCluster` would leave it.
fn holding(id: i32, name: &str, module: usize, std: i32, mac: i32) -> Cluster {
    let mut c = Cluster::new(id, name);
    c.db_modules.push(module);
    c.metrics = vyges_mpl::cluster::Metrics { num_std_cell: std, num_macro: mac , ..Default::default() };
    c
}

#[test]
fn an_empty_cluster_produces_no_children() {
    // ℹ️ The early return for an empty cluster is an OPTIMISATION, not a behaviour: with no
    // leaves and no modules, neither branch can produce anything anyway. Mutation testing
    // showed removing the guard changes nothing, so this asserts the outcome rather than
    // pretending to pin the guard.
    let d = design(vec![], vec![m("top", vec![], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(0, 0)]);
    let mut c = Cluster::new(0, "empty");
    b.break_cluster(&mut c, true, 10, 5, 1, 1);
    assert!(c.children.is_empty());
}

#[test]
fn a_glue_child_with_no_module_is_never_recursed_into() {
    // ⛔ The module check is what makes the recursion TERMINATE, not merely an optimisation.
    // A cluster with no module takes the merged branch, which copies its own leaves into a new
    // glue child of the same size and with no module -- so recursing into it regenerates it
    // forever. Measured: removing the check overflows the stack rather than failing an assertion,
    // which is also how the harness learned to judge by exit code instead of by grep.
    let d = design(
        vec![inst("g1", false), inst("g2", false), inst("g3", false), inst("a", false)],
        vec![m("top", vec![0, 1, 2], vec![1]), m("s1", vec![3], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(4, 0), metrics(1, 0)]);
    let mut root = holding(99, "root", 0, 4, 0);
    // max_std_cell = 1, so the 3-cell glue child is "too big" -- and must still be left alone.
    b.break_cluster(&mut root, true, 1, 100, 0, 0);
    let glue = root
        .children
        .iter()
        .find(|c| c.name == "(root)_glue_logic")
        .expect("a glue child was created");
    assert!(glue.db_modules.is_empty(), "it holds no module");
    assert_eq!(glue.leaf_std_cells.len(), 3, "and is over the maximum");
    assert!(glue.children.is_empty(), "yet it is NOT broken further");
}

#[test]
fn a_recursed_child_is_never_treated_as_the_root() {
    // ⚠️ The second hole. `is_root` decides whether a FLAT module becomes a glue child or is
    // absorbed. A recursive call passing `true` would give every flat descendant a glue child
    // instead of absorbing it -- a different tree, and one no golden would explain.
    //
    // s1 is flat (no child modules) and too big, so recursion enters it; being a descendant it
    // must be ABSORBED, not given a child.
    let d = design(
        vec![inst("a", false), inst("b", false)],
        vec![m("top", vec![], vec![1]), m("s1", vec![0, 1], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(2, 0), metrics(2, 0)]);
    let mut root = holding(99, "root", 0, 2, 0);
    b.break_cluster(&mut root, true, 1, 100, 0, 0);
    let s1 = &root.children[0];
    assert!(s1.children.is_empty(), "absorbed, so no glue child");
    assert!(s1.db_modules.is_empty(), "and it stopped being a module cluster");
    assert_eq!(s1.leaf_std_cells.len(), 2, "its instances came to it");
}

#[test]
fn a_flat_module_at_the_ROOT_gets_a_glue_child() {
    // 🔑 The root keeps its module and gains one cluster holding the instances.
    let d = design(vec![inst("a", false)], vec![m("top", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0)]);
    let mut root = holding(0, "root", 0, 1, 0);
    b.break_cluster(&mut root, true, 10, 5, 1, 1);
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.children[0].name, "(root)_glue_logic");
    assert_eq!(root.db_modules, vec![0], "the root keeps its module");
}

#[test]
fn the_SAME_flat_module_below_the_root_is_absorbed_instead() {
    // 🔑 Same input, different tree. Not bookkeeping -- `is_root` selects between producing a
    // child and dissolving the module into this cluster.
    let d = design(vec![inst("a", false)], vec![m("top", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0)]);
    let mut sub = holding(0, "sub", 0, 1, 0);
    b.break_cluster(&mut sub, false, 10, 5, 1, 1);
    assert!(sub.children.is_empty(), "no child is created");
    assert!(sub.db_modules.is_empty(), "and it stops being a module cluster");
    assert_eq!(sub.leaf_std_cells, vec![0], "the instances are absorbed into it");
}

#[test]
fn a_module_with_children_yields_one_cluster_each_then_the_glue() {
    // ⚠️ Order matters: the glue cluster's id follows the child modules'.
    let d = design(
        vec![inst("own", false), inst("a", false), inst("b", false)],
        vec![
            m("top", vec![0], vec![1, 2]),
            m("s1", vec![1], vec![]),
            m("s2", vec![2], vec![]),
        ],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(3, 0), metrics(1, 0), metrics(1, 0)]);
    let mut root = holding(99, "root", 0, 3, 0);
    b.break_cluster(&mut root, true, 100, 100, 0, 0);
    let names: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["top/s1", "top/s2", "(root)_glue_logic"]);
    assert!(root.children[0].id < root.children[2].id, "glue is created last");
}

#[test]
fn a_module_with_children_but_no_own_instances_gets_no_glue_cluster() {
    let d = design(
        vec![inst("a", false)],
        vec![m("top", vec![], vec![1]), m("s1", vec![0], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0), metrics(1, 0)]);
    let mut root = holding(99, "root", 0, 1, 0);
    b.break_cluster(&mut root, true, 100, 100, 0, 0);
    let names: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["top/s1"], "no empty glue cluster survives");
}

#[test]
fn a_merged_cluster_splits_by_module_and_then_by_its_own_leaves() {
    let d = design(
        vec![inst("loose", false), inst("a", false)],
        vec![m("top", vec![0], vec![1]), m("s1", vec![1], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(2, 0), metrics(1, 0)]);
    // A merged cluster: holds a module AND loose leaves, so it does NOT correspond to a module.
    let mut merged = Cluster::new(50, "merged");
    merged.db_modules.push(1);
    merged.leaf_std_cells.push(0);
    merged.metrics = vyges_mpl::cluster::Metrics { num_std_cell: 2, num_macro: 0 , ..Default::default() };
    b.break_cluster(&mut merged, false, 100, 100, 0, 0);
    let names: Vec<&str> = merged.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["top/s1", "(merged)_glue_logic"]);
}

#[test]
fn recursion_only_enters_children_that_are_too_big_AND_hold_a_module() {
    // ⚠️ A child with no module is skipped however large: there is nothing to split it on, and
    // it is left for flat partitioning instead.
    let d = design(
        vec![inst("a", false), inst("b", false)],
        vec![
            m("top", vec![], vec![1]),
            m("s1", vec![0], vec![2]),
            m("s2", vec![1], vec![]),
        ],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(2, 0), metrics(2, 0), metrics(1, 0)]);
    let mut root = holding(99, "root", 0, 2, 0);
    // max_std_cell = 1, so s1 (2 cells) is too big and is broken further.
    b.break_cluster(&mut root, true, 1, 100, 0, 0);
    assert_eq!(root.children.len(), 1, "one child module");
    assert!(!root.children[0].children.is_empty(), "s1 was broken further");
}

#[test]
fn a_child_that_fits_is_left_alone() {
    let d = design(
        vec![inst("a", false)],
        vec![m("top", vec![], vec![1]), m("s1", vec![0], vec![2]), m("s2", vec![], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0), metrics(1, 0), metrics(0, 0)]);
    let mut root = holding(99, "root", 0, 1, 0);
    b.break_cluster(&mut root, true, 100, 100, 0, 0);
    assert!(root.children[0].children.is_empty(), "it fits, so it is not broken");
}

#[test]
fn small_children_are_reported_in_child_order_and_not_merged() {
    // ⚠️ Identified, not merged: merging consults net connectivity, a separate stage. Reporting
    // them keeps the decision visible instead of silently skipped.
    let d = design(
        vec![inst("a", false), inst("b", false)],
        vec![m("top", vec![], vec![1, 2]), m("s1", vec![0], vec![]), m("s2", vec![1], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(2, 0), metrics(1, 0), metrics(1, 0)]);
    let mut root = holding(99, "root", 0, 2, 0);
    let out = b.break_cluster(&mut root, true, 100, 100, 5, 5);
    assert_eq!(out.merge_candidates.len(), 2, "both children are below the minimum");
    assert_eq!(
        out.merge_candidates,
        root.children.iter().map(|c| c.id).collect::<Vec<_>>(),
        "in child order"
    );
    assert_eq!(root.children.len(), 2, "and nothing was actually merged away");
}

#[test]
fn a_child_above_the_minimum_is_not_a_merge_candidate() {
    let d = design(
        vec![inst("a", false), inst("b", false)],
        vec![m("top", vec![], vec![1, 2]), m("s1", vec![0], vec![]), m("s2", vec![1], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(2, 0), metrics(9, 0), metrics(1, 0)]);
    let mut root = holding(99, "root", 0, 2, 0);
    let out = b.break_cluster(&mut root, true, 100, 100, 5, 5);
    assert_eq!(out.merge_candidates.len(), 1, "only the small one");
}

// ------------------------------------------------------------------ the autocluster descent

use vyges_mpl::thresholds::Thresholds;

fn t(max_std: i32, min_std: i32, max_mac: i32, min_mac: i32) -> Thresholds {
    Thresholds {
        max_std_cell: max_std,
        min_std_cell: min_std,
        max_macro: max_mac,
        min_macro: min_mac,
    }
}

#[test]
fn a_descent_that_reaches_the_level_limit_stops() {
    let d = design(vec![inst("a", false)], vec![m("top", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0)]);
    let mut root = holding(0, "root", 0, 20, 0);
    // ⚠️ The thresholds matter: with generous ones, continuing past the limit would split nothing
    // anyway and the test could not tell `>=` from `>`. Here level 2 would give a maximum of 1,
    // which the 20-cell root exceeds -- so an off-by-one WOULD split, and this catches it.
    let out = b.multilevel_autocluster(&mut root, true, 1, t(10, 10, 10, 10), 1, 10.0);
    assert!(root.children.is_empty(), "the level limit stopped it before any split");
    assert!(out.needs_partitioning.is_empty());
}

#[test]
fn a_root_smaller_than_a_leaf_is_force_split_anyway() {
    // 🔑 force_split_root: the root has fewer cells than a LEAF cluster may hold, so leaving it
    // whole would hand the placer one cluster. It is split despite being under the maximum.
    let d = design(
        vec![inst("a", false)],
        vec![m("top", vec![0], vec![1]), m("s1", vec![], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0), metrics(0, 0)]);
    let mut root = holding(0, "root", 0, 1, 0);
    // base max 5000, max_level 1 -> leaf max = 5000; root has 1 cell, far below.
    let out = b.multilevel_autocluster(&mut root, true, 0, t(5000, 1000, 5, 1), 1, 10.0);
    assert!(!root.children.is_empty(), "split despite being tiny");
    assert!(out.needs_partitioning.is_empty());
}

#[test]
fn force_split_measures_against_the_LEAF_maximum_not_the_base_one() {
    // ⚠️ With max_level == 1 the divisor is ratio^0 == 1 and the two readings AGREE, so the test
    // above cannot tell them apart. Separating them takes more than a deeper hierarchy, because
    // the descent eventually breaks the root either way -- what differs is WHICH LEVEL it breaks
    // at, and therefore which thresholds that break uses.
    //
    // Root holds 200 cells, base maximum 5000, ratio 10, max_level 3:
    //   correct  -- leaf maximum is 5000/100 = 50, and 200 is ABOVE it, so no force split. The
    //               descent runs to level 3, where the minimum is 10 and a 200-cell child is
    //               NOT a merge candidate.
    //   mutated  -- leaf maximum read as the base 5000, 200 is below it, so it force splits at
    //               level 1, where the minimum is 1000 and the same child IS a merge candidate.
    let d = design(
        vec![inst("a", false)],
        vec![m("top", vec![], vec![1]), m("s1", vec![0], vec![])],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(200, 0), metrics(200, 0)]);
    let mut root = holding(0, "root", 0, 200, 0);
    let out = b.multilevel_autocluster(&mut root, true, 0, t(5000, 1000, 5000, 1000), 3, 10.0);
    assert!(!root.children.is_empty(), "it is broken -- the question is at which level");
    assert!(
        out.merge_candidates.is_empty(),
        "broken at the DEEP level, where 200 cells clear the minimum: {:?}",
        out.merge_candidates
    );
}

#[test]
fn force_split_is_only_considered_at_the_top() {
    // ⚠️ At any level below 0 the flag is not even computed, so a small child is left alone.
    let d = design(vec![inst("a", false)], vec![m("top", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0)]);
    let mut sub = holding(0, "sub", 0, 1, 0);
    // Level 1 of 3, tiny cluster, generous maximums -> nothing to do.
    b.multilevel_autocluster(&mut sub, false, 1, t(5000, 1000, 5, 1), 3, 10.0);
    assert!(sub.children.is_empty(), "a small non-root is not force split");
    // ⚠️ "No children" alone cannot catch this: sub holds a FLAT module, and the flat branch at a
    // non-root ABSORBS rather than creating a child -- so a wrongly-forced split would also leave
    // no children. The absorption is the observable half.
    assert_eq!(sub.db_modules, vec![0], "and its module is untouched");
    assert!(sub.leaf_std_cells.is_empty(), "nothing was absorbed into it");
}

#[test]
fn a_cluster_that_fits_descends_a_level_WITHOUT_splitting() {
    // 🔑 The `else` branch recurses on the SAME cluster with the level incremented -- it does
    // not descend into children. Recursing on children there (the obvious reading) would skip
    // levels and produce a different tree. Observable as: it terminates, and splits nothing.
    let d = design(vec![inst("a", false)], vec![m("top", vec![0], vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(1, 0)]);
    let mut sub = holding(0, "sub", 0, 1, 0);
    // Starts at level 1 so force_split_root is off; fits comfortably; max_level 4.
    b.multilevel_autocluster(&mut sub, false, 1, t(5000, 1000, 5, 1), 4, 10.0);
    assert!(sub.children.is_empty(), "descended without splitting, and terminated");
}

#[test]
fn a_cluster_over_the_maximum_is_split_and_its_children_visited() {
    let d = design(
        vec![inst("a", false), inst("b", false)],
        vec![
            m("top", vec![], vec![1, 2]),
            m("s1", vec![0], vec![]),
            m("s2", vec![1], vec![]),
        ],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(2, 0), metrics(1, 0), metrics(1, 0)]);
    let mut root = holding(0, "root", 0, 2, 0);
    // ⚠️ Choosing thresholds here takes care, and my first attempt was wrong. With
    // `t(10, 1, ...)` the level-2 minimum divides to 0, the degenerate floor raises it to 100,
    // and the MAXIMUM is then recomputed as 500 -- so nothing exceeds it and nothing splits.
    // Base minimums must stay above the ratio for the level scaling to be the thing under test.
    // Here level 2 gives max 1 / min 1, and the root's 2 cells exceed it.
    //
    // ⚠️ And max_level must leave ROOM BELOW: with max_level 2 the recursive call into each child
    // starts at level 2 and returns immediately, so `is_root` is never read and a leaked value
    // cannot be observed. max_level 3 gives the recursion something to do.
    b.multilevel_autocluster(&mut root, true, 1, t(10, 10, 10, 10), 3, 10.0);
    let names: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["top/s1", "top/s2"]);
    // ⚠️ The GRANDCHILDREN are where a leaked `is_root` shows up: each child holds a flat module,
    // which a non-root absorbs but a root would give a glue child. Asserting only the child names
    // cannot see that.
    for child in &root.children {
        assert!(child.children.is_empty(), "{} was absorbed, not given a glue child", child.name);
    }
}

#[test]
fn a_recursed_child_that_BREAKS_is_not_treated_as_the_root() {
    // ⚠️ `is_root` is only READ inside break_cluster's flat-module branch, so the recursed child
    // has to actually BREAK there. Three earlier attempts missed it: a child that returns at the
    // level limit, or fits under its maximum, passes the flag along and never uses it.
    //
    // Base 1000 / ratio 10 / max_level 3. Level 2 gives a maximum of 100, which the 200-cell root
    // exceeds; its children then descend to level 3 where the maximum is 10.
    //
    // 🔑 The two children behave DIFFERENTLY, and that is upstream's design rather than a defect:
    // break_cluster has its OWN recursion into oversized children, so s1 (150 > 100) is already
    // broken and absorbed inside the root's break, and by the time the descent reaches it, it is a
    // merged cluster taking the other branch. s2 (50) is not, so it still holds its module when
    // the descent reaches it -- which is exactly where `is_root` is read.
    let d = design(
        vec![inst("a", false), inst("b", false), inst("c", false)],
        vec![
            m("top", vec![], vec![1, 2]),
            m("s1", vec![0, 1], vec![]),
            m("s2", vec![2], vec![]),
        ],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(200, 0), metrics(150, 0), metrics(50, 0)]);
    let mut root = holding(0, "root", 0, 200, 0);
    b.multilevel_autocluster(&mut root, true, 1, t(1000, 1000, 1000, 1000), 3, 10.0);

    let s2 = root.children.iter().find(|c| c.name.starts_with("top/s2")).expect("s2 survived");
    assert!(s2.children.is_empty(), "a recursed child ABSORBS its flat module, it is not the root");
    assert!(s2.db_modules.is_empty(), "and the module reference is dropped");
    assert_eq!(s2.leaf_std_cells.len(), 1, "its instance came to it");
}

#[test]
fn a_flat_cluster_needing_the_partitioner_is_reported_up_the_descent() {
    // ⛔ Stage 1 refuses on this. It must survive the recursion rather than being lost in a
    // child's outcome -- a refusal that does not reach the caller is a silent approximation.
    let insts: Vec<_> = (0..30).map(|i| inst(&format!("i{i}"), false)).collect();
    let d = design(insts, vec![m("top", (0..30).collect(), vec![])]);
    let mut b = TreeBuilder::new(&d, vec![metrics(30, 0)]);
    let mut root = holding(0, "root", 0, 30, 0);
    // Root is force split (30 < leaf max), producing a glue child with 30 leaves and no
    // module -- which is exactly a large flat cluster once the maximum is 5.
    let out = b.multilevel_autocluster(&mut root, true, 0, t(5, 1, 5, 1), 1, 10.0);
    assert!(!out.needs_partitioning.is_empty(), "the refusal reached the caller");
}

#[test]
fn merge_candidates_are_reported_per_parent() {
    let d = design(
        vec![inst("a", false), inst("b", false)],
        vec![
            m("top", vec![], vec![1, 2]),
            m("s1", vec![0], vec![]),
            m("s2", vec![1], vec![]),
        ],
    );
    let mut b = TreeBuilder::new(&d, vec![metrics(2, 0), metrics(1, 0), metrics(1, 0)]);
    let mut root = holding(0, "root", 0, 2, 0);
    let out = b.multilevel_autocluster(&mut root, true, 1, t(10, 50, 10, 50), 2, 10.0);
    assert!(!out.merge_candidates.is_empty(), "both children are below the minimum");
    assert_eq!(out.merge_candidates[0].0, 0, "reported against their parent");
}

