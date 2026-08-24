// SPDX-License-Identifier: Apache-2.0
//! Reading and classifying the design. Rules from upstream `ClusteringEngine`.
use vyges_mpl::design::{
    compute_module_metrics, floorplan_shape, is_ignored_inst, unfixed_macros, Design, Instance,
    MasterKind, Module, ModuleMetrics, Rect,
};

fn r(x0: i64, y0: i64, x1: i64, y1: i64) -> Rect {
    Rect { x_min: x0, y_min: y0, x_max: x1, y_max: y1 }
}

fn inst(name: &str, is_block: bool) -> Instance {
    Instance {
        name: name.into(),
        is_block,
        is_fixed: false,
        bbox: r(0, 0, 10, 10),
        master: MasterKind::default(),
        is_ignorable_macro: false,
    }
}

fn design(instances: Vec<Instance>, modules: Vec<Module>) -> Design {
    Design { instances, modules, top: 0, core_area: r(0, 0, 1000, 1000), die_area: r(0, 0, 1000, 1000) }
}

fn top(insts: Vec<usize>, children: Vec<usize>) -> Module {
    Module { name: "top".into(), hierarchical_name: "top".into(), insts, children }
}

// ------------------------------------------------------------------ geometry

#[test]
fn touching_rectangles_do_not_overlap() {
    // ⚠️ A macro abutting the placement area is OUTSIDE it. Using <= here would pull every
    // edge-abutting fixed macro into consideration.
    assert!(!r(0, 0, 10, 10).overlaps(&r(10, 0, 20, 10)), "shared edge");
    assert!(r(0, 0, 10, 10).overlaps(&r(9, 0, 20, 10)), "one unit of overlap");
}

#[test]
fn a_degenerate_intersection_has_no_area() {
    assert_eq!(r(0, 0, 10, 10).intersection(&r(10, 0, 20, 10)).area(), 0);
    assert_eq!(r(0, 0, 10, 10).intersection(&r(5, 5, 20, 20)), r(5, 5, 10, 10));
}

// ------------------------------------------------------------------ is_ignored_inst

#[test]
fn pads_covers_and_endcaps_are_ignored() {
    for set in [
        |m: &mut MasterKind| m.is_pad = true,
        |m: &mut MasterKind| m.is_cover = true,
        |m: &mut MasterKind| m.is_end_cap = true,
    ] {
        let mut i = inst("x", false);
        assert!(!is_ignored_inst(&i));
        set(&mut i.master);
        assert!(is_ignored_inst(&i), "physical-only cells are not the placer's to move");
    }
}

#[test]
fn an_ignorable_macro_is_ignored_but_an_ordinary_one_is_not() {
    let mut m = inst("m", true);
    assert!(!is_ignored_inst(&m));
    m.is_ignorable_macro = true;
    assert!(is_ignored_inst(&m));
}

#[test]
fn the_ignorable_flag_only_applies_to_macros() {
    // The flag is set on fixed macros outside the area; a std cell carrying it is not ignored
    // by that route, only by its master type.
    let mut s = inst("s", false);
    s.is_ignorable_macro = true;
    assert!(!is_ignored_inst(&s));
}

// ------------------------------------------------------------------ module metrics

#[test]
fn macros_and_standard_cells_are_counted_separately_with_their_areas() {
    let mut mac = inst("m", true);
    mac.bbox = r(0, 0, 100, 100);
    let d = design(vec![inst("s1", false), inst("s2", false), mac], vec![top(vec![0, 1, 2], vec![])]);
    let mut errs = Vec::new();
    let m = compute_module_metrics(&d, 0, &r(0, 0, 1000, 1000), &mut errs);
    assert_eq!(m, ModuleMetrics { num_std_cell: 2, num_macro: 1, std_cell_area: 200, macro_area: 10_000 });
    assert!(errs.is_empty());
}

#[test]
fn an_ignorable_macro_STILL_counts_as_a_macro() {
    // 🔑 The branch order is the rule: `is_block` is tested BEFORE the ignore check, so an
    // ignorable macro contributes to num_macro and macro_area even though clustering skips it.
    // Testing the ignore check first would silently shrink every macro count.
    let mut mac = inst("m", true);
    mac.is_ignorable_macro = true;
    mac.is_fixed = true;
    mac.bbox = r(0, 0, 100, 100);
    assert!(is_ignored_inst(&mac), "it IS ignored for clustering");

    let d = design(vec![mac], vec![top(vec![0], vec![])]);
    let mut errs = Vec::new();
    let m = compute_module_metrics(&d, 0, &r(0, 0, 1000, 1000), &mut errs);
    assert_eq!(m.num_macro, 1, "and it STILL counts here");
    assert_eq!(m.macro_area, 10_000);
}

