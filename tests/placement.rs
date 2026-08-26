// SPDX-License-Identifier: Apache-2.0
//! The placement stage's setup steps.

use vyges_mpl::placement::{adjusted_soft_blockage_weight, tiny_cluster_max_number_of_std_cells};

/// ⚠️ **Truncated, so a small block gets a threshold of ZERO** — and since the comparison against
/// it is a strict `<`, zero means no cluster is ever tiny.
#[test]
fn a_small_block_has_no_tiny_clusters_at_all() {
    assert_eq!(tiny_cluster_max_number_of_std_cells(0), 0);
    assert_eq!(tiny_cluster_max_number_of_std_cells(999), 0, "just under the first whole unit");
    assert_eq!(tiny_cluster_max_number_of_std_cells(1000), 1);
    assert_eq!(tiny_cluster_max_number_of_std_cells(1999), 1, "still one, truncated");
    assert_eq!(tiny_cluster_max_number_of_std_cells(2000), 2);
}

/// ⚠️ Every one of the regression designs is far below the first whole unit, so the threshold is
/// zero throughout — worth knowing before reading anything into it.
#[test]
fn the_regression_designs_are_all_below_the_threshold() {
    for instance_count in [4usize, 152, 500, 900] {
        assert_eq!(
            tiny_cluster_max_number_of_std_cells(instance_count),
            0,
            "{instance_count} instances"
        );
    }
}

/// 🔑 **Only a single-level tree is adjusted**, to half the outline weight. A deeper tree keeps
/// whatever it had.
#[test]
fn only_a_single_level_tree_has_its_soft_blockage_weight_raised() {
    assert_eq!(adjusted_soft_blockage_weight(1, 100.0, 50.0), 50.0, "half of 100 is 50");
    assert_eq!(adjusted_soft_blockage_weight(1, 40.0, 50.0), 20.0, "and it can go DOWN");
    assert_eq!(adjusted_soft_blockage_weight(2, 100.0, 50.0), 50.0, "two levels: untouched");
    assert_eq!(adjusted_soft_blockage_weight(0, 100.0, 7.0), 7.0, "zero levels: untouched");
}

// ---------------------------------------------------------------- blockages

use vyges_mpl::placement::{
    eliminate_overlaps, find_offset_intersections, soft_macros_for_blockages, NeedsPolygonUnion,
};

/// 🔑 Everything inside a placement problem is OUTLINE-RELATIVE.
#[test]
fn an_intersection_is_re_expressed_against_the_outline_corner() {
    let outline = (400, 400, 1400, 1400);
    let got = find_offset_intersections(&[(500, 500, 600, 700)], outline);
    assert_eq!(got, vec![(100, 100, 200, 300)]);
}

/// ⚠️ A blockage is CLIPPED to the outline, not dropped, when it only partly overlaps.
#[test]
fn a_blockage_hanging_outside_is_clipped() {
    let outline = (0, 0, 1000, 1000);
    assert_eq!(find_offset_intersections(&[(-500, 100, 500, 200)], outline), vec![(0, 100, 500, 200)]);
}

/// ⚠️ **A zero-area intersection is dropped.** A blockage touching the outline edge-on contributes
/// nothing, and a complete miss produces an inverted rectangle — both are rejected.
#[test]
fn edge_on_and_missing_blockages_are_dropped() {
    let outline = (0, 0, 1000, 1000);
    assert!(find_offset_intersections(&[(1000, 100, 1200, 200)], outline).is_empty(), "edge-on");
    assert!(find_offset_intersections(&[(5000, 5000, 6000, 6000)], outline).is_empty(), "a miss");
    assert!(find_offset_intersections(&[(100, 100, 100, 900)], outline).is_empty(), "zero width");
}

/// 🔑 Zero or one rectangle needs no polygon library, and one is what every design in the
/// reference suite that has a blockage at all actually has.
#[test]
fn a_single_blockage_passes_through_unchanged() {
    assert_eq!(eliminate_overlaps(&[]), Ok(Vec::new()));
    assert_eq!(eliminate_overlaps(&[(0, 0, 10, 10)]), Ok(vec![(0, 0, 10, 10)]));
}

