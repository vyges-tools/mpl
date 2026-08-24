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
