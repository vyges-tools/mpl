// SPDX-License-Identifier: Apache-2.0
//! The physical-hierarchy dump — our oracle's format.
//!
//! 🔑 **The headline test is pinned to output captured from upstream itself**, run at our pin.
//! That is what makes this the strongest check in the crate: it compares the whole tree, not a
//! handful of numbers, and it would catch a misreading of the format that a test written from
//! the source could not.
use vyges_mpl::cluster::{Cluster, ClusterType, Metrics};
use vyges_mpl::dump::physical_hierarchy;

fn cl(id: i32, name: &str, ty: ClusterType, std: i32, sa: i64, mac: i32, ma: i64) -> Cluster {
    let mut c = Cluster::new(id, name);
    c.cluster_type = ty;
    c.metrics = Metrics { num_std_cell: std, num_macro: mac, std_cell_area: sa, macro_area: ma };
    c
}

#[test]
fn the_dump_matches_output_captured_from_upstream() {
    // Captured verbatim by running upstream at pin 945a9f4 on its own `halos1` case, under
    // `set_debug_level MPL multilevel_autoclustering 1`:
    //
    //   root  (0) Type: Mixed , StdCells: 150 (2713200000 μ²), Macros: 2 (80000000000 μ²),
    //   +---(root)_glue_logic  (1) Type: StdCell Leaf, StdCells: 150 (2713200000 μ²)
    //   +---MACRO_2  (2) Type: Macro Leaf, Macros: 1 (40000000000 μ²),
    //   +---MACRO_1  (3) Type: Macro Leaf, Macros: 1 (40000000000 μ²),
    let mut root = cl(0, "root", ClusterType::Mixed, 150, 2_713_200_000, 2, 80_000_000_000);
    root.children = vec![
        cl(1, "(root)_glue_logic", ClusterType::StdCell, 150, 2_713_200_000, 0, 0),
        cl(2, "MACRO_2", ClusterType::HardMacro, 0, 0, 1, 40_000_000_000),
        cl(3, "MACRO_1", ClusterType::HardMacro, 0, 0, 1, 40_000_000_000),
    ];

    let expected = "\
root  (0) Type: Mixed , StdCells: 150 (2713200000 μ²), Macros: 2 (80000000000 μ²),
+---(root)_glue_logic  (1) Type: StdCell Leaf, StdCells: 150 (2713200000 μ²)
+---MACRO_2  (2) Type: Macro Leaf, Macros: 1 (40000000000 μ²),
+---MACRO_1  (3) Type: Macro Leaf, Macros: 1 (40000000000 μ²),
";
    assert_eq!(physical_hierarchy(&root), expected);
}

#[test]
fn a_non_leaf_keeps_the_space_before_the_comma() {
    // ⚠️ `Type: Mixed ,` -- the leaf string is empty but its leading space is still printed.
    // Trimming it reads as tidying and breaks every comparison.
    let mut root = cl(0, "r", ClusterType::Mixed, 1, 2, 0, 0);
    root.children = vec![cl(1, "k", ClusterType::StdCell, 1, 2, 0, 0)];
    assert!(physical_hierarchy(&root).starts_with("r  (0) Type: Mixed , StdCells:"));
}

#[test]
fn the_macro_field_ends_with_a_trailing_comma() {
    // ⚠️ Upstream's format string has it. It is not a separator, and dropping it as an obvious
    // typo would break every comparison.
    let root = cl(0, "r", ClusterType::HardMacro, 0, 0, 1, 5);
    assert_eq!(physical_hierarchy(&root), "r  (0) Type: Macro Leaf, Macros: 1 (5 μ²),\n");
}

#[test]
fn a_field_prints_when_the_area_is_nonzero_even_if_the_count_is_zero() {
    // 🔑 Upstream's `or` is deliberate: its comment says it certifies "there is no discrepancy
    // going on". A cluster with area but no count is a bug it wants VISIBLE, so the dump shows
    // it rather than hiding it. An `and` here would conceal exactly the case worth seeing.
    let root = cl(0, "r", ClusterType::Mixed, 0, 99, 0, 0);
    assert!(physical_hierarchy(&root).contains("StdCells: 0 (99 μ²)"));
}

#[test]
fn a_cluster_with_neither_count_nor_area_prints_no_fields() {
    let root = cl(0, "r", ClusterType::Mixed, 0, 0, 0, 0);
    assert_eq!(physical_hierarchy(&root), "r  (0) Type: Mixed Leaf\n");
}

#[test]
fn depth_is_marked_with_one_prefix_per_level() {
    let mut root = cl(0, "r", ClusterType::Mixed, 0, 0, 0, 0);
    let mut kid = cl(1, "k", ClusterType::Mixed, 0, 0, 0, 0);
    kid.children = vec![cl(2, "g", ClusterType::StdCell, 1, 1, 0, 0)];
    root.children = vec![kid];
    let out = physical_hierarchy(&root);
    assert!(out.contains("\n+---k  (1)"), "one level");
    assert!(out.contains("\n+---+---g  (2)"), "two levels");
}

#[test]
fn a_pin_cluster_prints_pins_and_nothing_else() {
    let mut c = cl(0, "io", ClusterType::StdCell, 7, 7, 0, 0);
    c.is_io_bundle = true;
    c.num_io_pins = 4;
    assert_eq!(physical_hierarchy(&c), "io  (0) Type: IO Bundle Pins: 4\n");
}

#[test]
fn an_io_pad_cluster_prints_neither_pins_nor_counts() {
    let mut c = cl(0, "pads", ClusterType::StdCell, 7, 7, 0, 0);
    c.is_io_pad_cluster = true;
    assert_eq!(physical_hierarchy(&c), "pads  (0) Type: IO Pad\n");
}

#[test]
fn the_type_string_checks_io_and_fixed_before_the_ordinary_type() {
    // ⚠️ Order matters: a fixed macro is typed HardMacro AND flagged fixed, and must print
    // "Fixed Macro" rather than "Macro".
    let mut c = cl(0, "m", ClusterType::HardMacro, 0, 0, 1, 5);
    c.is_fixed_macro = true;
    assert!(physical_hierarchy(&c).contains("Type: Fixed Macro"));
    c.is_io_bundle = true;
    assert!(physical_hierarchy(&c).contains("Type: IO Bundle"), "IO bundle outranks fixed macro");
}

#[test]
fn children_print_in_order_after_their_parent() {
    let mut root = cl(0, "r", ClusterType::Mixed, 0, 0, 0, 0);
    root.children = vec![
        cl(1, "first", ClusterType::StdCell, 1, 1, 0, 0),
        cl(2, "second", ClusterType::StdCell, 1, 1, 0, 0),
    ];
    let out = physical_hierarchy(&root);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("r  "));
    assert!(lines[1].contains("first"));
    assert!(lines[2].contains("second"));
}