/// ⛔ **Two or more is refused by name, even when they do not overlap** — the reference merges
/// rectangles that merely touch, so "no overlap" is not "no work", and a plausible decomposition
/// is not the same decomposition.
#[test]
fn several_blockages_are_refused_rather_than_approximated() {
    assert_eq!(eliminate_overlaps(&[(0, 0, 10, 10), (5, 5, 15, 15)]), Err(NeedsPolygonUnion));
    assert_eq!(
        eliminate_overlaps(&[(0, 0, 10, 10), (500, 500, 510, 510)]),
        Err(NeedsPolygonUnion),
        "disjoint is refused too"
    );
}

/// ⚠️ Blockage macros are FIXED and take the lowest ids, offsetting every cluster after them.
#[test]
fn blockage_macros_are_fixed_and_come_first() {
    let macros = soft_macros_for_blockages(&[(10, 20, 40, 60)]);
    assert_eq!(macros.len(), 1);
    assert_eq!((macros[0].x, macros[0].y, macros[0].width, macros[0].height), (10, 20, 30, 40));
    assert!(macros[0].fixed);
    assert_eq!(macros[0].area, 30 * 40);
}

// ---------------------------------------------------------------- fixed terminals

use vyges_mpl::placement::{fixed_terminal, fixed_terminal_walk, TerminalCluster};

fn ordinary(center: (i32, i32)) -> TerminalCluster {
    TerminalCluster {
        center,
        origin: (999, 999),
        width: 77,
        height: 88,
        is_cluster_of_unplaced_io_pins: false,
    }
}

/// 🔑 **An ordinary terminal is a POINT at the cluster's CENTRE**, with its size discarded — it
/// exists only to pull wirelength toward where that cluster sits.
#[test]
fn an_ordinary_terminal_is_a_point_at_the_centre() {
    let t = fixed_terminal(&ordinary((500, 600)), (100, 200));
    assert_eq!((t.x, t.y), (400, 400), "centre, offset by the outline");
    assert_eq!((t.width, t.height), (0, 0), "its extent is discarded");
    assert!(t.fixed);
    assert_eq!(t.area, 0);
}

/// ⚠️ **A cluster of unplaced IO pins keeps its ORIGIN and its extent** — a different anchor from
/// the ordinary case, because the wirelength model measures distance to the region it covers.
#[test]
fn an_unplaced_io_cluster_keeps_its_origin_and_extent() {
    let mut c = ordinary((500, 600));
    c.origin = (10, 20);
    c.is_cluster_of_unplaced_io_pins = true;
    let t = fixed_terminal(&c, (5, 5));
    assert_eq!((t.x, t.y), (5, 15), "origin, not centre");
    assert_eq!((t.width, t.height), (77, 88), "and its real extent");
}

/// ⛔ **The area is ZERO even for the sized one.** Area is what tells a terminal from a placeable
/// macro inside the annealer, so giving the region its real area would make it placeable.
#[test]
fn a_sized_terminal_still_has_no_area() {
    let mut c = ordinary((0, 0));
    c.is_cluster_of_unplaced_io_pins = true;
    assert_eq!(fixed_terminal(&c, (0, 0)).area, 0);
}

// A small tree:  0 (root) -> 1, 2 ;  1 -> 3, 4 ;  3 -> 5, 6
fn parent_of(id: usize) -> Option<usize> {
    match id {
        0 => None,
        1 | 2 => Some(0),
        3 | 4 => Some(1),
        5 | 6 => Some(3),
        _ => None,
    }
}
fn children_of(id: usize) -> Vec<usize> {
    match id {
        0 => vec![1, 2],
        1 => vec![3, 4],
        3 => vec![5, 6],
        _ => Vec::new(),
    }
}

/// ⚠️ A cluster with no parent contributes nothing.
#[test]
fn the_root_has_no_terminals() {
    assert!(fixed_terminal_walk(0, &parent_of, &children_of).is_empty());
}

/// ⛔ **The climb stops one level below the root**, because the test is on the GRANDparent. A
/// cluster whose parent is the root gets its siblings and nothing more.
#[test]
fn a_child_of_the_root_gets_only_its_siblings() {
    assert_eq!(fixed_terminal_walk(1, &parent_of, &children_of), vec![2]);
}

/// 🔑 It climbs, gathering aunts and uncles — siblings first, then the parent's siblings.
#[test]
fn the_walk_climbs_gathering_siblings_at_each_level() {
    // 3's siblings are [4]; then it steps to 1, whose sibling is [2].
    assert_eq!(fixed_terminal_walk(3, &parent_of, &children_of), vec![4, 2]);
    // 5's siblings are [6]; then 3 -> [4]; then 1 -> [2].
    assert_eq!(fixed_terminal_walk(5, &parent_of, &children_of), vec![6, 4, 2]);
}