#[test]
fn an_ignored_standard_cell_counts_as_neither() {
    // It falls through all three branches: not a block, not fixed, and ignored.
    let mut pad = inst("p", false);
    pad.master.is_pad = true;
    let d = design(vec![pad], vec![top(vec![0], vec![])]);
    let mut errs = Vec::new();
    let m = compute_module_metrics(&d, 0, &r(0, 0, 1000, 1000), &mut errs);
    assert_eq!(m, ModuleMetrics::default(), "neither macro nor standard cell");
}

#[test]
fn a_fixed_cell_inside_the_placement_area_is_an_error() {
    let mut f = inst("f", false);
    f.is_fixed = true;
    f.bbox = r(10, 10, 20, 20);
    let d = design(vec![f], vec![top(vec![0], vec![])]);
    let mut errs = Vec::new();
    let m = compute_module_metrics(&d, 0, &r(0, 0, 1000, 1000), &mut errs);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].name, "f");
    assert_eq!(m.num_std_cell, 0, "and it is not counted");
}

#[test]
fn a_fixed_cover_cell_inside_the_area_is_allowed() {
    // ⚠️ Cover masters are exempt: they are markers, not obstructions the placer must respect.
    let mut f = inst("f", false);
    f.is_fixed = true;
    f.master.is_cover = true;
    f.bbox = r(10, 10, 20, 20);
    let d = design(vec![f], vec![top(vec![0], vec![])]);
    let mut errs = Vec::new();
    compute_module_metrics(&d, 0, &r(0, 0, 1000, 1000), &mut errs);
    assert!(errs.is_empty(), "a cover is exempt from the fixed-instance error");
}

#[test]
fn a_fixed_cell_outside_the_placement_area_is_allowed_and_counted() {
    let mut f = inst("f", false);
    f.is_fixed = true;
    f.bbox = r(2000, 2000, 2010, 2010);
    let d = design(vec![f], vec![top(vec![0], vec![])]);
    let mut errs = Vec::new();
    let m = compute_module_metrics(&d, 0, &r(0, 0, 1000, 1000), &mut errs);
    assert!(errs.is_empty());
    assert_eq!(m.num_std_cell, 1, "outside the area it is an ordinary standard cell");
}

#[test]
fn metrics_accumulate_through_the_module_hierarchy() {
    let d = Design {
        instances: vec![inst("a", false), inst("b", false), inst("m", true)],
        modules: vec![
            Module { name: "top".into(), hierarchical_name: "top".into(), insts: vec![0], children: vec![1] },
            Module { name: "sub".into(), hierarchical_name: "top/sub".into(), insts: vec![1, 2], children: vec![2] },
            Module { name: "leaf".into(), hierarchical_name: "top/sub/leaf".into(), insts: vec![], children: vec![] },
        ],
        top: 0,
        core_area: r(0, 0, 1000, 1000),
        die_area: r(0, 0, 1000, 1000),
    };
    let mut errs = Vec::new();
    let m = compute_module_metrics(&d, 0, &r(0, 0, 1000, 1000), &mut errs);
    assert_eq!(m.num_std_cell, 2, "one from top, one from sub");
    assert_eq!(m.num_macro, 1, "the macro in sub reaches the top's total");
}

// ------------------------------------------------------------------ selection

#[test]
fn only_unfixed_macros_are_the_placers_to_move() {
    let mut fixed = inst("fm", true);
    fixed.is_fixed = true;
    let d = design(vec![inst("m", true), fixed, inst("s", false)], vec![top(vec![0, 1, 2], vec![])]);
    assert_eq!(unfixed_macros(&d), vec![0]);
}

// ------------------------------------------------------------------ floorplan shape

#[test]
fn without_a_fence_the_placement_area_is_the_core() {
    assert_eq!(floorplan_shape(&r(0, 0, 100, 100), None), Some(r(0, 0, 100, 100)));
}

#[test]
fn a_fence_clips_the_core() {
    assert_eq!(
        floorplan_shape(&r(0, 0, 100, 100), Some(&r(50, 50, 200, 200))),
        Some(r(50, 50, 100, 100))
    );
}

#[test]
fn a_fence_outside_the_core_leaves_nothing_to_place_into() {
    // ⛔ Upstream errors here rather than falling back to the core -- silently ignoring the
    // fence would place macros where the user explicitly excluded them.
    assert_eq!(floorplan_shape(&r(0, 0, 100, 100), Some(&r(500, 500, 600, 600))), None);
}
