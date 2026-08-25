// SPDX-License-Identifier: Apache-2.0
//! The feasibility checks `init` runs before clustering. Rules from upstream `ClusteringEngine`.
use vyges_mpl::design::{Design, Instance, MasterKind, Module, Rect};
use vyges_mpl::feasibility::{
    instance_area_with_halos_fits, macro_fits_in_core, movable_cells_fit, union_area,
};

fn r(x0: i64, y0: i64, x1: i64, y1: i64) -> Rect {
    Rect { x_min: x0, y_min: y0, x_max: x1, y_max: y1 }
}

fn inst(name: &str, is_block: bool, is_fixed: bool, bbox: Rect) -> Instance {
    Instance {
        name: name.into(),
        is_block,
        is_fixed,
        bbox,
        master: MasterKind::default(),
        is_ignorable_macro: false,
    }
}

fn design(instances: Vec<Instance>) -> Design {
    Design {
        instances,
        modules: vec![Module {
            name: "top".into(),
            hierarchical_name: "top".into(),
            insts: vec![],
            children: vec![],
        }],
        top: 0,
        core_area: r(0, 0, 100, 100),
        die_area: r(0, 0, 100, 100),
    }
}

// ------------------------------------------------------------------ blockage union

#[test]
fn overlapping_blockages_occupy_their_area_once() {
    // 🔑 Upstream unions the blockages with a polygon set before adding their area. ⚠️ Summing
    // them instead double-counts every overlap, and refuses designs that fit.
    assert_eq!(union_area(&[r(0, 0, 10, 10), r(5, 0, 15, 10)]), 150);
    assert_eq!(union_area(&[r(0, 0, 10, 10), r(0, 0, 10, 10)]), 100, "identical is not double");
}

#[test]
fn disjoint_blockages_add_up() {
    assert_eq!(union_area(&[r(0, 0, 10, 10), r(50, 50, 60, 60)]), 200);
}

#[test]
fn a_blockage_nested_inside_another_adds_nothing() {
    assert_eq!(union_area(&[r(0, 0, 20, 20), r(5, 5, 10, 10)]), 400);
}

#[test]
fn an_empty_blockage_list_occupies_nothing() {
    assert_eq!(union_area(&[]), 0);
}

// ------------------------------------------------------------------ MPL-65

#[test]
fn a_fixed_cell_outside_the_area_contributes_only_the_part_inside() {
    // 🔑 Upstream intersects rather than skipping: a fixed cell may sit wholly outside the
    // placement area, and a physical marker may straddle its edge. Counting the whole box would
    // refuse designs that fit; skipping it would accept designs that do not.
    let d = design(vec![inst("fixed", false, true, r(90, 0, 110, 10))]);
    let area = r(0, 0, 100, 100);
    assert!(movable_cells_fit(&d, &area, &|_| 0, &[]));

    // Exactly the 10x10 inside, so a placement area of that size is full but not overfull.
    let tight = r(90, 0, 100, 10);
    assert!(movable_cells_fit(&d, &tight, &|_| 0, &[]));
}

#[test]
fn a_fixed_cell_entirely_outside_the_area_contributes_nothing() {
    let d = design(vec![inst("fixed", false, true, r(200, 200, 300, 300))]);
    assert!(movable_cells_fit(&d, &r(0, 0, 10, 10), &|_| 0, &[]));
}

#[test]
fn an_unfixed_macro_is_measured_with_its_halo_not_its_box() {
    // ⚠️ The whole point of running this after `createHardMacros`: the macro's bounding box would
    // fit and the same macro with its halo does not.
    let d = design(vec![inst("m", true, false, r(0, 0, 50, 50))]);
    let area = r(0, 0, 100, 100);
    assert!(movable_cells_fit(&d, &area, &|_| 2500, &[]), "the bare macro fits");
    assert!(!movable_cells_fit(&d, &area, &|_| 10_001, &[]), "with its halo it does not");
}

#[test]
fn an_ordinary_cell_contributes_its_bounding_box() {
    let d = design(vec![inst("c", false, false, r(0, 0, 10, 10))]);
    assert!(movable_cells_fit(&d, &r(0, 0, 10, 10), &|_| 0, &[]));
    assert!(!movable_cells_fit(&d, &r(0, 0, 9, 10), &|_| 0, &[]));
}

#[test]
fn every_instance_counts_including_ones_the_engine_otherwise_ignores() {
    // ⚠️ Unlike almost every other rule in the engine, this one has NO ignored-instance filter:
    // a tapcell occupies area whether or not the placer cares about it.
    let mut tap = inst("tap", false, false, r(0, 0, 10, 10));
    tap.master = MasterKind { is_end_cap: true, ..MasterKind::default() };
    let d = design(vec![tap]);
    assert!(!movable_cells_fit(&d, &r(0, 0, 9, 10), &|_| 0, &[]));
}

#[test]
fn blockages_are_clipped_to_the_placement_area_before_they_count() {
    // ⚠️ A blockage reaching far outside the area must contribute only the part inside. Counting
    // its whole box would refuse almost any design with a die-sized blockage.
    let area = r(0, 0, 10, 10);
    let one_cell = design(vec![inst("c", false, false, r(0, 0, 1, 1))]);

    // The blockage covers the area exactly, which alone is still a fit — the test is `<=`.
    assert!(movable_cells_fit(&design(vec![]), &area, &|_| 0, &[r(-100, -100, 100, 100)]));
    // One more cell on top of a full area is not.
    assert!(!movable_cells_fit(&one_cell, &area, &|_| 0, &[r(-100, -100, 100, 100)]));
    // Clipped to 5x10 = 50, leaving room for the cell.
    assert!(movable_cells_fit(&one_cell, &area, &|_| 0, &[r(5, 0, 15, 10)]));
}

#[test]
fn exactly_filling_the_area_still_fits() {
    // 🔑 The comparison is `<=`. A design that exactly fills its placement area is legal.
    let d = design(vec![inst("c", false, false, r(0, 0, 10, 10))]);
    assert!(movable_cells_fit(&d, &r(0, 0, 10, 10), &|_| 0, &[]));
}

// ------------------------------------------------------------------ MPL-16 and MPL-6

#[test]
fn the_halo_area_test_adds_the_macros_to_the_standard_cells() {
    assert!(instance_area_with_halos_fits(400, 600, 1000));
    assert!(!instance_area_with_halos_fits(400, 601, 1000));
}

#[test]
fn a_macro_exactly_as_wide_as_the_core_fits() {
    // ⚠️ Upstream errors only when the macro is STRICTLY larger; equal is allowed.
    let core = r(0, 0, 100, 200);
    assert!(macro_fits_in_core(100, 200, &core));
    assert!(!macro_fits_in_core(101, 200, &core));
    assert!(!macro_fits_in_core(100, 201, &core));
}

#[test]
fn the_core_test_uses_both_dimensions_independently() {
    // A macro that would fit if rotated still does not fit: no rotation is considered here.
    let core = r(0, 0, 100, 10);
    assert!(!macro_fits_in_core(10, 100, &core));
}