/// ⚠️ The node itself is never among its own terminals.
#[test]
fn a_cluster_is_never_its_own_terminal() {
    for start in [1usize, 3, 5] {
        let walk = fixed_terminal_walk(start, &parent_of, &children_of);
        assert!(!walk.contains(&start), "{start} appeared in its own walk");
    }
}

// ---------------------------------------------------------------- nets, fences and guides

use vyges_mpl::placement::{merge_nets, merged_region, BundledNet};

fn net(source: usize, target: usize, weight: f32) -> BundledNet {
    BundledNet { source, target, weight }
}

/// ⚠️ Duplicates collapse onto the FIRST occurrence, summing their weights.
#[test]
fn duplicate_nets_collapse_onto_the_first() {
    let merged = merge_nets(&[net(1, 2, 3.0), net(4, 5, 1.0), net(1, 2, 7.0)]);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0], net(1, 2, 10.0), "summed, in the first position");
    assert_eq!(merged[1], net(4, 5, 1.0));
}

/// ⛔ **The match is DIRECTED.** `(a, b)` and `(b, a)` are NOT duplicates and both survive —
/// treating the pair as unordered would merge nets the reference keeps apart.
#[test]
fn a_reversed_pair_is_not_a_duplicate() {
    let merged = merge_nets(&[net(1, 2, 3.0), net(2, 1, 4.0)]);
    assert_eq!(merged.len(), 2, "both survive");
    assert_eq!(merged[0].weight, 3.0);
    assert_eq!(merged[1].weight, 4.0);
}

/// ⚠️ Three of a kind all fold into the first.
#[test]
fn several_duplicates_all_fold_into_one() {
    let merged = merge_nets(&[net(1, 2, 1.0), net(1, 2, 2.0), net(1, 2, 4.0)]);
    assert_eq!(merged, vec![net(1, 2, 7.0)]);
}

/// ℹ️ Nothing to merge.
#[test]
fn merging_an_empty_or_distinct_list_changes_nothing() {
    assert!(merge_nets(&[]).is_empty());
    let distinct = [net(1, 2, 1.0), net(3, 4, 2.0)];
    assert_eq!(merge_nets(&distinct), distinct.to_vec());
}

/// 🔑 **A cluster inherits the UNION of its macros' regions** — merging two far-apart boxes yields
/// a region much larger than either, which is the intended (and lossy) consequence of grouping.
#[test]
fn a_clusters_region_is_the_union_of_its_macros() {
    let outline = (0, 0, 1000, 1000);
    let got = merged_region(&[(100, 100, 200, 200), (800, 800, 900, 900)], outline);
    assert_eq!(got, Some((100, 100, 900, 900)), "the bounding box of both");
}

/// ⚠️ Clipped to the outline, and re-expressed against its corner.
#[test]
fn a_region_is_clipped_and_offset() {
    let outline = (400, 400, 1400, 1400);
    assert_eq!(merged_region(&[(300, 500, 600, 700)], outline), Some((0, 100, 200, 300)));
}

/// ⚠️ **A region entirely outside the outline is DROPPED**, so it constrains nothing rather than
/// constraining everything.
#[test]
fn a_region_outside_the_outline_is_dropped() {
    let outline = (0, 0, 1000, 1000);
    assert_eq!(merged_region(&[(5000, 5000, 6000, 6000)], outline), None);
    assert_eq!(merged_region(&[(1000, 100, 1200, 200)], outline), None, "edge-on is zero area");
    assert_eq!(merged_region(&[], outline), None, "and no regions at all");
}

// ---------------------------------------------------------------- utilization

use vyges_mpl::placement::{set_macro_cluster_shapes, valid_utilization, AreaContribution, AreaKind};

fn contribution(kind: AreaKind, soft: i64, std_cell: i64, tiling: i64) -> AreaContribution {
    AreaContribution {
        kind,
        soft_macro_area: soft,
        cluster_std_cell_area: std_cell,
        first_tiling_area: tiling,
    }
}

