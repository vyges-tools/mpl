// SPDX-License-Identifier: Apache-2.0
//! Reading a real design. ⚠️ The only tests in this crate that need a database — everything else
//! works on plain data, which is the point of keeping the reader logic-free.
#![cfg(unix)]

use vyges_mpl::design::{compute_module_metrics, floorplan_shape, unfixed_macros};
use vyges_mpl::read::{mark_ignorable_macros, read_design};
use vyges_opendb::Db;

const FIXTURE: &str = "tests/fixtures/counter.odb";

fn open() -> Db {
    Db::open(FIXTURE).expect("fixture reads")
}

#[test]
fn a_design_reads_with_instances_and_a_module_hierarchy() {
    let d = read_design(&open()).expect("read");
    assert!(!d.instances.is_empty(), "the design has instances");
    assert!(!d.modules.is_empty(), "and at least a top module");
    assert_eq!(d.top, 0, "the top module is the first one recorded");
    assert!(d.core_area.area() > 0, "a core area was read");
}

#[test]
fn every_instance_has_a_real_bounding_box() {
    // ⚠️ A zero-area box would read as a cell occupying nothing, which silently understates
    // every occupancy check downstream.
    let d = read_design(&open()).expect("read");
    for i in &d.instances {
        assert!(i.bbox.area() > 0, "{} has an empty bbox", i.name);
        assert!(i.bbox.x_max > i.bbox.x_min && i.bbox.y_max > i.bbox.y_min, "{} inverted", i.name);
    }
}

#[test]
fn no_instance_belongs_to_two_modules() {
    // ⚠️ "Exactly one" would be wrong, and asserting it was: physical-only cells -- tapcells,
    // decaps, end-caps inserted by a physical tool -- belong to NO logical module. Measured on
    // this fixture: 229 instances in the block, 47 owned by the top module, 182 physical-only.
    // The invariant that does hold is that none belongs to more than one.
    let d = read_design(&open()).expect("read");
    let mut seen = vec![0usize; d.instances.len()];
    for m in &d.modules {
        for &i in &m.insts {
            seen[i] += 1;
        }
    }
    let dupes: Vec<&str> =
        seen.iter().enumerate().filter(|(_, &c)| c > 1).map(|(i, _)| d.instances[i].name.as_str()).collect();
    assert!(dupes.is_empty(), "instances in several modules: {dupes:?}");
    assert!(seen.iter().any(|&c| c == 0), "and some belong to none -- the physical cells");
}

#[test]
fn the_module_walk_and_the_block_are_different_scopes() {
    // 🔑 Both are correct and both are needed. Reading one where the other is meant is the bug
    // this test exists to make visible.
    let d = read_design(&open()).expect("read");
    let owned = d.module_owned_instances();
    assert!(owned.len() < d.instances.len(), "the netlist is a subset of the block");
    assert!(!owned.is_empty(), "but not an empty one");
    let mut sorted = owned.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), owned.len(), "the walk visits each instance once");
}

#[test]
fn the_module_hierarchy_is_a_tree_with_no_cycles() {
    let d = read_design(&open()).expect("read");
    let mut seen = vec![0usize; d.modules.len()];
    for m in &d.modules {
        for &c in &m.children {
            seen[c] += 1;
        }
    }
    assert_eq!(seen[d.top], 0, "the top module is nobody's child");
    assert!(seen.iter().skip(1).all(|&c| c == 1), "every other module has exactly one parent");
}

#[test]
fn module_metrics_account_for_every_module_owned_instance() {
    // 🔑 The cross-check that matters: within the MODULE scope, macros + standard cells +
    // ignored + errors must equal the total. A rule that counted an instance twice, or not at
    // all, shows up here as arithmetic rather than as a misplaced macro much later.
    //
    // ⚠️ Scoped to module-owned instances deliberately. Comparing against the whole block was
    // the first version of this test and it failed for a reason that had nothing to do with
    // the metrics: physical-only cells belong to no module.
    let d = read_design(&open()).expect("read");
    let area = floorplan_shape(&d.core_area, None).expect("a core to place into");
    let mut errs = Vec::new();
    let m = compute_module_metrics(&d, d.top, &area, &mut errs);

    let owned = d.module_owned_instances();
    let ignored = owned
        .iter()
        .filter(|&&i| !d.instances[i].is_block && vyges_mpl::design::is_ignored_inst(&d.instances[i]))
        .count();

    assert_eq!(
        m.num_std_cell as usize + m.num_macro as usize + ignored + errs.len(),
        owned.len(),
        "every module-owned instance is counted exactly once: macro, standard cell, ignored or error"
    );
}

#[test]
fn marking_ignorable_macros_only_touches_fixed_macros_outside_the_area() {
    let mut d = read_design(&open()).expect("read");
    let area = floorplan_shape(&d.core_area, None).expect("core");
    let names = mark_ignorable_macros(&mut d, &area);
    for name in &names {
        let i = d.instances.iter().find(|i| &i.name == name).unwrap();
        assert!(i.is_block && i.is_fixed, "{name} is a fixed macro");
        assert!(!i.bbox.overlaps(&area), "{name} really is outside the placement area");
    }
    // ...and nothing else got marked.
    let marked = d.instances.iter().filter(|i| i.is_ignorable_macro).count();
    assert_eq!(marked, names.len());
}

#[test]
fn a_fence_outside_the_core_leaves_nothing_to_place_into() {
    let d = read_design(&open()).expect("read");
    let far = vyges_mpl::design::Rect {
        x_min: d.die_area.x_max + 1_000_000,
        y_min: d.die_area.y_max + 1_000_000,
        x_max: d.die_area.x_max + 2_000_000,
        y_max: d.die_area.y_max + 2_000_000,
    };
    assert!(floorplan_shape(&d.core_area, Some(&far)).is_none());
}

#[test]
fn unfixed_macros_are_a_subset_of_the_macros() {
    let d = read_design(&open()).expect("read");
    for &i in &unfixed_macros(&d) {
        assert!(d.instances[i].is_block && !d.instances[i].is_fixed);
    }
}