/// 🔑 Hard area is taken out first; what remains has to hold the inflated soft area.
#[test]
fn the_soft_area_must_fit_what_the_hard_area_leaves() {
    // Outline 1,000,000. Hard 400,000. Soft 300,000 at utilization 0.5 inflates to 600,000.
    let c = [
        contribution(AreaKind::HardMacroCluster, 0, 0, 400_000),
        contribution(AreaKind::StdCellCluster, 0, 300_000, 0),
    ];
    assert!(!valid_utilization(&c, 1_000_000, 0.5), "600,000 does not fit in 600,000");
    assert!(valid_utilization(&c, 1_000_001, 0.5), "one more unit and it does");
    assert!(valid_utilization(&c, 1_000_000, 0.6), "or less inflation");
}

/// ⚠️ **A fixed macro is measured by its SOFT MACRO's area** — the clipped one — so a macro half
/// outside the outline charges only for the half inside.
#[test]
fn a_fixed_macro_charges_only_the_part_inside() {
    let half_in = [contribution(AreaKind::FixedMacro, 200_000, 0, 999_999)];
    // The tiling area is deliberately huge; only the soft macro area should count.
    assert!(valid_utilization(&half_in, 1_000_000, 1.0), "the tiling area was not used");
}

/// ⚠️ IO clusters are skipped entirely, however large.
#[test]
fn io_clusters_contribute_nothing() {
    let c = [contribution(AreaKind::IoCluster, 900_000, 900_000, 900_000)];
    assert!(valid_utilization(&c, 1_000, 1.0), "a huge IO cluster still fits a tiny outline");
}

/// ⚠️ A mixed cluster splits: its macro half is hard, its cell half is soft.
#[test]
fn a_mixed_cluster_contributes_to_both_halves() {
    let c = [contribution(AreaKind::MixedCluster, 0, 100_000, 500_000)];
    // Hard 500,000, leaving 500,000; soft 100,000 at 0.5 inflates to 200,000 — fits.
    assert!(valid_utilization(&c, 1_000_000, 0.5));
    // Hard 500,000, leaving 100,000; 200,000 does not fit.
    assert!(!valid_utilization(&c, 600_000, 0.5));
}

/// ⚠️ Blockages count by their physical area.
#[test]
fn blockages_count_as_hard_area() {
    let c = [
        contribution(AreaKind::Blockage, 500_000, 0, 0),
        contribution(AreaKind::StdCellCluster, 0, 100_000, 0),
    ];
    assert!(!valid_utilization(&c, 550_000, 1.0), "the blockage ate the room");
    assert!(valid_utilization(&c, 700_000, 1.0));
}

/// ⚠️ **Without the force flag an empty tiling list is declined**, unlike the shaping stage which
/// forces the same call — so an unshaped macro cluster reaches placement with no curve at all.
#[test]
fn an_unshaped_macro_cluster_gets_no_curve_at_placement() {
    assert!(set_macro_cluster_shapes(true, false, &[]).is_none());
    assert!(set_macro_cluster_shapes(true, true, &[(10, 10)]).is_none(), "fixed is skipped");
    assert!(set_macro_cluster_shapes(false, false, &[(10, 10)]).is_none(), "not a macro cluster");
    assert!(set_macro_cluster_shapes(true, false, &[(10, 20)]).is_some());
}

// ---------------------------------------------------------------- fine shaping

use vyges_mpl::placement::{
    mixed_cluster_shape, single_array_single_std_cell_cluster, std_cell_cluster_shape,
};

/// 🔑 Exactly one macro array and exactly one cell cluster, and nothing else.
#[test]
fn the_single_array_case_needs_exactly_one_of_each() {
    let one_each = [(AreaKind::HardMacroCluster, true), (AreaKind::StdCellCluster, false)];
    assert!(single_array_single_std_cell_cluster(&one_each));

    let two_arrays = [
        (AreaKind::HardMacroCluster, true),
        (AreaKind::HardMacroCluster, true),
        (AreaKind::StdCellCluster, false),
    ];
    assert!(!single_array_single_std_cell_cluster(&two_arrays));
    assert!(!single_array_single_std_cell_cluster(&[(AreaKind::HardMacroCluster, true)]), "no cells");
    assert!(!single_array_single_std_cell_cluster(&[(AreaKind::StdCellCluster, false)]), "no array");
}

/// ⛔ **Any mixed cluster fails it outright**, and a macro cluster that is not an ARRAY does too.
#[test]
fn a_mixed_cluster_or_a_non_array_disqualifies_it() {
    let with_mixed = [
        (AreaKind::HardMacroCluster, true),
        (AreaKind::StdCellCluster, false),
        (AreaKind::MixedCluster, false),
    ];
    assert!(!single_array_single_std_cell_cluster(&with_mixed));

    let not_an_array = [(AreaKind::HardMacroCluster, false), (AreaKind::StdCellCluster, false)];
    assert!(!single_array_single_std_cell_cluster(&not_an_array));
}

/// ℹ️ Blockages and IO clusters are skipped, so they cannot disqualify it.
#[test]
fn blockages_and_io_clusters_do_not_disqualify_the_single_array_case() {
    let with_extras = [
        (AreaKind::Blockage, false),
        (AreaKind::IoCluster, false),
        (AreaKind::HardMacroCluster, true),
        (AreaKind::StdCellCluster, false),
    ];
    assert!(single_array_single_std_cell_cluster(&with_extras));
}

/// 🔑 **A tiny cluster is collapsed to ONE unit square** — erased, not shrunk. It still exists for
/// the netlist, but occupies nothing.
#[test]
fn a_tiny_cluster_is_collapsed_to_a_unit_square() {
    let (interval, area) = std_cell_cluster_shape(1_000_000, 5, 10, false, 0.5, 0.33);
    assert_eq!((interval.min, interval.max), (1, 1));
    assert_eq!(area, 1, "not zero — a zero area would make it a fixed terminal");
}

/// ⚠️ The lone cell cluster of a single-array design gets the same treatment, however large.
#[test]
fn the_lone_cell_cluster_of_a_single_array_design_is_also_collapsed() {
    let (interval, area) = std_cell_cluster_shape(1_000_000, 99_999, 0, true, 0.5, 0.33);
    assert_eq!((interval.min, interval.max, area), (1, 1, 1));
}

/// ⚠️ Otherwise the area inflates by the utilization and the width comes from the aspect limit.
#[test]
fn an_ordinary_cell_cluster_inflates_by_the_utilization() {
    let (interval, area) = std_cell_cluster_shape(100_000, 5_000, 10, false, 0.5, 0.33);
    assert_eq!(area, 200_000, "halved utilization doubles the area");
    // width = sqrt(200000 / 0.33) = sqrt(606060.6) = 778.
    assert_eq!(interval.max, 778);
    assert_eq!(interval.min, 200_000 / 778, "the narrow end is area over that width");
    assert!(interval.min <= interval.max);
}

/// ⛔ **The macro area comes from the LAST tiling — the largest — not the first.** Using the first
/// would under-inflate every mixed cluster.
#[test]
fn a_mixed_cluster_inflates_against_its_largest_tiling() {
    // Tilings ordered by area: 100x100 = 10,000 then 300x300 = 90,000.
    let tilings = [(100, 100), (300, 300)];
    let (_, inflated) = mixed_cluster_shape(&tilings, 10_000, 0.5).expect("has tilings");
    assert_eq!(inflated, 90_000 + 20_000, "the LAST tiling's area plus the inflated cells");
    assert_ne!(inflated, 10_000 + 20_000, "not the first tiling's");
}

/// 🔑 **Only the cell half is inflated** — macros do not compress, so the macro area is added back
/// at full size.
#[test]
fn only_the_cell_half_of_a_mixed_cluster_inflates() {
    let tilings = [(100, 100)];
    let (_, half) = mixed_cluster_shape(&tilings, 10_000, 0.5).expect("has tilings");
    let (_, quarter) = mixed_cluster_shape(&tilings, 10_000, 0.25).expect("has tilings");
    assert_eq!(half, 10_000 + 20_000);
    assert_eq!(quarter, 10_000 + 40_000, "only the cells scaled");
}

/// ⚠️ One interval per tiling: a tall thin tiling permits a wide range, a short wide one almost
/// none.
#[test]
fn each_tiling_gets_its_own_width_range() {
    let tilings = [(100, 400), (400, 100)];
    let (intervals, inflated) = mixed_cluster_shape(&tilings, 0, 1.0).expect("has tilings");
    assert_eq!(inflated, 400 * 100, "the last tiling's area, with no cells to inflate");
    assert_eq!(intervals[0].min, 100);
    assert_eq!(intervals[0].max, inflated as i32 / 400);
    assert_eq!(intervals[1].min, 400);
    assert_eq!(intervals[1].max, inflated as i32 / 100);
}

/// ℹ️ No tilings, no shape.
#[test]
fn a_mixed_cluster_without_tilings_has_no_shape() {
    assert!(mixed_cluster_shape(&[], 1000, 0.5).is_none());
}

// ---------------------------------------------------------------- dead space

use vyges_mpl::placement::{dead_space_grid, fill_dead_space, segment_index, DeadSpaceMacro};

fn ds(x: i32, y: i32, w: i32, h: i32, mixed: bool, std_cell: bool) -> DeadSpaceMacro {
    DeadSpaceMacro {
        x,
        y,
        width: w,
        height: h,
        area: w as i64 * h as i64,
        is_mixed_cluster: mixed,
        is_std_cell_cluster: std_cell,
    }
}

/// ⚠️ **`lower_bound`: a coordinate that IS an edge returns that edge's index**, which is what
/// makes a macro's `[start, end)` span cover exactly its own cells.
#[test]
fn a_coordinate_on_an_edge_indexes_that_edge() {
    let coords = [0, 100, 300, 500];
    assert_eq!(segment_index(0, &coords), 0);
    assert_eq!(segment_index(100, &coords), 1, "on the edge, not the cell before");
    assert_eq!(segment_index(150, &coords), 2, "inside a cell, the next edge");
    assert_eq!(segment_index(500, &coords), 3);
}

/// 🔑 Every macro edge is a grid line, plus the outline's corners.
#[test]
fn the_grid_is_cut_at_every_macro_edge() {
    let macros = [ds(100, 50, 200, 100, false, false)];
    let (xs, ys) = dead_space_grid(&macros, (1000, 800));
    assert_eq!(xs, vec![0, 100, 300, 1000]);
    assert_eq!(ys, vec![0, 50, 150, 800]);
}

/// ⚠️ **A zero-area macro cuts nothing.** Fixed terminals carry zero area, so the filler expands
/// straight through where they sit.
#[test]
fn a_zero_area_macro_does_not_cut_the_grid() {
    let mut terminal = ds(400, 400, 0, 0, false, false);
    terminal.area = 0;
    let (xs, ys) = dead_space_grid(&[terminal], (1000, 800));
    assert_eq!(xs, vec![0, 1000], "no edge at 400");
    assert_eq!(ys, vec![0, 800]);
}

/// 🔑 **A lone cell cluster expands to fill the whole outline.**
#[test]
fn a_lone_cluster_takes_the_whole_outline() {
    let mut macros = [ds(100, 100, 200, 200, false, true)];
    fill_dead_space(&mut macros, (1000, 1000));
    assert_eq!((macros[0].x, macros[0].y), (0, 0));
    assert_eq!((macros[0].width, macros[0].height), (1000, 1000));
}

/// ⛔ **Expansion stops at the first occupied column** — it does not step over an obstacle to take
/// free space beyond it.
#[test]
fn expansion_stops_at_the_first_obstacle() {
    // A fixed block at x 400..600 spanning the full height, and a cell cluster to its left.
    let mut macros = [
        ds(0, 0, 200, 1000, false, true),
        ds(400, 0, 200, 1000, false, false),
    ];
    fill_dead_space(&mut macros, (1000, 1000));
    assert_eq!(macros[0].x, 0);
    assert_eq!(macros[0].width, 400, "grew right up to the block and stopped");
    assert_eq!((macros[1].x, macros[1].width), (400, 200), "the blocker did not move");
}

/// 🔑 **Mixed clusters take their space BEFORE cell clusters**, so what the first pass claims is
/// gone from the second. Swapping the passes would redistribute the empty space differently.
#[test]
fn a_mixed_cluster_claims_space_before_a_cell_cluster() {
    // ⚠️ The two must contend for the SAME gap. Side by side with free space above each, they
    // simply take their own column and the ordering proves nothing — which is what a first
    // version of this fixture did.
    //
    // Full-height clusters at either edge, with one free column between them:
    //   cell 0..400 | gap 400..600 | mixed 600..1000
    let mut macros = [
        ds(0, 0, 400, 1000, false, true),
        ds(600, 0, 400, 1000, true, false),
    ];
    fill_dead_space(&mut macros, (1000, 1000));
    assert_eq!((macros[1].x, macros[1].width), (400, 600), "the mixed cluster took the gap");
    assert_eq!((macros[0].x, macros[0].width), (0, 400), "the cell cluster found it gone");
}

/// ⚠️ A macro that is neither mixed nor a cell cluster is never grown.
#[test]
fn a_macro_cluster_is_not_grown() {
    let mut macros = [ds(100, 100, 200, 200, false, false)];
    let before = macros[0];
    fill_dead_space(&mut macros, (1000, 1000));
    assert_eq!(macros[0], before, "untouched");
}
