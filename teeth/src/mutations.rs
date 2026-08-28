// SPDX-License-Identifier: Apache-2.0
//! The mutation table: one broken rule per entry, and the test that must notice.
//!
//! 🔑 **This is a typed array, and that is the point.** Its predecessor was a list of bash
//! double-quoted strings where a raw `"` ended a row early and swallowed every row after it —
//! silently, and for several milestones. Here a malformed entry does not compile.
//!
//! ⚠️ Patterns are matched **literally**, once, at the first occurrence. Raw strings (`r#"…"#`)
//! mean quotes, backslashes and newlines all appear exactly as they do in the source, with no
//! escaping rules to get wrong.
//!
//! ⛔ **Three shaping mutations were REMOVED as EQUIVALENT, not fixed.** Each changed the source
//! and could not change the result, so no test can catch one and writing one would be writing a
//! test that cannot fail:
//!
//! * dropping the `num_macro() > 0` guard before recursing — the callee's own base case is the
//!   same test, so the guard is redundant. Upstream carries it too; ours matches for fidelity.
//! * adding `is_fixed_macro ||` to the single-contributor re-scan — a fixed macro cluster never
//!   has tilings, so finding it or not both leave the parent with none.
//! * dropping `is_fixed_macro ||` from the contributor filter — a fixed macro cluster reports
//!   `num_macro() == 1` (the `fixed_covers` dump says `Macros: 1`), so the count test already
//!   covers it. ⚠️ This one *looked* catchable and had a test, which passed only because it built
//!   a fixed cluster reporting zero macros — a state that never occurs.
//!
//! ⛔ **A FOURTH was removed as EQUIVALENT on 2026-08-26**, from the placement batch:
//!
//! * turning `fence_penalty`'s `x_dist <= max_x_dist` into `<`. At equality the `else` branch
//!   evaluates `x_dist - max_x_dist`, which is zero — the same answer the `then` branch gives. The
//!   two spellings cannot differ for any input. `a_macro_exactly_at_its_slack_limit_costs_nothing`
//!   still pins the BEHAVIOUR and is worth keeping; it simply cannot be reached by this operator.
//!
//! ⛔ **A FIFTH was removed as EQUIVALENT on 2026-08-27**, from the annealing batch:
//!
//! * `initialize`'s replay assigning `Penalties { area: self.penalties.area, ..s.penalties }`
//!   versus the whole struct. `Penalties::area` is never written by `cal_penalty` and never read
//!   by `norm_cost` — which derives area from the width and height instead — so the sample and the
//!   live state hold the same untouched zero and the two spellings cannot differ. 🔑 The mutation
//!   was worth writing anyway: it proved the partial update was drawing a distinction that does
//!   not exist, and the source now assigns the whole struct.
//!
//! ⚠️ **`engine::run_clustering` has NO unit coverage at all** — nothing in `tests/` drives it, so
//! a mutation to the handoff it builds (e.g. hardcoding `max_level: 1`) leaves `cargo test` green
//! and cannot be scored here. It IS caught, by `compare-placement.py` on the box, which this
//! harness does not run. A mutation whose only witness is an integration gate is not a hole in the
//! unit suite so much as a statement about where the coverage lives — recorded rather than added,
//! because adding it would report a permanent false hole.
//!
//! Do not re-add them: an equivalent mutant reported as a hole trains people to ignore holes.

use crate::Mutation;

pub const MUTATIONS: &[Mutation] = &[
    // ---------------------------------------------------------------- the boundary term
    Mutation {
        name: r#"boundary-left-side-absolute"#,
        file: r#"src/placement.rs"#,
        find: r#"            let x_dist_from_root = global_lx.min((root.width - global_ux).abs());"#,
        replace: r#"            let x_dist_from_root = global_lx.abs().min((root.width - global_ux).abs());"#,
        want: r#"overhanging_left_and_right_are_scored_differently"#,
    },
    Mutation {
        name: r#"boundary-nearer-edge-is-max"#,
        file: r#"src/placement.rs"#,
        find: r#"            let y_dist_from_root = global_ly.min((root.height - global_uy).abs());"#,
        replace: r#"            let y_dist_from_root = global_ly.max((root.height - global_uy).abs());"#,
        want: r#"hugging_one_edge_only_forgives_one_axis"#,
    },
    Mutation {
        name: r#"boundary-not-weighted-by-macro-count"#,
        file: r#"src/placement.rs"#,
        find: r#"            penalty = (penalty as f64 + microns * m.num_macro as f64) as f32;"#,
        replace: r#"            penalty = (penalty as f64 + microns) as f32;"#,
        want: r#"the_penalty_is_averaged_over_hard_macros_not_clusters"#,
    },
    Mutation {
        name: r#"boundary-measured-from-the-outline"#,
        file: r#"src/placement.rs"#,
        find: r#"            let global_lx = m.x + outline_origin.0 - root.x;"#,
        replace: r#"            let global_lx = m.x;"#,
        want: r#"the_distance_is_measured_from_the_root_not_the_outline"#,
    },
    Mutation {
        name: r#"boundary-counts-fixed-macros"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut number_of_movable_macros = 0i32;
    for &i in order {
        let m = &macros[i];
        if m.fixed {
            continue;
        }"#,
        replace: r#"    let mut number_of_movable_macros = 0i32;
    for &i in order {
        let m = &macros[i];
        if false {
            continue;
        }"#,
        want: r#"a_fixed_macro_is_skipped_entirely"#,
    },
    // ---------------------------------------------------------------- the notch term
    Mutation {
        name: r#"notch-coalesces-at-the-tolerance"#,
        file: r#"src/placement.rs"#,
        find: r#"        if coords[coords.len() - 1] - sorted[i] > epsilon {"#,
        replace: r#"        if coords[coords.len() - 1] - sorted[i] >= epsilon {"#,
        want: r#"near_coincident_grid_lines_are_coalesced"#,
    },
    Mutation {
        name: r#"notch-grid-tolerances-crossed"#,
        file: r#"src/placement.rs"#,
        find: r#"    (coalesce_downwards(&x_point, outline.0 / 100), coalesce_downwards(&y_point, outline.1 / 100))"#,
        replace: r#"    (coalesce_downwards(&x_point, outline.1 / 100), coalesce_downwards(&y_point, outline.0 / 100))"#,
        want: r#"each_axis_coalesces_at_its_own_tolerance"#,
    },
    Mutation {
        name: r#"notch-thresholds-crossed"#,
        file: r#"src/placement.rs"#,
        find: r#"    let notch_h_th = outline.1 / 10;
    let notch_v_th = outline.0 / 10;"#,
        replace: r#"    let notch_h_th = outline.0 / 10;
    let notch_v_th = outline.1 / 10;"#,
        want: r#"each_axis_is_measured_against_its_own_extent"#,
    },
    Mutation {
        name: r#"notch-threshold-inclusive"#,
        file: r#"src/placement.rs"#,
        find: r#"            if current.top && current.bottom && height < notch_h_th {"#,
        replace: r#"            if current.top && current.bottom && height <= notch_h_th {"#,
        want: r#"a_gap_at_the_threshold_is_not_a_notch"#,
    },
    Mutation {
        name: r#"notch-fixed-macro-obstructs"#,
        file: r#"src/placement.rs"#,
        find: r#"            Some(AreaKind::HardMacroCluster)
                | Some(AreaKind::MixedCluster)
                | Some(AreaKind::FixedMacro)"#,
        replace: r#"            Some(AreaKind::HardMacroCluster)
                | Some(AreaKind::MixedCluster)"#,
        // ⛔ The old `want` named `a_fixed_macro_does_not_create_a_notch`, a test DELETED when the
        // "a fixed macro obstructs nothing" misreading was corrected. ⚠️ A STALE PATTERN HID THAT:
        // the harness never reached the test name, so it reported 0 no-such-test while pointing at
        // a test that had not existed for two days.
        want: r#"a_fixed_macro_obstructs_the_notch_grid"#,
    },
    Mutation {
        name: r#"notch-invalid-floorplan-scanned"#,
        file: r#"src/placement.rs"#,
        find: r#"    if !valid {
        let width = packing.0.max(outline.0);"#,
        replace: r#"    if false {
        let width = packing.0.max(outline.0);"#,
        want: r#"an_invalid_floorplan_is_charged_as_one_huge_notch"#,
    },
    Mutation {
        name: r#"notch-vicinity-starts-false"#,
        file: r#"src/placement.rs"#,
        find: r#"        Self { top: true, bottom: true, left: true, right: true }"#,
        replace: r#"        Self { top: false, bottom: false, left: false, right: false }"#,
        want: r#"the_outline_boundary_counts_as_enclosing"#,
    },
    Mutation {
        name: r#"notch-region-not-marked-visited"#,
        file: r#"src/placement.rs"#,
        find: r#"            for row in visited.iter_mut().take(end_row + 1).skip(start_row) {"#,
        replace: r#"            for row in visited.iter_mut().take(0).skip(start_row) {"#,
        want: r#"an_expanded_region_is_not_counted_twice"#,
    },
    Mutation {
        name: r#"notch-vicinity-cleared-by-any-wall"#,
        file: r#"src/placement.rs"#,
        find: r#"    if end_row < num_y - 1 {
        for i in start_col..=end_col {
            if !grid[end_row + 1][i] {
                vicinity.top = false;
                break;
            }
        }
    }"#,
        replace: r#"    if end_row < num_y - 1 {
        for i in start_col..=end_col {
            if grid[end_row + 1][i] {
                vicinity.top = false;
                break;
            }
        }
    }"#,
        want: r#"one_gap_in_a_wall_clears_that_side"#,
    },
    // ---------------------------------------------------------------- the fence term
    Mutation {
        name: r#"fence-unsatisfiable-not-skipped"#,
        file: r#"src/placement.rs"#,
        find: r#"        if m.width > fence_dx || m.height > fence_dy {
            continue;
        }"#,
        replace: r#"        if false {
            continue;
        }"#,
        want: r#"a_fence_the_macro_cannot_fit_is_skipped"#,
    },
    Mutation {
        name: r#"fence-not-averaged"#,
        file: r#"src/placement.rs"#,
        find: r#"    penalty / fences.len() as f32
}"#,
        replace: r#"    penalty
}"#,
        want: r#"a_skipped_fence_still_dilutes_the_mean"#,
    },
    Mutation {
        name: r#"fence-squares-the-distance-not-the-ratio"#,
        file: r#"src/placement.rs"#,
        find: r#"        let width_ratio = width as f32 / outline.0 as f32;"#,
        replace: r#"        let width_ratio = width as f32;"#,
        want: r#"a_macro_outside_its_fence_pays_the_squared_overshoot"#,
    },
    // ---------------------------------------------------------------- the enhancements
    Mutation {
        name: r#"centralization-runs-when-overflowing"#,
        file: r#"src/placement.rs"#,
        find: r#"    if state.outline_penalty() > 0.0 {
        return false;
    }"#,
        replace: r#"    if false {
        return false;
    }"#,
        want: r#"an_overflowing_floorplan_gets_neither_enhancement"#,
    },
    Mutation {
        name: r#"centralization-reverts-on-equal-cost"#,
        file: r#"src/placement.rs"#,
        find: r#"    if state.norm_cost() > pre_cost && !force {"#,
        replace: r#"    if state.norm_cost() >= pre_cost && !force {"#,
        want: r#"an_equally_costly_centralization_is_kept"#,
    },
    Mutation {
        name: r#"centralization-force-ignored"#,
        file: r#"src/placement.rs"#,
        find: r#"    if state.norm_cost() > pre_cost && !force {
        let _ = set_cluster_locations(state.macros_mut(), &order, &saved);"#,
        replace: r#"    if state.norm_cost() > pre_cost {
        let _ = set_cluster_locations(state.macros_mut(), &order, &saved);"#,
        want: r#"forcing_keeps_a_costlier_centralization"#,
    },
    Mutation {
        name: r#"centralization-offset-not-halved"#,
        file: r#"src/placement.rs"#,
        find: r#"    ((outline.0 - packing.0) / 2, (outline.1 - packing.1) / 2)"#,
        replace: r#"    (outline.0 - packing.0, outline.1 - packing.1)"#,
        want: r#"the_centralization_offset_truncates"#,
    },
    Mutation {
        name: r#"move-floorplan-skips-fixed"#,
        file: r#"src/placement.rs"#,
        find: r#"        let (x, y) = (macros[id].x + offset.0, macros[id].y + offset.1);
        macros[id].set_x(x);
        macros[id].set_y(y);"#,
        replace: r#"    for &id in order {
        if macros[id].fixed {
            continue;
        }
        macros[id].x += offset.0;
        macros[id].y += offset.1;
    }"#,
        // ⛔ Same story: the old `want` named a test deleted with the same misreading. The rule
        // INVERTED — a fixed macro is NOT shifted, because `set_x`/`set_y` refuse it.
        want: r#"a_fixed_macro_is_not_shifted_with_the_rest"#,
    },
    Mutation {
        name: r#"locations-indexed-positionally"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut locations = vec![(0, 0); order.len()];
    for &id in order {
        locations[id] = (macros[id].x, macros[id].y);
    }"#,
        replace: r#"    let mut locations = vec![(0, 0); order.len()];
    for (position, &id) in order.iter().enumerate() {
        locations[position] = (macros[id].x, macros[id].y);
    }"#,
        want: r#"locations_are_indexed_by_macro_id"#,
    },
    Mutation {
        name: r#"alignment-thresholds-crossed"#,
        file: r#"src/placement.rs"#,
        find: r#"    (h.min(outline.1 / RATIO), v.min(outline.0 / RATIO))"#,
        replace: r#"    (h.min(outline.0 / RATIO), v.min(outline.1 / RATIO))"#,
        want: r#"the_alignment_thresholds_are_crossed"#,
    },
    Mutation {
        name: r#"alignment-ignores-the-smallest-cluster"#,
        file: r#"src/placement.rs"#,
        find: r#"    for m in macro_clusters {
        h = h.min(m.width);
        v = v.min(m.height);
    }"#,
        replace: r#"    for _m in macro_clusters {}"#,
        want: r#"the_smallest_macro_cluster_floors_both_thresholds"#,
    },
    Mutation {
        name: r#"alignment-right-edge-wins"#,
        file: r#"src/placement.rs"#,
        find: r#"        if lx < h_th {
            macros[id].set_x(0);
        } else if outline.0 - ux < h_th {"#,
        replace: r#"        if lx < h_th {
            macros[id].x = 0;
        }
        if outline.0 - ux < h_th {"#,
        want: r#"a_cluster_wider_than_the_outline_snaps_left"#,
    },
    Mutation {
        name: r#"alignment-runs-on-invalid-floorplans"#,
        file: r#"src/placement.rs"#,
        find: r#"    if !state.is_valid() {
        return false;
    }"#,
        replace: r#"    if false {
        return false;
    }"#,
        want: r#"an_invalid_floorplan_is_not_aligned"#,
    },
    Mutation {
        name: r#"alignment-runs-after-a-kept-centralization"#,
        file: r#"src/placement.rs"#,
        find: r#"    if attempt_centralization(state, pre_cost, force_centralization) {
        attempt_macro_cluster_alignment(state);
    }"#,
        replace: r#"    attempt_centralization(state, pre_cost, force_centralization);
    attempt_macro_cluster_alignment(state);"#,
        want: r#"alignment_runs_only_after_a_reverted_centralization"#,
    },
    Mutation {
        name: r#"alignment-snaps-every-cluster"#,
        file: r#"src/placement.rs"#,
        find: r#"        if !macros[id].is_macro_cluster {
            continue;
        }"#,
        replace: r#"        if false {
            continue;
        }"#,
        want: r#"only_macro_clusters_are_aligned"#,
    },
    // ---------------------------------------------------------------- assembling the problem
    Mutation {
        name: r#"io-clusters-not-deferred"#,
        file: r#"src/placement.rs"#,
        find: r#"        if child.kind == AreaKind::IoCluster {
            deferred_io.push(child);
            continue;
        }"#,
        replace: r#"        if false {
            deferred_io.push(child);
            continue;
        }"#,
        want: r#"io_clusters_are_appended_after_the_placeable_clusters"#,
    },
    Mutation {
        name: r#"sequence-pair-counted-after-the-terminals"#,
        file: r#"src/placement.rs"#,
        find: r#"    out.number_of_sequence_pair_macros = out.macros.len();

    for child in deferred_io {"#,
        replace: r#"    for child in deferred_io {"#,
        want: r#"the_sequence_pair_stops_before_the_io_clusters"#,
    },
    Mutation {
        name: r#"blockages-outside-the-sequence-pair"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut out = Assembly { macros: blockages.to_vec(), ..Default::default() };"#,
        replace: r#"    let mut out = Assembly::default();
    let _ = blockages;"#,
        want: r#"blockages_come_first_and_offset_every_cluster"#,
    },
    Mutation {
        name: r#"fixed-macro-cluster-takes-no-id"#,
        file: r#"src/placement.rs"#,
        find: r#"        let id = out.macros.len();
        out.id_of.push((child.name.clone(), id));"#,
        replace: r#"        let id = out.macros.len();
        if child.kind != AreaKind::FixedMacro {
            out.id_of.push((child.name.clone(), id));
        }"#,
        want: r#"a_fixed_macro_cluster_still_takes_an_id"#,
    },
    Mutation {
        name: r#"std-cell-cluster-gets-a-fence"#,
        file: r#"src/placement.rs"#,
        find: r#"        if child.kind == AreaKind::FixedMacro || child.kind == AreaKind::StdCellCluster {
            continue;
        }"#,
        replace: r#"        if child.kind == AreaKind::FixedMacro {
            continue;
        }"#,
        want: r#"a_std_cell_cluster_gets_no_fence_or_guide"#,
    },
    Mutation {
        name: r#"fixed-macro-cluster-gets-a-fence"#,
        file: r#"src/placement.rs"#,
        find: r#"        if child.kind == AreaKind::FixedMacro || child.kind == AreaKind::StdCellCluster {
            continue;
        }"#,
        replace: r#"        if child.kind == AreaKind::StdCellCluster {
            continue;
        }"#,
        want: r#"a_fixed_macro_cluster_gets_no_fence_or_guide"#,
    },
    Mutation {
        name: r#"fence-not-rebased-on-the-outline"#,
        file: r#"src/placement.rs"#,
        find: r#"            if let Some(clipped) = merged_region(&[fence], outline) {
                out.fences.push((id, clipped));
            }"#,
        replace: r#"            {
                out.fences.push((id, fence));
            }"#,
        want: r#"a_fence_is_clipped_rebased_and_keyed_by_id"#,
    },
    Mutation {
        name: r#"first-name-binding-wins"#,
        file: r#"src/placement.rs"#,
        find: r#"        self.id_of.iter().rev().find(|(n, _)| n == name).map(|(_, id)| *id)"#,
        replace: r#"        self.id_of.iter().find(|(n, _)| n == name).map(|(_, id)| *id)"#,
        want: r#"a_repeated_name_keeps_the_last_id"#,
    },
    Mutation {
        name: r#"blockages-given-names"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut deferred_io: Vec<&AssemblyChild> = Vec::new();"#,
        replace: r#"    for i in 0..out.macros.len() {
        out.id_of.push((format!("blockage_{i}"), i));
    }
    let mut deferred_io: Vec<&AssemblyChild> = Vec::new();"#,
        want: r#"a_blockage_has_no_name"#,
    },
    // ---------------------------------------------------------------- building from the tree
    Mutation {
        name: r#"classification-type-before-fixedness"#,
        file: r#"src/placement.rs"#,
        find: r#"    if cluster.is_fixed_macro {
        return AreaKind::FixedMacro;
    }"#,
        replace: r#"    if false {
        return AreaKind::FixedMacro;
    }"#,
        want: r#"the_classification_order_is_not_interchangeable"#,
    },
    Mutation {
        name: r#"classification-io-checked-last"#,
        file: r#"src/placement.rs"#,
        find: r#"    if cluster.is_io_cluster() {
        return AreaKind::IoCluster;
    }
    if cluster.is_fixed_macro {"#,
        replace: r#"    if cluster.is_fixed_macro {
        return AreaKind::FixedMacro;
    }
    if cluster.is_io_cluster() {
        return AreaKind::IoCluster;
    }
    if false {"#,
        want: r#"the_classification_order_is_not_interchangeable"#,
    },
    Mutation {
        name: r#"fixed-macro-not-clipped-into-the-problem"#,
        file: r#"src/placement.rs"#,
        find: r#"            let clipped = (
                bbox.0.max(outline.0),
                bbox.1.max(outline.1),
                bbox.2.min(outline.2),
                bbox.3.min(outline.3),
            );"#,
        replace: r#"            let clipped = bbox;"#,
        want: r#"a_fixed_macro_is_clipped_and_rebased_into_the_problem"#,
    },
    Mutation {
        name: r#"io-cluster-clipped-into-the-problem"#,
        file: r#"src/placement.rs"#,
        find: r#"                width: own.width,
                height: own.height,
                fixed: true,
                area: 0,"#,
        replace: r#"                width: own.width.min(outline.2 - outline.0),
                height: own.height.min(outline.3 - outline.1),
                fixed: true,
                area: 0,"#,
        want: r#"an_io_cluster_is_rebased_but_not_clipped"#,
    },
    Mutation {
        name: r#"cluster-starts-from-its-last-tiling"#,
        file: r#"src/placement.rs"#,
        find: r#"            let first = child.tilings.first();"#,
        replace: r#"            let first = child.tilings.last();"#,
        want: r#"an_ordinary_cluster_starts_from_its_first_tiling"#,
    },
    Mutation {
        name: r#"parent-nets-not-halved"#,
        file: r#"src/placement.rs"#,
        find: r#"            if child.id > target_cluster {
                nets.push(BundledNet { source, target, weight });
            }"#,
        replace: r#"            {
                nets.push(BundledNet { source, target, weight });
            }"#,
        want: r#"virtual_connections_lead_and_the_pairs_are_halved"#,
    },
    Mutation {
        name: r#"virtual-connections-appended-last"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut nets = Vec::new();
    for &(a, b) in ctx.virtual_connections {"#,
        replace: r#"    let mut nets = Vec::new();
    for &(a, b) in ctx.virtual_connections.iter().skip(usize::MAX) {"#,
        want: r#"virtual_connections_lead_and_the_pairs_are_halved"#,
    },
    Mutation {
        name: r#"every-child-reaches-the-fixed-penalty"#,
        file: r#"src/placement.rs"#,
        // ⚠️ The list is the SEQUENCE-PAIR PREFIX filtered by `fixed`, not the children filtered
        // by `AreaKind::FixedMacro` — a blockage proxy is fixed and belongs here too, and the old
        // pattern was written against the version that missed it.
        find: r#"            .take(assembly.number_of_sequence_pair_macros)
            .filter(|m| m.fixed)"#,
        replace: r#"            .take(assembly.number_of_sequence_pair_macros)"#,
        want: r#"only_fixed_macros_reach_the_fixed_penalty"#,
    },
    // ---------------------------------------------------------------- annealing one parent
    Mutation {
        name: r#"anneal-skips-initialize"#,
        file: r#"src/placement.rs"#,
        find: r#"    let init_temperature = search.initialize(&mut rng, params);"#,
        replace: r#"    let init_temperature = 1.0;"#,
        want: r#"a_different_seed_explores_differently"#,
    },
    Mutation {
        name: r#"anneal-skips-the-enhancements"#,
        file: r#"src/placement.rs"#,
        find: r#"    run_enhancements(&mut search, single_array);"#,
        replace: r#"    let _ = single_array;"#,
        want: r#"the_enhancements_move_the_floorplan_off_the_corner"#,
    },
    Mutation {
        name: r#"anneal-reports-every-run-valid"#,
        file: r#"src/placement.rs"#,
        find: r#"    if !search.is_valid(fixed_present) {
        return None;
    }"#,
        replace: r#"    if false {
        return None;
    }"#,
        want: r#"a_parent_that_cannot_fit_reports_invalid"#,
    },
    Mutation {
        name: r#"anneal-ignores-the-utilization"#,
        file: r#"src/placement.rs"#,
        find: r#"        utilization,
        problem.min_ar,
    );"#,
        replace: r#"        1.0,
        problem.min_ar,
    );"#,
        want: r#"the_utilization_changes_what_fits"#,
    },
    Mutation {
        name: r#"anneal-ignores-the-seed"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut rng = crate::rng::Mt19937::new(seed);"#,
        replace: r#"    let mut rng = crate::rng::Mt19937::new(0);"#,
        want: r#"a_different_seed_explores_differently"#,
    },
    Mutation {
        name: r#"anneal-drops-the-reshaped-curves"#,
        file: r#"src/placement.rs"#,
        find: r#"            curves[r.id] = curve;"#,
        replace: r#"            let _ = curve;"#,
        want: r#"a_resize_moves_along_the_reshaped_curve"#,
    },
    Mutation {
        name: r#"notch-thresholds-not-from-the-notch-pass"#,
        file: r#"src/placement.rs"#,
        find: r#"        if self.weights.notch > 0.0 {
            // ⛔ Crossed: `h` from the HEIGHT, `v` from the WIDTH. See `notch_penalty`.
            (self.outline_height / 10, self.outline_width / 10)
        } else {"#,
        replace: r#"        if false {
            (self.outline_height / 10, self.outline_width / 10)
        } else {"#,
        want: r#"the_alignment_thresholds_come_from_the_notch_pass"#,
    },
    Mutation {
        name: r#"seam-thresholds-uncrossed"#,
        file: r#"src/placement.rs"#,
        find: r#"            (self.outline_height / 10, self.outline_width / 10)"#,
        replace: r#"            (self.outline_width / 10, self.outline_height / 10)"#,
        want: r#"the_alignment_thresholds_come_from_the_notch_pass"#,
    },
    Mutation {
        name: r#"seam-order-is-every-macro"#,
        file: r#"src/placement.rs"#,
        find: r#"    fn order(&self) -> &[usize] {
        &self.sp.pos
    }"#,
        replace: r#"    fn order(&self) -> &[usize] {
        &self.sp.neg
    }"#,
        want: r#"the_seam_reports_only_the_sequence_pair"#,
    },
    // ---------------------------------------------------------------- applying one utilization
    Mutation {
        name: r#"utilization-reshapes-macro-clusters-too"#,
        file: r#"src/placement.rs"#,
        find: r#"            Some(AreaKind::HardMacroCluster) => continue,"#,
        replace: r#"            Some(AreaKind::HardMacroCluster) => {
                out.push(ReshapedMacro { id, intervals: Vec::new(), area: m.cluster_area });
            }"#,
        want: r#"only_cell_and_mixed_clusters_are_reshaped"#,
    },
    Mutation {
        name: r#"utilization-reshapes-fixed-macros"#,
        file: r#"src/placement.rs"#,
        find: r#"            None | Some(AreaKind::IoCluster) | Some(AreaKind::FixedMacro) | Some(AreaKind::Blockage) => {
                continue
            }"#,
        replace: r#"            None | Some(AreaKind::IoCluster) | Some(AreaKind::Blockage) => continue,
            Some(AreaKind::FixedMacro) => {
                out.push(ReshapedMacro { id, intervals: Vec::new(), area: m.cluster_area });
            }"#,
        want: r#"blockages_io_clusters_and_fixed_macros_are_untouched"#,
    },
    Mutation {
        name: r#"utilization-multiplies-instead-of-dividing"#,
        file: r#"src/placement.rs"#,
        find: r#"    let area = (cluster_area as f32 / utilization) as i64;"#,
        replace: r#"    let area = (cluster_area as f32 * utilization) as i64;"#,
        want: r#"a_lower_utilization_inflates_further"#,
    },
    Mutation {
        name: r#"tiny-cluster-collapsed-to-zero"#,
        file: r#"src/placement.rs"#,
        find: r#"        const NEGLIGIBLE_WIDTH: i32 = 1;"#,
        replace: r#"        const NEGLIGIBLE_WIDTH: i32 = 0;"#,
        want: r#"a_tiny_cluster_collapses_to_one_unit"#,
    },
    Mutation {
        name: r#"single-array-does-not-collapse"#,
        file: r#"src/placement.rs"#,
        find: r#"    if num_std_cell <= tiny_threshold || single_array_single_std_cell {"#,
        replace: r#"    if num_std_cell <= tiny_threshold {"#,
        want: r#"the_single_array_case_collapses_it_too"#,
    },
    Mutation {
        name: r#"mixed-cluster-inflates-its-whole-area"#,
        file: r#"src/placement.rs"#,
        find: r#"                    mixed_cluster_shape(&m.tilings, m.cluster_std_cell_area, utilization)"#,
        replace: r#"                    mixed_cluster_shape(&m.tilings, m.cluster_area, utilization)"#,
        want: r#"a_mixed_cluster_inflates_only_its_cells"#,
    },
    Mutation {
        name: r#"mixed-cluster-inflates-against-the-first-tiling"#,
        file: r#"src/placement.rs"#,
        find: r#"    let macro_area = tilings.last()?.0 as i64 * tilings.last()?.1 as i64;"#,
        replace: r#"    let macro_area = tilings.first()?.0 as i64 * tilings.first()?.1 as i64;"#,
        want: r#"a_mixed_cluster_inflates_only_its_cells"#,
    },
    // ---------------------------------------------------------------- the placement driver
    // ⚠️ This one must MOVE the recursion, not delete the comment above it. A find/replace whose
    // only difference is a comment is a no-op that can never fail — a badly written mutation, not
    // an equivalent one, and the harness reported it as a hole until it was rewritten.
    Mutation {
        name: r#"children-placed-before-the-parent"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut winning_macros = None;
    let selected = select_run("#,
        // ⚠️ The call must carry the CURRENT arity. This replacement was written against a
        // five-parameter `place_one_parent` and went uncompilable when macro placement and the
        // write-back callback were added — a `replace` can rot exactly like a `find`, and
        // `check-patterns.py` cannot see it.
        replace: r#"    for child in (tree.children)(cluster) {
        if !place_one_parent(
            tree,
            child,
            root,
            place_one,
            place_macros_one,
            on_parent_placed,
            visits,
        ) {
            return false;
        }
    }
    let mut winning_macros = None;
    let selected = select_run("#,
        want: r#"a_parent_is_placed_before_its_children"#,
    },
    Mutation {
        name: r#"driver-skips-clusters-that-do-nothing"#,
        file: r#"src/placement.rs"#,
        find: r#"        PlacementAction::Nothing => {
            visits.push(PlacementVisit { cluster, outcome: ParentOutcome::Leaf });
            return true;
        }"#,
        replace: r#"        PlacementAction::Nothing => {
            return true;
        }"#,
        want: r#"clusters_that_do_nothing_are_still_visited"#,
    },
    Mutation {
        name: r#"driver-continues-past-a-refusal"#,
        file: r#"src/placement.rs"#,
        find: r#"            return false;
        }
    }

    // 🔑 Only now"#,
        replace: r#"        }
    }

    // 🔑 Only now"#,
        want: r#"a_parent_that_cannot_be_placed_stops_the_walk"#,
    },
    Mutation {
        name: r#"driver-reports-the-root-code-everywhere"#,
        file: r#"src/placement.rs"#,
        find: r#"                outcome: ParentOutcome::NoValidSolution(no_valid_solution_error(
                    cluster == root,"#,
        replace: r#"                outcome: ParentOutcome::NoValidSolution(no_valid_solution_error(
                    true,"#,
        want: r#"a_parent_that_cannot_be_placed_stops_the_walk"#,
    },
    Mutation {
        name: r#"setup-adjusts-the-wrong-weight"#,
        file: r#"src/placement.rs"#,
        find: r#"    adjusted.soft_blockage =
        adjusted_soft_blockage_weight(max_level, adjusted.outline, adjusted.soft_blockage);"#,
        replace: r#"    adjusted.notch =
        adjusted_soft_blockage_weight(max_level, weights.outline, weights.soft_blockage);"#,
        want: r#"the_soft_blockage_weight_is_adjusted_once_up_front"#,
    },
    // ---------------------------------------------------------------- temporary std cell places
    Mutation {
        name: r#"std-cell-placed-at-the-cluster-origin"#,
        file: r#"src/placement.rs"#,
        find: r#"    Some((center.0 - cell_extent.0 / 2, center.1 - cell_extent.1 / 2))"#,
        replace: r#"    Some((center.0, center.1))"#,
        want: r#"every_cell_lands_on_the_clusters_centre"#,
    },
    Mutation {
        name: r#"std-cell-half-extent-not-halved"#,
        file: r#"src/placement.rs"#,
        find: r#"    Some((center.0 - cell_extent.0 / 2, center.1 - cell_extent.1 / 2))"#,
        replace: r#"    Some((center.0 - cell_extent.0, center.1 - cell_extent.1))"#,
        want: r#"both_halvings_truncate"#,
    },
    Mutation {
        name: r#"non-leaf-places-its-own-cells"#,
        file: r#"src/placement.rs"#,
        find: r#"    if cluster.is_leaf && cluster.num_std_cell != 0 {"#,
        replace: r#"    if cluster.num_std_cell != 0 {"#,
        want: r#"only_a_leaf_with_cells_places_anything"#,
    },
    Mutation {
        name: r#"leaf-without-cells-still-places"#,
        file: r#"src/placement.rs"#,
        find: r#"    if cluster.is_leaf && cluster.num_std_cell != 0 {
        // ⚠️ Modules first, then the explicit list."#,
        replace: r#"    if cluster.is_leaf {
        // ⚠️ Modules first, then the explicit list."#,
        want: r#"a_leaf_without_cells_places_nothing"#,
    },
    Mutation {
        name: r#"explicit-list-walked-before-modules"#,
        file: r#"src/placement.rs"#,
        find: r#"        for &inst in &cluster.module_core_insts {
            out.push((inst, id));
        }
        for &inst in &cluster.leaf_std_cells {
            out.push((inst, id));
        }"#,
        replace: r#"        for &inst in &cluster.leaf_std_cells {
            out.push((inst, id));
        }
        for &inst in &cluster.module_core_insts {
            out.push((inst, id));
        }"#,
        want: r#"modules_are_walked_before_the_explicit_list"#,
    },
    Mutation {
        name: r#"reset-leaves-resize-alone"#,
        file: r#"src/placement.rs"#,
        find: r#"        resize: 0.0,"#,
        replace: r#"        resize: 0.2,"#,
        want: r#"a_design_without_cells_never_resizes"#,
    },
    Mutation {
        name: r#"reset-leaves-the-fence-alone"#,
        file: r#"src/placement.rs"#,
        find: r#"    weights.fence = 0.0;"#,
        replace: r#"    weights.wirelength = 0.0;"#,
        want: r#"the_reset_is_the_only_path_that_zeroes_the_fence"#,
    },
    Mutation {
        name: r#"wirelength-metric-in-database-units"#,
        file: r#"src/placement.rs"#,
        find: r#"    hpwl_dbu as f64 / dbu_per_micron as f64"#,
        replace: r#"    hpwl_dbu as f64"#,
        want: r#"the_wirelength_metric_is_in_microns"#,
    },
    // ---------------------------------------------------------------- clustering data to the db
    Mutation {
        name: r#"modules-swept-before-the-children-recurse"#,
        file: r#"src/placement.rs"#,
        find: r#"    let slot = out.len();
    out.push((cluster.name.clone(), Vec::new()));
    for &child in &cluster.children {
        create_group(clusters, child, claimed, out);
    }

    for &(inst, is_block) in &cluster.module_insts {"#,
        replace: r#"    let slot = out.len();
    out.push((cluster.name.clone(), Vec::new()));

    for &(inst, is_block) in &cluster.module_insts {"#,
        want: r#"a_child_claims_its_instances_before_the_parent_sweeps"#,
    },
    Mutation {
        name: r#"io-cluster-gets-a-group"#,
        file: r#"src/placement.rs"#,
        find: r#"    if cluster.kind == AreaKind::IoCluster {
        return;
    }"#,
        replace: r#"    if false {
        return;
    }"#,
        want: r#"an_io_cluster_gets_no_group_and_is_not_descended_into"#,
    },
    Mutation {
        name: r#"std-cell-cluster-keeps-module-macros"#,
        file: r#"src/placement.rs"#,
        find: r#"        if is_block && cluster.kind == AreaKind::StdCellCluster {
            continue;
        }"#,
        replace: r#"        if false && is_block && cluster.kind == AreaKind::StdCellCluster {
            continue;
        }"#,
        want: r#"a_std_cell_cluster_skips_macros_from_modules_only"#,
    },
    Mutation {
        name: r#"std-cell-cluster-skips-its-own-macros-too"#,
        file: r#"src/placement.rs"#,
        find: r#"    for &inst in &cluster.leaf_macros {
        if claimed.insert(inst) {
            members.push(inst);
        }
    }"#,
        replace: r#"    for &inst in &cluster.leaf_macros {
        if cluster.kind != AreaKind::StdCellCluster && claimed.insert(inst) {
            members.push(inst);
        }
    }"#,
        want: r#"a_std_cell_cluster_skips_macros_from_modules_only"#,
    },
    Mutation {
        name: r#"instances-claimed-more-than-once"#,
        file: r#"src/placement.rs"#,
        find: r#"    for &inst in &cluster.leaf_std_cells {
        if claimed.insert(inst) {
            members.push(inst);
        }
    }"#,
        replace: r#"    for &inst in &cluster.leaf_std_cells {
        claimed.insert(inst);
        members.push(inst);
    }"#,
        want: r#"an_instance_is_claimed_only_once"#,
    },
    Mutation {
        name: r#"macros-claimed-before-cells"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut members = Vec::new();
    for &inst in &cluster.leaf_std_cells {"#,
        replace: r#"    let mut members = Vec::new();
    for &inst in &cluster.leaf_macros {
        if claimed.insert(inst) {
            members.push(inst);
        }
    }
    for &inst in &cluster.leaf_std_cells {"#,
        want: r#"cells_are_claimed_before_macros"#,
    },
    // ---------------------------------------------------------------- snapping to the grid
    Mutation {
        name: r#"snap-passes-horizontal-first"#,
        file: r#"src/placement.rs"#,
        find: r#"pub const SNAP_PASSES: [SnapAxis; 2] = [SnapAxis::Vertical, SnapAxis::Horizontal];"#,
        replace: r#"pub const SNAP_PASSES: [SnapAxis; 2] = [SnapAxis::Horizontal, SnapAxis::Vertical];"#,
        want: r#"the_snap_passes_are_vertical_then_horizontal"#,
    },
    Mutation {
        name: r#"pin-offset-negation-crossed"#,
        file: r#"src/placement.rs"#,
        find: r#"        SnapAxis::Vertical => matches!(orient, Orient::My | Orient::R180),
        SnapAxis::Horizontal => matches!(orient, Orient::Mx | Orient::R180),"#,
        replace: r#"        SnapAxis::Vertical => matches!(orient, Orient::Mx | Orient::R180),
        SnapAxis::Horizontal => matches!(orient, Orient::My | Orient::R180),"#,
        want: r#"each_axis_is_negated_by_its_own_two_orientations"#,
    },
    Mutation {
        name: r#"pin-offset-negates-rotations-too"#,
        file: r#"src/placement.rs"#,
        find: r#"        SnapAxis::Vertical => matches!(orient, Orient::My | Orient::R180),"#,
        replace: r#"        SnapAxis::Vertical => matches!(orient, Orient::My | Orient::R180 | Orient::Other),"#,
        want: r#"a_rotated_orientation_is_not_negated"#,
    },
    Mutation {
        name: r#"pin-offset-uses-the-full-width"#,
        file: r#"src/placement.rs"#,
        find: r#"    let offset = mterm_min + (pin_width / 2);"#,
        replace: r#"    let offset = mterm_min + pin_width;"#,
        want: r#"the_offset_is_the_terminal_edge_plus_half_the_pin"#,
    },
    Mutation {
        name: r#"grid-rounds-toward-zero"#,
        file: r#"src/placement.rs"#,
        find: r#"    ((origin as f64 / manufacturing_grid as f64).round() * manufacturing_grid as f64) as i32"#,
        replace: r#"    ((origin as f64 / manufacturing_grid as f64).trunc() * manufacturing_grid as f64) as i32"#,
        want: r#"the_grid_rounds_half_away_from_zero"#,
    },
    Mutation {
        name: r#"grid-applied-before-the-offset"#,
        file: r#"src/placement.rs"#,
        find: r#"    align_with_manufacturing_grid(position - pin_offset, manufacturing_grid)"#,
        replace: r#"    align_with_manufacturing_grid(position, manufacturing_grid) - pin_offset"#,
        want: r#"the_offset_is_removed_before_rounding"#,
    },
    Mutation {
        name: r#"snap-takes-the-nearest-track"#,
        file: r#"src/placement.rs"#,
        find: r#"    let index = positions.partition_point(|&p| p < pin_center);"#,
        replace: r#"    let index = positions.partition_point(|&p| p <= pin_center);"#,
        want: r#"a_pin_already_on_a_track_stays"#,
    },
    Mutation {
        name: r#"snap-past-the-last-track-does-not-step-back"#,
        file: r#"src/placement.rs"#,
        find: r#"    Some(if index == positions.len() { index - 1 } else { index })"#,
        replace: r#"    Some(index)"#,
        want: r#"a_pin_past_the_last_track_steps_back"#,
    },
    Mutation {
        name: r#"spiral-negative-first"#,
        file: r#"src/placement.rs"#,
        find: r#"    if i % 2 == 1 {
        (i + 1) / 2
    } else {
        -(i / 2)
    }"#,
        replace: r#"    if i % 2 == 1 {
        -((i + 1) / 2)
    } else {
        i / 2
    }"#,
        want: r#"the_search_spirals_outward_positive_first"#,
    },
    Mutation {
        name: r#"spiral-one-attempt-short"#,
        file: r#"src/placement.rs"#,
        find: r#"    for i in 0..=TOTAL_ATTEMPTS {"#,
        replace: r#"    for i in 0..TOTAL_ATTEMPTS {"#,
        want: r#"the_last_attempt_reaches_fifty_tracks_below_the_start"#,
    },
    Mutation {
        name: r#"aligned-pins-advance-the-wrong-pointer"#,
        file: r#"src/placement.rs"#,
        find: r#"            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,"#,
        replace: r#"            std::cmp::Ordering::Less => j += 1,
            std::cmp::Ordering::Greater => i += 1,"#,
        want: r#"a_pin_short_of_the_current_track_is_dropped"#,
    },
    Mutation {
        name: r#"aligned-pins-counts-near-misses"#,
        file: r#"src/placement.rs"#,
        find: r#"            std::cmp::Ordering::Equal => {
                aligned += 1;
                i += 1;
            }"#,
        replace: r#"            std::cmp::Ordering::Equal => {
                aligned += 1;
                i += 1;
                j += 1;
            }"#,
        want: r#"several_pins_can_share_one_track"#,
    },
    Mutation {
        name: r#"snap-search-does-not-stop-when-complete"#,
        file: r#"src/placement.rs"#,
        find: r#"            if best_aligned == total_pins {
                break;
            }"#,
        replace: r#"            if false {
                break;
            }"#,
        want: r#"the_search_stops_once_everything_aligns"#,
    },
    Mutation {
        name: r#"snap-search-tie-goes-later"#,
        file: r#"src/placement.rs"#,
        find: r#"        if aligned > best_aligned {"#,
        replace: r#"        if aligned >= best_aligned {"#,
        want: r#"a_tie_goes_to_the_earlier_candidate"#,
    },
    Mutation {
        name: r#"snap-search-out-of-range-stops"#,
        file: r#"src/placement.rs"#,
        find: r#"        if current < 0 || current >= position_count as i64 {
            continue;
        }"#,
        replace: r#"        if current < 0 || current >= position_count as i64 {
            break;
        }"#,
        want: r#"the_spiral_steps_past_one_end_and_keeps_going"#,
    },
    Mutation {
        name: r#"layers-not-sorted-by-number"#,
        file: r#"src/placement.rs"#,
        find: r#"    out.sort_by_key(|d| d.layer_number);
    Ok(out)"#,
        replace: r#"    Ok(out)"#,
        want: r#"layers_come_out_sorted_by_layer_number"#,
    },
    Mutation {
        name: r#"snap-pins-not-sorted-by-centre"#,
        file: r#"src/placement.rs"#,
        find: r#"            pins.sort_by_key(|(center, _)| *center);"#,
        replace: r#"            pins.sort_by_key(|(_, iterm)| *iterm);"#,
        want: r#"pins_are_sorted_by_centre_within_a_layer"#,
    },
    Mutation {
        name: r#"snap-keeps-power-pins"#,
        file: r#"src/placement.rs"#,
        find: r#"        if !c.is_signal {
            continue;
        }"#,
        replace: r#"        if false {
            continue;
        }"#,
        want: r#"power_and_ground_pins_are_skipped"#,
    },
    Mutation {
        name: r#"snap-direction-filter-inverted"#,
        file: r#"src/placement.rs"#,
        find: r#"        if c.layer_is_vertical != want_vertical {
            continue;
        }"#,
        replace: r#"        if c.layer_is_vertical == want_vertical {
            continue;
        }"#,
        want: r#"a_pass_sees_only_layers_running_its_way"#,
    },
    Mutation {
        name: r#"missing-track-grid-skipped"#,
        file: r#"src/placement.rs"#,
        find: r#"        if !c.has_track_grid {
            return Err(MissingTrackGrid(c.layer));
        }"#,
        replace: r#"        if !c.has_track_grid {
            continue;
        }"#,
        want: r#"a_layer_without_a_track_grid_is_refused"#,
    },
    Mutation {
        name: r#"track-grid-checked-before-direction"#,
        file: r#"src/placement.rs"#,
        find: r#"        if c.layer_is_vertical != want_vertical {
            continue;
        }
        if !c.has_track_grid {"#,
        replace: r#"        if !c.has_track_grid {
            return Err(MissingTrackGrid(c.layer));
        }
        if c.layer_is_vertical != want_vertical {
            continue;
        }
        if false {"#,
        want: r#"a_wrong_way_layer_without_a_grid_is_not_refused"#,
    },
    Mutation {
        name: r#"duplicate-mpins-deduplicated"#,
        file: r#"src/placement.rs"#,
        find: r#"        grouped.entry(c.layer).or_insert((c.layer_number, Vec::new())).1.push((c.center, c.iterm));"#,
        replace: r#"        let slot = grouped.entry(c.layer).or_insert((c.layer_number, Vec::new()));
        if !slot.1.iter().any(|(_, t)| *t == c.iterm) {
            slot.1.push((c.center, c.iterm));
        }"#,
        want: r#"a_terminal_with_two_master_pins_appears_twice"#,
    },
    // ---------------------------------------------------------------- committing to the database
    Mutation {
        name: r#"location-written-before-orientation"#,
        file: r#"src/placement.rs"#,
        find: r#"    Some(MacroCommit { orientation_first: true, location: real_location, locked: false })"#,
        replace: r#"    Some(MacroCommit { orientation_first: false, location: real_location, locked: false })"#,
        want: r#"orientation_is_written_before_location"#,
    },
    Mutation {
        name: r#"first-write-locks-the-macro"#,
        file: r#"src/placement.rs"#,
        find: r#"    Some(MacroCommit { orientation_first: true, location: real_location, locked: false })"#,
        replace: r#"    Some(MacroCommit { orientation_first: true, location: real_location, locked: true })"#,
        want: r#"the_first_write_does_not_lock_the_macro"#,
    },
    Mutation {
        name: r#"fixed-instance-written-anyway"#,
        file: r#"src/placement.rs"#,
        find: r#"    if inst_is_fixed {
        return None;
    }
    Some(MacroCommit"#,
        replace: r#"    if false {
        return None;
    }
    Some(MacroCommit"#,
        want: r#"a_fixed_instance_is_not_written"#,
    },
    Mutation {
        name: r#"soft-halo-casts-a-blockage"#,
        file: r#"src/placement.rs"#,
        find: r#"    !matches!(halo, HaloKind::Soft)"#,
        replace: r#"    !matches!(halo, HaloKind::None)"#,
        want: r#"a_soft_halo_casts_no_blockage"#,
    },
    Mutation {
        name: r#"fixed-macro-casts-no-blockage"#,
        file: r#"src/placement.rs"#,
        find: r#"        blockage: needs_halo_blockage(halo),"#,
        replace: r#"        blockage: !inst_is_fixed && needs_halo_blockage(halo),"#,
        want: r#"a_fixed_macro_is_not_snapped_but_still_casts_a_blockage"#,
    },
    Mutation {
        name: r#"fixed-macro-still-snapped"#,
        file: r#"src/placement.rs"#,
        find: r#"        snapped: !inst_is_fixed,"#,
        replace: r#"        snapped: true,"#,
        want: r#"a_fixed_macro_is_not_snapped_but_still_casts_a_blockage"#,
    },
    // ---------------------------------------------------------------- orientation correction
    Mutation {
        name: r#"orientation-branch-inverted"#,
        file: r#"src/placement.rs"#,
        find: r#"    if use_full_halo {
        OrientationStrategy::Single
    } else {
        OrientationStrategy::ByCluster
    }"#,
        replace: r#"    if use_full_halo {
        OrientationStrategy::ByCluster
    } else {
        OrientationStrategy::Single
    }"#,
        want: r#"pin_aware_halos_take_the_restricted_path"#,
    },
    Mutation {
        name: r#"flip-tie-is-reverted"#,
        file: r#"src/placement.rs"#,
        find: r#"    !(new_wirelength > original_wirelength)"#,
        replace: r#"    new_wirelength < original_wirelength"#,
        want: r#"an_equal_wirelength_keeps_the_flip"#,
    },
    Mutation {
        name: r#"flip-passes-horizontal-first"#,
        file: r#"src/placement.rs"#,
        find: r#"pub const FLIP_PASSES: [bool; 2] = [true, false];"#,
        replace: r#"pub const FLIP_PASSES: [bool; 2] = [false, true];"#,
        want: r#"the_passes_are_vertical_then_horizontal"#,
    },
    Mutation {
        name: r#"orientation-groups-by-the-wrong-axis"#,
        file: r#"src/placement.rs"#,
        find: r#"        cols.entry(x).or_default().push(id);
        rows.entry(y).or_default().push(id);"#,
        replace: r#"        cols.entry(y).or_default().push(id);
        rows.entry(x).or_default().push(id);"#,
        want: r#"a_macro_is_in_both_a_column_and_a_row"#,
    },
    Mutation {
        name: r#"orientation-groups-in-descending-order"#,
        file: r#"src/placement.rs"#,
        find: r#"    (cols.into_values().collect(), rows.into_values().collect())"#,
        replace: r#"    (cols.into_values().rev().collect(), rows.into_values().rev().collect())"#,
        want: r#"groups_come_out_in_ascending_coordinate_order"#,
    },
    Mutation {
        name: r#"vertical-flip-mirrors-the-wrong-axis"#,
        file: r#"src/placement.rs"#,
        find: r#"    if is_vertical_flip {
        // `flipY`: mirror about the vertical axis."#,
        replace: r#"    if !is_vertical_flip {
        // `flipY`: mirror about the vertical axis."#,
        want: r#"a_vertical_flip_mirrors_about_the_vertical_axis"#,
    },
    Mutation {
        name: r#"half-turn-flips-to-itself"#,
        file: r#"src/placement.rs"#,
        find: r#"            Orient::R180 => Orient::Mx,
            Orient::Mx => Orient::R180,"#,
        replace: r#"            Orient::R180 => Orient::R180,
            Orient::Mx => Orient::R180,"#,
        want: r#"a_half_turn_flips_to_the_other_mirror"#,
    },
    Mutation {
        name: r#"flip-moves-the-macro"#,
        file: r#"src/placement.rs"#,
        find: r#"    (flip_orientation(orient, is_vertical_flip), real_location)"#,
        replace: r#"    (flip_orientation(orient, is_vertical_flip), (0, 0))"#,
        want: r#"the_location_survives_the_flip"#,
    },
    Mutation {
        name: r#"pin-without-geometry-placed-at-zero"#,
        file: r#"src/placement.rs"#,
        find: r#"            NetTerminal::Instance(None) => continue,"#,
        replace: r#"            NetTerminal::Instance(None) => (0, 0),"#,
        want: r#"a_pin_without_geometry_is_skipped_not_placed_at_zero"#,
    },
    Mutation {
        name: r#"real-wirelength-is-full-perimeter"#,
        file: r#"src/placement.rs"#,
        find: r#"            wirelength += (b.2 - b.0) as i64 + (b.3 - b.1) as i64;"#,
        replace: r#"            wirelength += 2 * ((b.2 - b.0) as i64 + (b.3 - b.1) as i64);"#,
        want: r#"the_wirelength_is_the_summed_half_perimeter"#,
    },
    Mutation {
        name: r#"real-wirelength-deduplicates-nets"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut wirelength: i64 = 0;
    for terminals in nets_of_macro_pins {"#,
        replace: r#"    let mut wirelength: i64 = 0;
    let mut seen: Vec<&Vec<NetTerminal>> = Vec::new();
    for terminals in nets_of_macro_pins {
        if seen.contains(&terminals) {
            continue;
        }
        seen.push(terminals);"#,
        want: r#"a_net_reached_by_two_pins_is_counted_twice"#,
    },
    // ---------------------------------------------------------------- the boundary push
    Mutation {
        name: r#"push-descends-into-std-cell-clusters"#,
        file: r#"src/placement.rs"#,
        find: r#"            ClusterType::Mixed => {
                out.extend(fetch_macro_clusters(child, type_of, children_of));
            }
            ClusterType::StdCell => {}"#,
        replace: r#"            ClusterType::Mixed | ClusterType::StdCell => {
                out.extend(fetch_macro_clusters(child, type_of, children_of));
            }"#,
        want: r#"a_macro_cluster_under_a_std_cell_cluster_is_never_fetched"#,
    },
    Mutation {
        name: r#"push-does-not-descend-at-all"#,
        file: r#"src/placement.rs"#,
        find: r#"                out.extend(fetch_macro_clusters(child, type_of, children_of));"#,
        replace: r#"                let _ = child;"#,
        want: r#"macro_clusters_are_fetched_depth_first_through_mixed_clusters"#,
    },
    Mutation {
        name: r#"centralized-array-uses-the-cluster-area"#,
        file: r#"src/placement.rs"#,
        find: r#"                if soft_macro_area != 0 {
                    return false;
                }"#,
        replace: r#"                let _ = soft_macro_area;"#,
        want: r#"a_cell_cluster_that_was_not_shrunk_fails_the_test"#,
    },
    Mutation {
        name: r#"centralized-array-allows-a-mixed-cluster"#,
        file: r#"src/placement.rs"#,
        find: r#"            ClusterType::Mixed => return false,
            ClusterType::HardMacro => macro_cluster_count += 1,"#,
        replace: r#"            ClusterType::Mixed => {}
            ClusterType::HardMacro => macro_cluster_count += 1,"#,
        want: r#"a_mixed_cluster_fails_it_at_once"#,
    },
    // ⚠️ The first spelling of this mutation skipped a Mixed child only when its area was zero —
    // and the test's IO cluster HAS area zero, so the mutant behaved identically and reported as a
    // hole. A mutation must break the rule for the fixture that pins it, not for some other input.
    // ⛔ **The rule this replaces was the WRONG WAY ROUND.** Its predecessor asserted that an IO
    // cluster and a fixed macro cluster are both ignored by the guard, and named a test that has
    // since been deleted for saying the same thing. Both are read through `getClusterType()`,
    // where an IO cluster is `Mixed` and a fixed macro cluster is `HardMacro` — so each is
    // load-bearing rather than ignored, and this is the mutation that says so.
    Mutation {
        name: r#"centralized-array-skips-io-clusters"#,
        file: r#"src/placement.rs"#,
        find: r#"pub fn has_single_centralized_macro_array(children: &[(crate::cluster::ClusterType, i64)]) -> bool {
    use crate::cluster::ClusterType;
    let mut macro_cluster_count = 0;
    for &(cluster_type, soft_macro_area) in children {"#,
        replace: r#"pub fn has_single_centralized_macro_array(children: &[(crate::cluster::ClusterType, i64)]) -> bool {
    use crate::cluster::ClusterType;
    let mut macro_cluster_count = 0;
    for &(cluster_type, soft_macro_area) in children.iter().filter(|c| c.0 != ClusterType::Mixed) {"#,
        want: r#"an_io_cluster_fails_the_guard_because_its_type_is_mixed"#,
    },
    Mutation {
        name: r#"centralized-array-allows-two-arrays"#,
        file: r#"src/placement.rs"#,
        find: r#"        if macro_cluster_count > 1 {
            return false;
        }"#,
        replace: r#"        if macro_cluster_count > 2 {
            return false;
        }"#,
        want: r#"two_macro_clusters_fail_it"#,
    },
    Mutation {
        name: r#"centralized-array-does-not-count-a-fixed-cluster"#,
        file: r#"src/placement.rs"#,
        find: r#"            ClusterType::HardMacro => macro_cluster_count += 1,
            ClusterType::StdCell => {"#,
        replace: r#"            ClusterType::HardMacro => {
                if soft_macro_area != 999 {
                    macro_cluster_count += 1;
                }
            }
            ClusterType::StdCell => {"#,
        want: r#"a_fixed_macro_cluster_counts_towards_the_two"#,
    },
    Mutation {
        name: r#"push-guards-in-the-wrong-order"#,
        file: r#"src/placement.rs"#,
        find: r#"    if root_type == crate::cluster::ClusterType::HardMacro {
        return Err(NoPush::DesignIsAllMacros);
    }
    if has_single_centralized_macro_array(root_children) {
        return Err(NoPush::SingleCentralizedMacroArray);
    }"#,
        replace: r#"    if has_single_centralized_macro_array(root_children) {
        return Err(NoPush::SingleCentralizedMacroArray);
    }
    if root_type == crate::cluster::ClusterType::HardMacro {
        return Err(NoPush::DesignIsAllMacros);
    }"#,
        want: r#"the_all_macro_guard_is_checked_first"#,
    },
    Mutation {
        name: r#"overlap-is-inclusive"#,
        file: r#"src/placement.rs"#,
        find: r#"    b.2 > a.0 && b.0 < a.2 && b.3 > a.1 && b.1 < a.3
}"#,
        replace: r#"    b.2 >= a.0 && b.0 <= a.2 && b.3 >= a.1 && b.1 <= a.3
}"#,
        want: r#"touching_boxes_do_not_overlap"#,
    },
    Mutation {
        name: r#"hard-macro-moves-on-both-axes"#,
        file: r#"src/placement.rs"#,
        find: r#"        Boundary::L => (location.0 - distance, location.1),"#,
        replace: r#"        Boundary::L => (location.0 - distance, location.1 - distance),"#,
        want: r#"a_hard_macro_moves_on_one_axis_only"#,
    },
    Mutation {
        name: r#"cluster-obstructs-itself"#,
        file: r#"src/placement.rs"#,
        find: r#"        if owner == cluster_id {
            continue;
        }"#,
        replace: r#"        if false {
            continue;
        }"#,
        want: r#"a_cluster_does_not_obstruct_itself"#,
    },
    Mutation {
        name: r#"io-blockage-reported-before-a-macro"#,
        file: r#"src/placement.rs"#,
        find: r#"    for (i, &(owner, bbox)) in hard_macros.iter().enumerate() {"#,
        replace: r#"    for (i, &blockage) in io_blockages.iter().enumerate() {
        if boxes_overlap(cluster_box, blockage) {
            return Some(PushObstacle::IoBlockage(i));
        }
    }
    for (i, &(owner, bbox)) in hard_macros.iter().enumerate() {"#,
        want: r#"a_hard_macro_is_reported_before_an_io_blockage"#,
    },
    Mutation {
        name: r#"io-blockages-not-tested"#,
        file: r#"src/placement.rs"#,
        find: r#"    for (i, &blockage) in io_blockages.iter().enumerate() {
        if boxes_overlap(cluster_box, blockage) {
            return Some(PushObstacle::IoBlockage(i));
        }
    }
    None"#,
        replace: r#"    None"#,
        want: r#"an_io_blockage_obstructs_on_its_own"#,
    },
    Mutation {
        name: r#"push-tie-goes-left"#,
        file: r#"src/placement.rs"#,
        find: r#"    let (hor_boundary, smaller_hor) = if distance_to_left < distance_to_right {"#,
        replace: r#"    let (hor_boundary, smaller_hor) = if distance_to_left <= distance_to_right {"#,
        want: r#"a_tie_goes_right_and_top"#,
    },
    Mutation {
        name: r#"push-tie-goes-bottom"#,
        file: r#"src/placement.rs"#,
        find: r#"    let (ver_boundary, smaller_ver) = if distance_to_bottom < distance_to_top {"#,
        replace: r#"    let (ver_boundary, smaller_ver) = if distance_to_bottom <= distance_to_top {"#,
        want: r#"a_tie_goes_right_and_top"#,
    },
    Mutation {
        name: r#"push-threshold-crossed"#,
        file: r#"src/placement.rs"#,
        find: r#"    if smaller_hor < macro_width {"#,
        replace: r#"    if smaller_hor < macro_height {"#,
        want: r#"each_axis_is_measured_against_its_own_macro_dimension"#,
    },
    Mutation {
        name: r#"push-distance-not-absolute"#,
        file: r#"src/placement.rs"#,
        find: r#"    let distance_to_left = (cluster_box.0 - core.0).abs();"#,
        replace: r#"    let distance_to_left = cluster_box.0 - core.0;"#,
        want: r#"a_cluster_outside_the_core_is_pushed_further_out"#,
    },
    Mutation {
        name: r#"push-boundaries-in-decision-order"#,
        file: r#"src/placement.rs"#,
        find: r#"    found.sort_by_key(|(b, _)| *b);"#,
        replace: r#"    found.reverse();"#,
        want: r#"the_boundaries_come_out_in_enum_order"#,
    },
    Mutation {
        name: r#"push-direction-inverted"#,
        file: r#"src/placement.rs"#,
        find: r#"        Boundary::L => (-distance, 0),
        Boundary::R => (distance, 0),"#,
        replace: r#"        Boundary::L => (distance, 0),
        Boundary::R => (-distance, 0),"#,
        want: r#"the_direction_comes_from_the_boundary_not_the_sign"#,
    },
    Mutation {
        name: r#"push-does-not-compose"#,
        file: r#"src/placement.rs"#,
        find: r#"            cluster_box = moved;"#,
        replace: r#"            let _ = moved;"#,
        want: r#"the_two_pushes_compose"#,
    },
    Mutation {
        name: r#"push-stops-after-a-revert"#,
        file: r#"src/placement.rs"#,
        find: r#"                attempts.push(PushAttempt { boundary, distance, committed: false, obstacle: Some(obstacle) })
            }"#,
        replace: r#"                attempts.push(PushAttempt { boundary, distance, committed: false, obstacle: Some(obstacle) });
                break;
            }"#,
        want: r#"a_reverted_push_does_not_block_the_next"#,
    },
    Mutation {
        name: r#"push-zero-distance-attempted"#,
        file: r#"src/placement.rs"#,
        find: r#"        if distance == 0 {
            continue;
        }
        let moved = move_towards_boundary(cluster_box, boundary, distance);"#,
        replace: r#"        let moved = move_towards_boundary(cluster_box, boundary, distance);"#,
        want: r#"a_zero_distance_is_skipped_entirely"#,
    },
    Mutation {
        name: r#"push-reverted-attempt-not-recorded"#,
        file: r#"src/placement.rs"#,
        find: r#"            Some(obstacle) => {
                attempts.push(PushAttempt { boundary, distance, committed: false, obstacle: Some(obstacle) })
            }"#,
        replace: r#"            Some(obstacle) => {
                let _ = obstacle;
            }"#,
        want: r#"a_reverted_push_is_still_an_attempt"#,
    },
    // ---------------------------------------------------------------- the push COMPOSITION
    //
    // 🔑 **Witnessed only by `placement_push_upstream.rs`**, the port of upstream's own
    // `TestPusher.cpp`. No design in the 34-case regression suite reaches an all-macro root, a
    // fixed cluster inside the push loop, the HARD-MACRO revert, or a cluster too far from every
    // edge to move — so before that file existed these rules had no unit witness and every one of
    // these mutations would have reported as a hole.
    Mutation {
        name: r#"push-header-only-when-something-moves"#,
        file: r#"src/placement.rs"#,
        find: r#"        out.push("Distance to Close Boundaries:".to_string());"#,
        replace: r#"        if !distances.is_empty() {
            out.push("Distance to Close Boundaries:".to_string());
        }"#,
        want: r#"a_cluster_far_from_every_edge_still_prints_the_header"#,
    },
    Mutation {
        name: r#"push-moved-printed-only-when-committed"#,
        file: r#"src/placement.rs"#,
        find: r#"        for attempt in attempts {
            out.push(format!("#,
        replace: r#"        for attempt in attempts.iter().filter(|a| a.committed) {
            out.push(format!("#,
        want: r#"a_horizontal_push_onto_another_macro_is_reverted"#,
    },
    Mutation {
        name: r#"push-visits-fixed-clusters"#,
        file: r#"src/placement.rs"#,
        find: r#"        if cluster.is_fixed_macro {
            continue;
        }"#,
        replace: r#"        if false {
            continue;
        }"#,
        want: r#"a_fixed_macro_cluster_is_skipped_without_a_trace_line"#,
    },
    Mutation {
        name: r#"push-obstacles-exclude-fixed-clusters"#,
        file: r#"src/placement.rs"#,
        find: r#"        let flat: Vec<(i32, (i32, i32, i32, i32))> =
            macros.iter().map(|m| (m.cluster_id, m.bbox())).collect();"#,
        replace: r#"        let flat: Vec<(i32, (i32, i32, i32, i32))> = macros
            .iter()
            .filter(|m| !clusters.iter().any(|c| c.id == m.cluster_id && c.is_fixed_macro))
            .map(|m| (m.cluster_id, m.bbox()))
            .collect();"#,
        want: r#"a_fixed_macro_cluster_still_obstructs_another_clusters_push"#,
    },
    // ⛔ **The obvious spelling of this one is EQUIVALENT and was replaced, not kept.** Turning the
    // commit arm's `None =>` into `_ =>` changes nothing: Rust matches arms IN ORDER, so the two
    // `Some(...)` arms above still win and `_` catches exactly what `None` caught. It reported as a
    // hole on 2026-08-28 and was not one. This version breaks the rule for real, by making every
    // attempt take the commit arm.
    Mutation {
        name: r#"push-commits-macros-even-when-reverted"#,
        file: r#"src/placement.rs"#,
        find: r#"            match attempt.obstacle {"#,
        replace: r#"            match None::<PushObstacle> {"#,
        want: r#"a_horizontal_push_onto_another_macro_is_reverted"#,
    },
    Mutation {
        name: r#"push-threshold-from-the-wrong-macro"#,
        file: r#"src/placement.rs"#,
        find: r#"        let Some(&first) = cluster.macros.first() else { continue };"#,
        replace: r#"        let Some(&first) = cluster.macros.last() else { continue };"#,
        want: r#"the_push_threshold_comes_from_the_first_macro_not_the_last"#,
    },
    // ---------------------------------------------------------------- temporary macro clusters
    Mutation {
        name: r#"temp-cluster-ids-rewind"#,
        file: r#"src/placement.rs"#,
        find: r#"    out.distinct_masters = distinct.len();
    out
}"#,
        replace: r#"    out.distinct_masters = distinct.len();
    out.next_cluster_id = first_id;
    out
}"#,
        want: r#"cluster_ids_are_consumed_permanently"#,
    },
    Mutation {
        name: r#"temp-cluster-macro-id-after-push"#,
        file: r#"src/placement.rs"#,
        find: r#"            macro_id: index,"#,
        replace: r#"            macro_id: index + 1,"#,
        want: r#"the_macro_id_is_the_position_in_the_list"#,
    },
    Mutation {
        name: r#"temp-cluster-masters-counted-total"#,
        file: r#"src/placement.rs"#,
        find: r#"    out.distinct_masters = distinct.len();"#,
        replace: r#"    out.distinct_masters = macro_names.len().min(masters.len());"#,
        want: r#"the_master_count_is_distinct_not_total"#,
    },
    Mutation {
        name: r#"temp-cluster-named-by-index"#,
        file: r#"src/placement.rs"#,
        find: r#"            name: name.clone(),"#,
        replace: r#"            name: format!("cluster_{index}"),"#,
        want: r#"one_temporary_cluster_per_macro_named_after_it"#,
    },
    Mutation {
        name: r#"temp-cluster-ids-all-the-same"#,
        file: r#"src/placement.rs"#,
        find: r#"        out.next_cluster_id += 1;"#,
        replace: r#"        out.next_cluster_id += 0;"#,
        want: r#"cluster_ids_are_consumed_permanently"#,
    },
    // ---------------------------------------------------------------- the hard-macro netlist
    Mutation {
        name: r#"terminals-in-connection-order"#,
        file: r#"src/placement.rs"#,
        find: r#"    let unique: std::collections::BTreeSet<i32> = connected_ids.iter().copied().collect();
    unique.into_iter().filter(|id| !is_already_a_macro(*id)).collect()"#,
        replace: r#"    let mut seen = std::collections::BTreeSet::new();
    connected_ids
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .filter(|id| !is_already_a_macro(*id))
        .collect()"#,
        want: r#"terminals_are_created_in_ascending_cluster_id_order"#,
    },
    Mutation {
        name: r#"placed-cluster-also-made-a-terminal"#,
        file: r#"src/placement.rs"#,
        find: r#"    unique.into_iter().filter(|id| !is_already_a_macro(*id)).collect()"#,
        replace: r#"    unique.into_iter().collect()"#,
        want: r#"a_cluster_already_being_placed_is_not_a_terminal"#,
    },
    Mutation {
        name: r#"hard-nets-halved-by-an-id-filter"#,
        file: r#"src/placement.rs"#,
        find: r#"            nets.push(BundledNet { source, target, weight });"#,
        replace: r#"            if *cluster_id > target_id {
                nets.push(BundledNet { source, target, weight });
            }"#,
        want: r#"every_connection_is_emitted_from_both_ends"#,
    },
    Mutation {
        name: r#"hard-nets-drop-self-connections"#,
        file: r#"src/placement.rs"#,
        find: r#"        for &(target_id, weight) in connections {"#,
        replace: r#"        for &(target_id, weight) in connections.iter().filter(|c| c.0 != *cluster_id) {"#,
        want: r#"a_self_connection_survives"#,
    },
    Mutation {
        name: r#"unmapped-cluster-skipped-not-refused"#,
        file: r#"src/placement.rs"#,
        find: r#"            let Some(target) = macro_of(target_id) else {
                return Err(UnmappedCluster(target_id));
            };"#,
        replace: r#"            let Some(target) = macro_of(target_id) else {
                continue;
            };"#,
        want: r#"a_cluster_missing_from_the_macro_map_is_an_error"#,
    },
    // ---------------------------------------------------------------- the hard-macro core
    Mutation {
        name: r#"hard-cost-includes-the-soft-penalties"#,
        file: r#"src/placement.rs"#,
        find: r#"        &crate::anneal::Penalties {
            boundary: 0.0,
            soft_blockage: 0.0,
            fixed_macros: 0.0,
            notch: 0.0,
            ..*p
        },"#,
        replace: r#"        p,"#,
        want: r#"the_four_soft_only_penalties_are_ignored"#,
    },
    Mutation {
        name: r#"hard-perturb-has-a-fourth-threshold"#,
        file: r#"src/placement.rs"#,
        find: r#"        } else {
            crate::anneal::Action::Exchange
        }
    }
}"#,
        replace: r#"        } else if draw <= three + self.exchange {
            crate::anneal::Action::Exchange
        } else {
            crate::anneal::Action::Resize
        }
    }
}"#,
        want: r#"a_draw_past_every_threshold_is_an_exchange"#,
    },
    Mutation {
        name: r#"hard-action-thresholds-exclusive"#,
        file: r#"src/placement.rs"#,
        find: r#"        if draw <= one {
            crate::anneal::Action::SwapPositive"#,
        replace: r#"        if draw < one {
            crate::anneal::Action::SwapPositive"#,
        want: r#"exchange_takes_everything_past_the_third_threshold"#,
    },
    Mutation {
        name: r#"norm-floor-exclusive"#,
        file: r#"src/placement.rs"#,
        find: r#"    if value <= 1e-4 {
        1.0
    } else {
        value
    }"#,
        replace: r#"    if value < 1e-4 {
        1.0
    } else {
        value
    }"#,
        want: r#"a_tiny_normalisation_factor_becomes_exactly_one"#,
    },
    Mutation {
        name: r#"norm-floor-clamps-instead-of-replacing"#,
        file: r#"src/placement.rs"#,
        find: r#"    if value <= 1e-4 {
        1.0"#,
        replace: r#"    if value <= 1e-4 {
        1e-4"#,
        want: r#"a_tiny_normalisation_factor_becomes_exactly_one"#,
    },
    Mutation {
        name: r#"temperature-from-the-spread"#,
        file: r#"src/placement.rs"#,
        find: r#"        delta_cost += (costs[i] - costs[i - 1]).abs();"#,
        replace: r#"        delta_cost += (costs[i] - costs[0]).abs();"#,
        want: r#"the_temperature_measures_change_not_spread"#,
    },
    Mutation {
        name: r#"temperature-divides-by-the-sample-count"#,
        file: r#"src/placement.rs"#,
        find: r#"        -(delta_cost / (costs.len() - 1) as f32) / init_prob.ln()"#,
        replace: r#"        -(delta_cost / costs.len() as f32) / init_prob.ln()"#,
        want: r#"the_temperature_measures_change_not_spread"#,
    },
    Mutation {
        name: r#"still-sweep-does-not-give-one"#,
        file: r#"src/placement.rs"#,
        find: r#"    if costs.len() > 1 && delta_cost > 0.0 {"#,
        replace: r#"    if costs.len() > 1 {"#,
        want: r#"a_still_or_empty_sweep_gives_a_temperature_of_one"#,
    },
    Mutation {
        name: r#"hard-sampled-width-kept-as-int"#,
        file: r#"src/placement.rs"#,
        find: r#"    extent as f32 as i32
}"#,
        replace: r#"    extent
}"#,
        want: r#"a_hard_cores_sampled_width_makes_a_lossy_round_trip"#,
    },
    // ---------------------------------------------------------------- per-macro-cluster inputs
    Mutation {
        name: r#"missing-fence-dropped-instead-of-degenerate"#,
        file: r#"src/placement.rs"#,
        find: r#"    let clipped = if rects_intersect(region, outline) {"#,
        replace: r#"    let clipped = if true {"#,
        want: r#"a_missing_fence_becomes_a_degenerate_box_at_a_negative_position"#,
    },
    Mutation {
        name: r#"fence-clipping-exclusive"#,
        file: r#"src/placement.rs"#,
        find: r#"    b.2 >= a.0 && b.0 <= a.2 && b.3 >= a.1 && b.1 <= a.3"#,
        replace: r#"    b.2 > a.0 && b.0 < a.2 && b.3 > a.1 && b.1 < a.3"#,
        want: r#"a_touching_fence_survives_as_a_zero_width_box"#,
    },
    Mutation {
        name: r#"fence-not-rebased-in-macro-placement"#,
        file: r#"src/placement.rs"#,
        find: r#"    (clipped.0 - outline.0, clipped.1 - outline.1, clipped.2 - outline.0, clipped.3 - outline.1)
}"#,
        replace: r#"    clipped
}"#,
        want: r#"a_fence_is_clipped_and_rebased"#,
    },
    Mutation {
        name: r#"degenerate-fence-skipped-by-area"#,
        file: r#"src/placement.rs"#,
        find: r#"        if let Some(fence) = fence_of(i) {
            fences.push((i, clip_region_to_outline(fence, outline)));
        }"#,
        replace: r#"        if let Some(fence) = fence_of(i) {
            let c = clip_region_to_outline(fence, outline);
            if (c.2 - c.0) > 0 && (c.3 - c.1) > 0 {
                fences.push((i, c));
            }
        }"#,
        want: r#"every_macro_with_a_fence_gets_an_entry"#,
    },
    Mutation {
        name: r#"array-columns-rounded-not-truncated"#,
        file: r#"src/placement.rs"#,
        find: r#"    let columns = if macro_width != 0 { cluster_width / macro_width } else { 0 };"#,
        replace: r#"    let columns = if macro_width != 0 {
        (cluster_width as f64 / macro_width as f64).round() as i32
    } else {
        0
    };"#,
        want: r#"the_column_count_truncates_rather_than_rounding"#,
    },
    Mutation {
        name: r#"array-grid-walked-row-major"#,
        file: r#"src/placement.rs"#,
        find: r#"            let macro_id = (rows * i) - j;"#,
        replace: r#"            let macro_id = (columns * j) - i;"#,
        want: r#"the_grid_is_encoded_column_by_column_downwards"#,
    },
    Mutation {
        name: r#"array-gap-not-reported"#,
        file: r#"src/placement.rs"#,
        find: r#"            } else {
                out.has_empty_space = true;
            }"#,
        replace: r#"            } else {
            }"#,
        want: r#"a_gap_in_the_grid_is_reported"#,
    },
    Mutation {
        name: r#"array-undersized-grid-reports-a-gap"#,
        file: r#"src/placement.rs"#,
        find: r#"    let mut out = ArraySequencePair { pos: (0..macro_count).collect(), ..Default::default() };"#,
        replace: r#"    let mut out = ArraySequencePair { pos: (0..macro_count).collect(), ..Default::default() };
    if macro_width != 0 && macro_height != 0 {
        let c = (cluster_width / macro_width) as usize;
        let r = (cluster_height / macro_height) as usize;
        if c * r < macro_count {
            out.has_empty_space = true;
        }
    }"#,
        want: r#"an_undersized_grid_reports_no_empty_space_and_a_short_sequence"#,
    },
    // ---------------------------------------------------------------- macro-placement runs
    Mutation {
        name: r#"exchange-not-scaled-by-master-sharing"#,
        file: r#"src/placement.rs"#,
        find: r#"    let exchange = exchange * 5.0 * sharing;"#,
        replace: r#"    let exchange = exchange * 5.0;"#,
        want: r#"exchange_is_switched_off_when_no_master_is_shared"#,
    },
    Mutation {
        name: r#"double-swap-scaled-by-ten-too"#,
        file: r#"src/placement.rs"#,
        find: r#"    let action_sum = pos_swap * 10.0 + neg_swap * 10.0 + double_swap + exchange;"#,
        replace: r#"    let action_sum = pos_swap * 10.0 + neg_swap * 10.0 + double_swap * 10.0 + exchange;"#,
        want: r#"the_double_swap_is_ten_times_rarer_than_a_single_one"#,
    },
    Mutation {
        name: r#"single-swaps-not-scaled"#,
        file: r#"src/placement.rs"#,
        find: r#"        pos_swap: pos_swap * 10.0 / action_sum,"#,
        replace: r#"        pos_swap: pos_swap / action_sum,"#,
        want: r#"the_double_swap_is_ten_times_rarer_than_a_single_one"#,
    },
    Mutation {
        name: r#"perturbation-floor-not-a-tenth"#,
        file: r#"src/placement.rs"#,
        find: r#"    let minimum = num_perturb_per_step / 10;"#,
        replace: r#"    let minimum = num_perturb_per_step;"#,
        want: r#"a_small_cluster_gets_the_floor_not_its_macro_count"#,
    },
    Mutation {
        name: r#"large-cluster-still-takes-the-floor"#,
        file: r#"src/placement.rs"#,
        find: r#"    if large {
        macro_count
    } else {
        minimum
    }"#,
        replace: r#"    minimum"#,
        want: r#"a_large_cluster_is_perturbed_once_per_macro"#,
    },
    Mutation {
        name: r#"large-array-gets-no-exception"#,
        file: r#"src/placement.rs"#,
        find: r#"    if is_macro_array && large {
        return num_perturb_per_step;
    }"#,
        replace: r#"    if false {
        return num_perturb_per_step;
    }"#,
        want: r#"a_large_macro_array_gets_the_full_count"#,
    },
    Mutation {
        name: r#"full-array-keeps-its-probabilities"#,
        file: r#"src/placement.rs"#,
        find: r#"    MacroArraySetup {
        probabilities: HardActionProbabilities {
            pos_swap: 0.0,
            neg_swap: 0.0,
            double_swap: 0.0,
            exchange: 1.0,
        },
        invalid_states_allowed: true,
    }"#,
        replace: r#"    MacroArraySetup { probabilities, invalid_states_allowed: true }"#,
        want: r#"a_full_array_only_exchanges"#,
    },
    Mutation {
        name: r#"empty-space-array-still-allows-invalid"#,
        file: r#"src/placement.rs"#,
        find: r#"    if array_has_empty_space {
        return MacroArraySetup { probabilities, invalid_states_allowed: false };
    }"#,
        replace: r#"    if array_has_empty_space {
        return MacroArraySetup { probabilities, invalid_states_allowed: true };
    }"#,
        want: r#"an_array_with_empty_space_disallows_invalid_states"#,
    },
    Mutation {
        name: r#"non-array-cluster-treated-as-an-array"#,
        file: r#"src/placement.rs"#,
        find: r#"    if !is_macro_array {
        return MacroArraySetup { probabilities, invalid_states_allowed: true };
    }"#,
        replace: r#"    if false {
        return MacroArraySetup { probabilities, invalid_states_allowed: true };
    }"#,
        want: r#"a_non_array_cluster_is_untouched"#,
    },
    Mutation {
        name: r#"run-ramp-off-by-one"#,
        file: r#"src/placement.rs"#,
        find: r#"    w.outline *= ((run_id + 1) * 10) as f32;"#,
        replace: r#"    w.outline *= (run_id * 10) as f32;"#,
        want: r#"each_run_is_a_harder_version_of_the_last"#,
    },
    Mutation {
        name: r#"run-ramp-multiplies-wirelength"#,
        file: r#"src/placement.rs"#,
        find: r#"    w.wirelength /= (run_id + 1) as f32;"#,
        replace: r#"    w.wirelength *= (run_id + 1) as f32;"#,
        want: r#"each_run_is_a_harder_version_of_the_last"#,
    },
    Mutation {
        name: r#"every-run-shares-one-seed"#,
        file: r#"src/placement.rs"#,
        find: r#"    random_seed.wrapping_add(run_id as u32)"#,
        replace: r#"    random_seed"#,
        want: r#"each_run_gets_its_own_seed"#,
    },
    Mutation {
        name: r#"first-valid-macro-run-wins"#,
        file: r#"src/placement.rs"#,
        find: r#"            Some((_, best_cost)) if cost >= best_cost => {}"#,
        replace: r#"            Some(_) => {}"#,
        want: r#"the_cheapest_valid_run_wins"#,
    },
    Mutation {
        name: r#"macro-run-tie-goes-to-the-later-run"#,
        file: r#"src/placement.rs"#,
        find: r#"            Some((_, best_cost)) if cost >= best_cost => {}"#,
        replace: r#"            Some((_, best_cost)) if cost > best_cost => {}"#,
        want: r#"a_tie_goes_to_the_earlier_run"#,
    },
    Mutation {
        name: r#"invalid-macro-run-can-win"#,
        file: r#"src/placement.rs"#,
        find: r#"        if !is_valid {
            continue;
        }
        match best {"#,
        replace: r#"        if false {
            continue;
        }
        match best {"#,
        want: r#"an_invalid_run_is_never_chosen"#,
    },
    // ---------------------------------------------------------------- closing out a parent
    Mutation {
        name: r#"leaf-tested-before-macro-cluster"#,
        file: r#"src/placement.rs"#,
        find: r#"    if kind == AreaKind::HardMacroCluster || is_fixed_macro {"#,
        replace: r#"    if is_leaf {
        return PlacementAction::Nothing;
    }
    if kind == AreaKind::HardMacroCluster || is_fixed_macro {"#,
        want: r#"a_leaf_macro_cluster_is_placed_not_skipped"#,
    },
    Mutation {
        name: r#"fixed-macro-cluster-not-distinguished"#,
        file: r#"src/placement.rs"#,
        find: r#"        return if is_fixed_macro {
            PlacementAction::PlaceMacrosButRefused
        } else {
            PlacementAction::PlaceMacros
        };"#,
        replace: r#"        return PlacementAction::PlaceMacros;"#,
        want: r#"a_fixed_macro_cluster_reaches_macro_placement_and_is_refused"#,
    },
    Mutation {
        name: r#"io-clusters-written-back-too"#,
        file: r#"src/placement.rs"#,
        find: r#"        if *kind == AreaKind::IoCluster {
            continue;
        }
        let Some(id) = assembly.id(name) else {"#,
        replace: r#"        if false {
            continue;
        }
        let Some(id) = assembly.id(name) else {"#,
        want: r#"an_io_clusters_own_soft_macro_is_left_alone"#,
    },
    Mutation {
        name: r#"fixed-macro-cluster-not-written-back"#,
        file: r#"src/placement.rs"#,
        find: r#"        if *kind == AreaKind::IoCluster {
            continue;
        }"#,
        replace: r#"        if *kind == AreaKind::IoCluster || *kind == AreaKind::FixedMacro {
            continue;
        }"#,
        want: r#"a_fixed_macro_clusters_soft_macro_is_overwritten"#,
    },
    Mutation {
        name: r#"unknown-child-skipped-not-refused"#,
        file: r#"src/placement.rs"#,
        find: r#"        let Some(id) = assembly.id(name) else {
            return Err(UnknownChild(name.clone()));
        };"#,
        replace: r#"        let Some(id) = assembly.id(name) else {
            continue;
        };"#,
        want: r#"a_child_missing_from_the_id_map_is_an_error"#,
    },
    // ---------------------------------------------------------------- numeric types
    Mutation {
        name: r#"double-added-into-float-narrows-first"#,
        file: r#"src/anneal.rs"#,
        find: r#"    (accumulator as f64 + addend) as f32
}"#,
        replace: r#"    accumulator + addend as f32
}"#,
        want: r#"adding_a_double_into_a_float_rounds_only_once"#,
    },
    Mutation {
        name: r#"guidance-narrows-each-addend"#,
        file: r#"src/placement.rs"#,
        find: r#"        penalty = crate::anneal::plus_double(penalty, area_to_microns_f64(best, dbu_per_micron));"#,
        replace: r#"        penalty += area_to_microns_f64(best, dbu_per_micron) as f32;"#,
        want: r#"the_guidance_sum_is_formed_in_f64_and_rounded_once"#,
    },
    Mutation {
        name: r#"io-charge-computed-in-f64"#,
        file: r#"src/placement.rs"#,
        find: r#"        return (net_weight * max_dist as f32) as i64;"#,
        replace: r#"        return (net_weight as f64 * max_dist as f64) as i64;"#,
        want: r#"the_out_of_outline_charge_is_quantised_by_f32"#,
    },
    Mutation {
        name: r#"die-margin-narrowed-before-halving"#,
        file: r#"src/placement.rs"#,
        find: r#"    let max_dist = (die_margin / 2) as i32;"#,
        replace: r#"    let max_dist = die_margin as i32 / 2;"#,
        want: r#"a_die_margin_past_the_int_range_wraps"#,
    },
    Mutation {
        name: r#"die-margin-not-halved"#,
        file: r#"src/placement.rs"#,
        find: r#"    let max_dist = (die_margin / 2) as i32;"#,
        replace: r#"    let max_dist = die_margin as i32;"#,
        want: r#"a_macro_outside_the_outline_is_charged_the_whole_die"#,
    },
    Mutation {
        name: r#"fence-zero-area-test-widened"#,
        file: r#"src/placement.rs"#,
        find: r#"        if m.width.wrapping_mul(m.height) == 0 {"#,
        replace: r#"        if m.width as i64 * m.height as i64 == 0 {"#,
        want: r#"a_macro_whose_area_wraps_to_zero_is_skipped"#,
    },
    // ---------------------------------------------------------------- choosing a run
    Mutation {
        name: r#"utilization-ramp-all-one-precision"#,
        file: r#"src/placement.rs"#,
        find: r#"        .map(|i| (target_utilization as f64 * (exponential_ratio as f64).powf(i as f64)) as f32)"#,
        replace: r#"        .map(|i| target_utilization * exponential_ratio.powi(i))"#,
        want: r#"the_utilization_list_matches_the_reference_bit_for_bit"#,
    },
    Mutation {
        name: r#"utilization-ratio-divided-in-f64"#,
        file: r#"src/placement.rs"#,
        find: r#"    let base = (maximum_utilization / target_utilization) as f64;"#,
        replace: r#"    let base = maximum_utilization as f64 / target_utilization as f64;"#,
        want: r#"the_division_happens_in_f32_and_the_reference_says_so"#,
    },
    Mutation {
        name: r#"utilization-exponent-off-by-one"#,
        file: r#"src/placement.rs"#,
        find: r#"    let exponential_ratio = base.powf(1.0 / (total_number_of_runs as f64 - 1.0)) as f32;"#,
        replace: r#"    let exponential_ratio = base.powf(1.0 / total_number_of_runs as f64) as f32;"#,
        want: r#"the_utilization_list_matches_the_reference_bit_for_bit"#,
    },
    Mutation {
        name: r#"skipped-utilization-does-not-spend-its-slot"#,
        file: r#"src/placement.rs"#,
        find: r#"            let index = run_id;
            run_id += 1;"#,
        replace: r#"            let index = run_id;"#,
        want: r#"a_skipped_utilization_still_spends_its_slot"#,
    },
    Mutation {
        name: r#"batch-judged-as-it-anneals"#,
        file: r#"src/placement.rs"#,
        find: r#"        let results: Vec<bool> = batch.iter().map(|&i| run(i, utilizations[i])).collect();"#,
        replace: r#"        let mut results: Vec<bool> = Vec::new();
        for &i in &batch {
            let ok = run(i, utilizations[i]);
            results.push(ok);
            if ok {
                break;
            }
        }"#,
        want: r#"a_single_thread_does_less_work_for_the_same_answer"#,
    },
    Mutation {
        name: r#"first-run-reported-as-adjusted"#,
        file: r#"src/placement.rs"#,
        find: r#"                    utilization_was_adjusted: index != 0,"#,
        replace: r#"                    utilization_was_adjusted: true,"#,
        want: r#"the_asked_for_utilization_is_not_an_adjustment"#,
    },
    Mutation {
        name: r#"root-and-child-share-one-code"#,
        file: r#"src/placement.rs"#,
        find: r#"    if is_root {
        crate::options::MplError::new(
            40,"#,
        replace: r#"    if false {
        crate::options::MplError::new(
            40,"#,
        want: r#"the_root_and_a_child_fail_with_different_codes"#,
    },
    Mutation {
        name: r#"real-location-added-in-integers"#,
        file: r#"src/placement.rs"#,
        find: r#"        *x = (*x as f32 + offset.0 as f32) as i32;"#,
        replace: r#"        *x = *x + offset.0;"#,
        want: r#"a_large_coordinate_loses_precision_in_the_round_trip"#,
    },
    Mutation {
        name: r#"initial-sequence-pair-ignored"#,
        file: r#"src/anneal.rs"#,
        find: r#"    if let Some(sp) = initial {
        return sp;
    }"#,
        replace: r#"    if false {
        return initial.unwrap();
    }"#,
        want: r#"an_initial_sequence_pair_suppresses_the_default"#,
    },
    Mutation {
        name: r#"sequence-pair-covers-every-macro"#,
        file: r#"src/anneal.rs"#,
        find: r#"    let size = if number_of_sequence_pair_macros != 0 {
        number_of_sequence_pair_macros
    } else {
        macro_count
    };"#,
        replace: r#"    let size = macro_count;"#,
        want: r#"the_sequence_pair_covers_only_the_placeable_macros"#,
    },
    Mutation {
        name: r#"invalid-states-disallowed-by-default"#,
        file: r#"src/anneal.rs"#,
        find: r#"            invalid_states_allowed: true,"#,
        replace: r#"            invalid_states_allowed: false,"#,
        want: r#"invalid_states_are_allowed_by_default"#,
    },
    // ---------------------------------------------------------------- the nine-term cost
    Mutation {
        name: r#"cost-area-term-divided"#,
        file: r#"src/anneal.rs"#,
        find: r#"        cost += w.area * p.area;"#,
        replace: r#"        cost += w.area * p.area / n.area;"#,
        want: r#"the_area_term_is_not_divided_by_its_normalisation_factor"#,
    },
    Mutation {
        name: r#"cost-notch-divides-by-the-wrong-factor"#,
        file: r#"src/anneal.rs"#,
        find: r#"        cost += w.notch * p.notch / n.notch;"#,
        replace: r#"        cost += w.notch * p.notch / n.boundary;"#,
        want: r#"each_term_divides_by_its_own_normalisation_factor"#,
    },
    Mutation {
        name: r#"cost-zero-factor-still-divides"#,
        file: r#"src/anneal.rs"#,
        find: r#"    if n.outline > 0.0 {
        cost += w.outline * p.outline / n.outline;
    }"#,
        replace: r#"    {
        cost += w.outline * p.outline / n.outline;
    }"#,
        want: r#"a_zero_normalisation_factor_drops_its_term"#,
    },
    Mutation {
        name: r#"cal-penalty-refreshes-fixed-macros-first"#,
        file: r#"src/anneal.rs"#,
        find: r#"            let valid = self.is_valid(!self.fixed_bboxes.is_empty());"#,
        replace: r#"            self.penalties.fixed_macros = fixed_macros_penalty(
                &self.macros,
                &self.fixed_bboxes,
                &self.sp,
                self.dbu_per_micron,
            );
            let valid = self.is_valid(!self.fixed_bboxes.is_empty());"#,
        want: r#"the_notch_term_sees_a_stale_fixed_macro_penalty"#,
    },
    Mutation {
        name: r#"cal-penalty-drops-the-placement-context"#,
        file: r#"src/anneal.rs"#,
        // ⛔ **DISAMBIGUATED.** `self.placement = Some(inputs);` appears TWICE — once in the
        // hard-macro branch and once in the soft one — and the harness mutates the FIRST. The test
        // named here drives a SOFT run, so the mutation was landing in the branch the test never
        // enters and reported as a hole. ⚠️ `check-patterns.py` flags this class as `ambiguous`;
        // that warning is what it is for.
        find: r#"                inputs.notch(&self.macros, outline, (self.width, self.height), valid);

            self.placement = Some(inputs);"#,
        replace: r#"                inputs.notch(&self.macros, outline, (self.width, self.height), valid);

            self.placement = None;
            drop(inputs);"#,
        want: r#"the_placement_context_survives_being_scored"#,
    },
    // 🔑 The HARD branch's own restore. ⚠️ It is a DIFFERENT line from the soft one above and needs
    // its own witness: the soft test never enters this branch.
    Mutation {
        name: r#"hard-cal-penalty-drops-the-placement-context"#,
        file: r#"src/anneal.rs"#,
        find: r#"                self.penalties.fence = inputs.fence(&self.macros, outline);
                self.placement = Some(inputs);"#,
        replace: r#"                self.penalties.fence = inputs.fence(&self.macros, outline);
                self.placement = None;
                drop(inputs);"#,
        want: r#"the_placement_context_survives_a_hard_run_too"#,
    },
    // ---------------------------------------------------------------- the cluster's soft macro
    Mutation {
        name: r#"missing-soft-macro-is-not-the-origin"#,
        file: r#"src/cluster.rs"#,
        find: r#"        self.soft_macro.map_or((0, 0), |m| (m.x, m.y))"#,
        replace: r#"        self.soft_macro.map_or((1, 1), |m| (m.x, m.y))"#,
        want: r#"a_cluster_without_a_soft_macro_reads_as_the_origin"#,
    },
    Mutation {
        name: r#"set-location-works-without-a-soft-macro"#,
        file: r#"src/cluster.rs"#,
        find: r#"        if let Some(m) = self.soft_macro.as_mut() {
            m.x = location.0;
            m.y = location.1;
        }"#,
        replace: r#"        let m = self.soft_macro.get_or_insert(crate::anneal::SoftMacro::default());
        m.x = location.0;
        m.y = location.1;"#,
        want: r#"moving_a_cluster_without_a_soft_macro_does_nothing"#,
    },
    Mutation {
        name: r#"io-soft-macro-has-an-area"#,
        file: r#"src/cluster.rs"#,
        find: r#"        fixed: true,
        area: 0,
        is_macro_cluster: false,
    }
}"#,
        replace: r#"        fixed: true,
        area: width as i64 * height as i64,
        is_macro_cluster: false,
    }
}"#,
        want: r#"an_io_clusters_area_is_zero_however_large_its_region"#,
    },
    Mutation {
        name: r#"fixed-macro-area-from-its-metrics"#,
        file: r#"src/cluster.rs"#,
        find: r#"        if self.is_fixed_macro {
            return self.soft_macro.map_or(0, |m| m.area);
        }"#,
        replace: r#"        if false {
            return self.soft_macro.map_or(0, |m| m.area);
        }"#,
        want: r#"a_fixed_macro_reports_its_soft_macro_area"#,
    },
    Mutation {
        name: r#"cluster-centre-uses-floating-point"#,
        file: r#"src/cluster.rs"#,
        find: r#"        (x + w / 2, y + h / 2)"#,
        replace: r#"        ((x as f64 + 0.5 * w as f64).round() as i32, (y as f64 + 0.5 * h as f64).round() as i32)"#,
        want: r#"the_centre_halves_in_integers"#,
    },
    Mutation {
        name: r#"fixed-macro-soft-macro-rebased"#,
        file: r#"src/cluster.rs"#,
        find: r#"            x: bbox.0,
            y: bbox.1,
            width: bbox.2 - bbox.0,"#,
        replace: r#"            x: 0,
            y: 0,
            width: bbox.2 - bbox.0,"#,
        want: r#"a_fixed_macro_reports_its_soft_macro_area"#,
    },
    Mutation {
        name: r#"halo-order-lrbt"#,
        file: r#"src/options.rs"#,
        find: r#"4 => (values[0], values[1], values[2], values[3]),"#,
        replace: r#"4 => (values[0], values[2], values[1], values[3]),"#,
        want: r#"a_four_value_halo_is_left_bottom_right_top"#,
    },
    Mutation {
        name: r#"halo-two-value-not-mirrored"#,
        file: r#"src/options.rs"#,
        find: r#"2 => (values[0], values[1], values[0], values[1]),"#,
        replace: r#"2 => (values[0], values[1], 0, 0),"#,
        want: r#"a_two_value_halo_mirrors_into_four"#,
    },
    Mutation {
        name: r#"negative-halo-allowed"#,
        file: r#"src/options.rs"#,
        find: r#"if v < 0 {"#,
        replace: r#"if false {"#,
        want: r#"a_negative_halo_value_is_mpl73"#,
    },
    Mutation {
        name: r#"both-blockage-weights-allowed"#,
        file: r#"src/options.rs"#,
        find: r#"if saw_macro_blockage_weight && saw_soft_blockage_weight {"#,
        replace: r#"if false {"#,
        want: r#"giving_both_blockage_weights_is_mpl69"#,
    },
    Mutation {
        name: r#"macro-blockage-does-not-alias"#,
        file: r#"src/options.rs"#,
        find: r#"                saw_macro_blockage_weight = true;
                warnings.push(MplWarning {
                    code: 70,"#,
        replace: r#"                saw_macro_blockage_weight = true;
                warnings.push(MplWarning {
                    code: 700,"#,
        want: r#"macro_blockage_weight_aliases_soft_and_warns_mpl70"#,
    },
    Mutation {
        name: r#"macro-blockage-weight-not-applied"#,
        file: r#"src/options.rs"#,
        find: r#"                });
                o.soft_blockage_weight = num(value)?;"#,
        replace: r#"                });
                let _ = num(value)?;"#,
        want: r#"macro_blockage_weight_aliases_soft_and_warns_mpl70"#,
    },
    Mutation {
        name: r#"halo-width-alone-loses-height"#,
        file: r#"src/options.rs"#,
        find: r#"let h = halo_height.or(halo_width).unwrap_or(0);"#,
        replace: r#"let h = halo_height.unwrap_or(0);"#,
        want: r#"halo_width_alone_sets_height_to_it_and_warns_mpl74"#,
    },
    Mutation {
        name: r#"halo-height-alone-loses-width"#,
        file: r#"src/options.rs"#,
        find: r#"let w = halo_width.or(halo_height).unwrap_or(0);"#,
        replace: r#"let w = halo_width.unwrap_or(0);"#,
        want: r#"halo_height_alone_sets_width_to_it"#,
    },
    Mutation {
        name: r#"mpl74-warning-dropped"#,
        file: r#"src/options.rs"#,
        find: r#"code: 74,"#,
        replace: r#"code: 0,"#,
        want: r#"halo_width_alone_sets_height_to_it_and_warns_mpl74"#,
    },
    Mutation {
        name: r#"target-util-default-wrong"#,
        file: r#"src/options.rs"#,
        find: r#"target_util: 0.25,"#,
        replace: r#"target_util: 0.30,"#,
        want: r#"every_default_matches_upstreams_tcl"#,
    },
    Mutation {
        name: r#"report-dir-default-wrong"#,
        file: r#"src/options.rs"#,
        find: r#"report_directory: "hier_rtlmp".to_string(),"#,
        replace: r#"report_directory: "mpl".to_string(),"#,
        want: r#"every_default_matches_upstreams_tcl"#,
    },
    Mutation {
        name: r#"region-inversion-unchecked"#,
        file: r#"src/options.rs"#,
        find: r#"if x1 > x2 {"#,
        replace: r#"if false {"#,
        want: r#"a_region_is_four_values_and_must_not_be_inverted"#,
    },
    Mutation {
        name: r#"vacuous-reads-as-applied"#,
        file: r#"src/status.rs"#,
        find: r#"if placed == 0 {"#,
        replace: r#"if false {"#,
        want: r#"placing_nothing_is_never_applied"#,
    },
    Mutation {
        name: r#"refusal-does-not-outrank"#,
        file: r#"src/status.rs"#,
        find: r#"if refusal.is_some() {"#,
        replace: r#"if false {"#,
        want: r#"a_refusal_outranks_the_count"#,
    },
    Mutation {
        name: r#"stop-after-uses-first-occurrence"#,
        file: r#"src/pipeline.rs"#,
        find: r#"seq.iter().rposition("#,
        replace: r#"seq.iter().position("#,
        want: r#"repeat_duplicates_in_place_and_composes_with_stop_after"#,
    },
    Mutation {
        name: r#"only-reorders-as-asked"#,
        file: r#"src/pipeline.rs"#,
        find: r#"ORDER.iter().copied().filter(|s| only.contains(s)).collect()"#,
        replace: r#"only.clone()"#,
        want: r#"only_keeps_upstreams_relative_order_not_the_order_asked_for"#,
    },
    Mutation {
        name: r#"boundary-order-l-after-r"#,
        file: r#"src/halo.rs"#,
        find: r#"    B = 0,
    L = 1,
    T = 2,
    R = 3,"#,
        replace: r#"    B = 0,
    L = 3,
    T = 2,
    R = 1,"#,
        want: r#"equidistant_same_direction_falls_back_to_the_enum_order"#,
    },
    Mutation {
        name: r#"boundary-order-b-after-t"#,
        file: r#"src/halo.rs"#,
        find: r#"    B = 0,
    L = 1,
    T = 2,
    R = 3,"#,
        replace: r#"    B = 2,
    L = 1,
    T = 0,
    R = 3,"#,
        want: r#"a_centred_pin_prefers_bottom_over_top"#,
    },
    Mutation {
        name: r#"is-vertical-inverted"#,
        file: r#"src/halo.rs"#,
        find: r#"        matches!(self, Boundary::L | Boundary::R)"#,
        replace: r#"        matches!(self, Boundary::B | Boundary::T)"#,
        want: r#"a_corner_pin_is_decided_by_its_layer_direction"#,
    },
    Mutation {
        name: r#"corner-rule-layer-dir-swapped"#,
        file: r#"src/halo.rs"#,
        find: r#"        LayerDir::Vertical => {
            if first.1.is_vertical() { second.1 } else { first.1 }
        }"#,
        replace: r#"        LayerDir::Vertical => {
            if first.1.is_vertical() { first.1 } else { second.1 }
        }"#,
        want: r#"a_corner_pin_is_decided_by_its_layer_direction"#,
    },
    Mutation {
        name: r#"right-distance-uses-x-min"#,
        file: r#"src/halo.rs"#,
        find: r#"        (master_width - pin.x_max, Boundary::R),"#,
        replace: r#"        (pin.x_min, Boundary::R),"#,
        want: r#"right_and_bottom_distances_use_the_master_extents"#,
    },
    Mutation {
        name: r#"soft-halo-floored-to-base"#,
        file: r#"src/halo.rs"#,
        find: r#"        Some((h, true)) => h,"#,
        replace: r#"        Some((h, true)) => Halo { left: h.left.max(base.left), bottom: h.bottom.max(base.bottom), right: h.right.max(base.right), top: h.top.max(base.top) },"#,
        want: r#"a_soft_instance_halo_is_taken_as_is_and_is_not_floored"#,
    },
    Mutation {
        name: r#"unfixed-macro-reoriented"#,
        file: r#"src/halo.rs"#,
        find: r#"    if is_fixed {
        return reorient_for_fixed(halo, orient);
    }"#,
        replace: r#"    return reorient_for_fixed(halo, orient);
    #[allow(unreachable_code)]"#,
        want: r#"an_unfixed_macro_is_not_reoriented"#,
    },
    Mutation {
        name: r#"mx-swaps-left-right"#,
        file: r#"src/halo.rs"#,
        find: r#"    if matches!(orient, Orient::Mx | Orient::R180) {
        std::mem::swap(&mut h.bottom, &mut h.top);"#,
        replace: r#"    if matches!(orient, Orient::Mx | Orient::R180) {
        std::mem::swap(&mut h.left, &mut h.right);"#,
        want: r#"a_fixed_macro_has_its_halo_reoriented"#,
    },
    Mutation {
        name: r#"minimum-spacing-ignored"#,
        file: r#"src/halo.rs"#,
        find: r#"    let mut halo = Halo {
        left: minimum_spacing,"#,
        replace: r#"    let mut halo = Halo {
        left: 0,"#,
        want: r#"no_pins_at_all_leaves_the_minimum_spacing_on_every_side"#,
    },
    Mutation {
        name: r#"explicit-halo-not-short-circuited"#,
        file: r#"src/halo.rs"#,
        find: r#"    if let Some(h) = explicit {
        return h;
    }

    let full = full_halo(None, inst_halo, base);"#,
        replace: r#"    let full = full_halo(explicit, inst_halo, base);"#,
        want: r#"an_explicit_halo_bypasses_use_full_halo_and_reorientation"#,
    },
    Mutation {
        name: r#"level-reset-outside-derivation"#,
        file: r#"src/thresholds.rs"#,
        find: r#"        if metrics.num_macro <= MIN_NUM_MACROS_FOR_MULTILEVEL {
            max_level = 1;
        }"#,
        replace: r#"    }
    if metrics.num_macro <= MIN_NUM_MACROS_FOR_MULTILEVEL {
        max_level = 1;
    }
    if false {"#,
        want: r#"keep_clustering_data2_matches_upstreams_reported_thresholds"#,
    },
    Mutation {
        name: r#"virtual-connections-stored-on-the-leaf"#,
        file: r#"src/tree.rs"#,
        find: r#"        new_virtual.extend(plan.virtual_connections.iter().copied());"#,
        replace: r#"        child.virtual_connections.extend(plan.virtual_connections.iter().copied());"#,
        want: r#"virtual_connections_are_stored_on_the_broken_leafs_parent_not_on_the_leaf_or_the_root"#,
    },
    Mutation {
        name: r#"virtual-connections-dropped"#,
        file: r#"src/tree.rs"#,
        find: r#"    parent.virtual_connections.extend(new_virtual);"#,
        replace: r#"    let _ = new_virtual;"#,
        want: r#"a_design_with_no_db_nets_still_has_bundled_nets_from_the_virtual_connections"#,
    },
    Mutation {
        name: r#"virtual-connection-pairs-reversed"#,
        file: r#"src/macroclass.rs"#,
        find: r#"            virtual_connections.push((virtual_members[i], virtual_members[j]));"#,
        replace: r#"            virtual_connections.push((virtual_members[j], virtual_members[i]));"#,
        want: r#"virtual_connections_are_stored_on_the_broken_leafs_parent_not_on_the_leaf_or_the_root"#,
    },
    Mutation {
        name: r#"dead-space-not-filled-by-the-driver"#,
        file: r#"src/placement.rs"#,
        find: r#"            fill_dead_space_on_solution(
                &mut search.macros,
                &kinds,
                (search.outline_width, search.outline_height),
                valid,
            );"#,
        replace: r#"            let _ = (&kinds, valid);"#,
        want: r#"the_driver_fills_dead_space_before_returning_the_placement"#,
    },
    Mutation {
        name: r#"dead-space-fills-invalid-solutions"#,
        file: r#"src/placement.rs"#,
        find: r#"    if !is_valid {
        return;
    }
    let mut cells: Vec<DeadSpaceMacro> = macros"#,
        replace: r#"    let mut cells: Vec<DeadSpaceMacro> = macros"#,
        want: r#"an_invalid_solution_is_not_filled"#,
    },
    Mutation {
        name: r#"dead-space-keeps-the-stale-area"#,
        file: r#"src/placement.rs"#,
        find: r#"            macros[id].area = macros[id].width as i64 * macros[id].height as i64;"#,
        replace: r#"            macros[id].area = macros[id].area;"#,
        want: r#"growing_a_cluster_recomputes_its_area"#,
    },
    Mutation {
        name: r#"io-pad-cluster-has-no-soft-macro"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"        c.set_as_io_pad_cluster(
            (b.x_min as i32, b.y_min as i32),
            (b.x_max - b.x_min) as i32,
            (b.y_max - b.y_min) as i32,
        );"#,
        replace: r#"        c.is_io_pad_cluster = true;
        let _ = &b;"#,
        want: r#"an_io_pad_cluster_takes_the_pads_own_bbox"#,
    },
    Mutation {
        name: r#"io-pin-cluster-has-no-soft-macro"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"        c.set_as_cluster_of_unplaced_io_pins(
            (shape.x_min as i32, shape.y_min as i32),
            (shape.x_max - shape.x_min) as i32,
            (shape.y_max - shape.y_min) as i32,
            pin.constraint.is_none(),
        );"#,
        replace: r#"        c.is_cluster_of_unplaced_io_pins = true;
        c.is_cluster_of_unconstrained_io_pins = pin.constraint.is_none();
        let _ = &shape;"#,
        want: r#"an_io_pin_cluster_takes_its_constraint_region_or_the_whole_die"#,
    },
    Mutation {
        name: r#"unconstrained-io-cluster-takes-an-edge"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"            None => *die,
        };"#,
        replace: r#"            None => crate::design::Rect { x_max: die.x_min, ..*die },
        };"#,
        want: r#"an_io_pin_cluster_takes_its_constraint_region_or_the_whole_die"#,
    },
    Mutation {
        name: r#"hard-core-computes-the-fixed-macro-penalty"#,
        file: r#"src/anneal.rs"#,
        find: r#"                self.placement = Some(inputs);
            }
            return;
        }"#,
        replace: r#"                self.placement = Some(inputs);
            }
        }"#,
        want: r#"a_hard_macro_run_does_not_compute_the_soft_penalties"#,
    },
    Mutation {
        name: r#"macro-summary-prints-the-soft-rows"#,
        file: r#"src/placement.rs"#,
        find: r#"    row("Fence", weights.fence, penalties.fence, norms.fence);
    out.push_str("---------------------------------------------------------------\n");
    // ⚠️ `reportTotalCost` is the BASE class's"#,
        replace: r#"    row("Fence", weights.fence, penalties.fence, norms.fence);
    row("Notch", weights.notch, penalties.notch, norms.notch);
    out.push_str("---------------------------------------------------------------\n");
    // ⚠️ `reportTotalCost` is the BASE class's"#,
        want: r#"the_macro_summary_is_the_references_five_row_table"#,
    },
    Mutation {
        name: r#"fixed-macros-from-children-not-the-sequence"#,
        file: r#"src/placement.rs"#,
        find: r#"        fixed_bboxes: assembly
            .macros
            .iter()
            .take(assembly.number_of_sequence_pair_macros)
            .filter(|m| m.fixed)
            .map(|m| m.bbox())
            .collect(),"#,
        replace: r#"        fixed_bboxes: children
            .iter()
            .filter(|c| c.kind == AreaKind::FixedMacro)
            .map(|c| c.macro_.bbox())
            .collect(),"#,
        want: r#"a_blockage_proxy_counts_as_a_fixed_macro"#,
    },
    Mutation {
        name: r#"fixed-macros-ignore-the-sequence-bound"#,
        file: r#"src/placement.rs"#,
        find: r#"            .take(assembly.number_of_sequence_pair_macros)
            .filter(|m| m.fixed)"#,
        replace: r#"            .filter(|m| m.fixed)"#,
        want: r#"a_blockage_proxy_counts_as_a_fixed_macro"#,
    },
    Mutation {
        name: r#"blockages-not-clipped-to-the-outline"#,
        file: r#"src/placement.rs"#,
        find: r#"        if (x1 - x0) as i64 * (y1 - y0) as i64 == 0 {
            continue;
        }"#,
        replace: r#"        if false {
            continue;
        }"#,
        want: r#"blockages_are_clipped_to_the_outline_and_rebased"#,
    },
    Mutation {
        name: r#"set-x-ignores-the-fixed-guard"#,
        file: r#"src/anneal.rs"#,
        find: r#"    pub fn set_x(&mut self, x: i32) {
        if !self.fixed {
            self.x = x;
        }
    }"#,
        replace: r#"    pub fn set_x(&mut self, x: i32) {
        self.x = x;
    }"#,
        want: r#"a_fixed_macro_is_selected_by_the_alignment_but_never_moved"#,
    },
    Mutation {
        name: r#"set-y-ignores-the-fixed-guard"#,
        file: r#"src/anneal.rs"#,
        find: r#"    pub fn set_y(&mut self, y: i32) {
        if !self.fixed {
            self.y = y;
        }
    }"#,
        replace: r#"    pub fn set_y(&mut self, y: i32) {
        self.y = y;
    }"#,
        want: r#"a_fixed_macro_is_selected_by_the_alignment_but_never_moved"#,
    },
    Mutation {
        name: r#"notch-grid-skips-fixed-macros"#,
        file: r#"src/placement.rs"#,
        find: r#"            Some(AreaKind::HardMacroCluster)
                | Some(AreaKind::MixedCluster)
                | Some(AreaKind::FixedMacro)"#,
        replace: r#"            Some(AreaKind::HardMacroCluster) | Some(AreaKind::MixedCluster)"#,
        want: r#"a_fixed_macro_obstructs_the_notch_grid"#,
    },
    Mutation {
        name: r#"fixed-macro-does-not-obstruct"#,
        file: r#"src/placement.rs"#,
        find: r#"                // it does so through `fixed`, not through this flag.
                is_macro_cluster: true,"#,
        replace: r#"                // it does so through `fixed`, not through this flag.
                is_macro_cluster: false,"#,
        want: r#"a_fixed_macro_is_a_macro_cluster_to_the_annealer"#,
    },
    Mutation {
        name: r#"macro-arrays-not-marked"#,
        file: r#"src/tree.rs"#,
        find: r#"            c.is_macro_array = plan.arrays.iter().any(|a| a.id == c.id);"#,
        replace: r#"            c.is_macro_array = false;"#,
        want: r#"a_movable_macro_cluster_is_marked_as_an_array"#,
    },
    Mutation {
        name: r#"fixed-macro-clusters-marked-as-arrays"#,
        file: r#"src/tree.rs"#,
        find: r#"            c.is_macro_array = plan.arrays.iter().any(|a| a.id == c.id);"#,
        replace: r#"            c.is_macro_array = true;"#,
        want: r#"a_movable_macro_cluster_is_marked_as_an_array"#,
    },
    Mutation {
        name: r#"fixed-macro-soft-macro-uses-the-unhaloed-bbox"#,
        file: r#"src/tree.rs"#,
        find: r#"                let b = ctx.macro_bboxes.get(i).copied().unwrap_or(inst.bbox);"#,
        replace: r#"                let b = inst.bbox;"#,
        want: r#"a_fixed_macro_cluster_carries_its_haloed_soft_macro"#,
    },
    Mutation {
        name: r#"every-macro-cluster-gets-a-fixed-soft-macro"#,
        file: r#"src/tree.rs"#,
        find: r#"            if inst.is_fixed {
                let b = ctx.macro_bboxes.get(i).copied().unwrap_or(inst.bbox);"#,
        replace: r#"            if true {
                let b = ctx.macro_bboxes.get(i).copied().unwrap_or(inst.bbox);"#,
        want: r#"a_fixed_macro_cluster_carries_its_haloed_soft_macro"#,
    },
    Mutation {
        name: r#"constraint-region-keyed-by-macro-index"#,
        file: r#"src/placement.rs"#,
        find: r#"                .filter_map(|c| {
                    let region = (ctx.constraint_region_of)(c.id)?;
                    Some((assembly.id(&c.name)?, region))
                })"#,
        replace: r#"                .enumerate()
                .filter_map(|(i, c)| {
                    let region = (ctx.constraint_region_of)(c.id)?;
                    let _ = &c.name;
                    Some((i, region))
                })"#,
        want: r#"a_constrained_io_clusters_region_reaches_the_problem_by_cluster_id"#,
    },
    Mutation {
        name: r#"reset-runs-after-the-soft-blockage-adjustment"#,
        file: r#"src/placement.rs"#,
        find: r#"    adjusted.soft_blockage =
        adjusted_soft_blockage_weight(max_level, adjusted.outline, adjusted.soft_blockage);
    (adjusted, tiny_cluster_max_number_of_std_cells(block_instance_count), probabilities)"#,
        replace: r#"    adjusted.soft_blockage =
        adjusted_soft_blockage_weight(max_level, adjusted.outline, adjusted.soft_blockage);
    if !has_std_cells {
        adjusted.soft_blockage = 0.0;
    }
    (adjusted, tiny_cluster_max_number_of_std_cells(block_instance_count), probabilities)"#,
        want: r#"a_design_with_no_standard_cells_is_reset_before_the_soft_blockage_adjustment"#,
    },
    Mutation {
        name: r#"no-reset-without-standard-cells"#,
        file: r#"src/placement.rs"#,
        find: r#"    let (mut adjusted, probabilities) = if has_std_cells {"#,
        replace: r#"    let (mut adjusted, probabilities) = if true {"#,
        want: r#"a_design_with_no_standard_cells_is_reset_before_the_soft_blockage_adjustment"#,
    },
    Mutation {
        name: r#"macro-cluster-gets-no-shape-curve"#,
        file: r#"src/placement.rs"#,
        find: r#"        if r.kind == Some(AreaKind::HardMacroCluster) && !r.tilings.is_empty() {"#,
        replace: r#"        if false && r.kind == Some(AreaKind::HardMacroCluster) && !r.tilings.is_empty() {"#,
        want: r#"a_macro_cluster_is_given_its_tilings_as_its_shape_curve"#,
    },
    Mutation {
        name: r#"placement-resize-share-equals-a-swap"#,
        file: r#"src/anneal.rs"#,
        find: r#"        Self::normalized(0.2, 0.2, 0.2, 0.2, 0.4)"#,
        replace: r#"        Self::normalized(0.2, 0.2, 0.2, 0.2, 0.2)"#,
        want: r#"the_placement_resize_share_is_double_a_swap_share"#,
    },
    Mutation {
        name: r#"norm-sweep-skips-the-placement-terms"#,
        file: r#"src/anneal.rs"#,
        find: r#"            wirelength: mean(&|s| s.penalties.wirelength),"#,
        replace: r#"            wirelength: 1.0,"#,
        want: r#"a_placement_sweep_fills_the_placement_normalisation_factors"#,
    },
    Mutation {
        name: r#"norm-sweep-floor-is-not-applied"#,
        file: r#"src/anneal.rs"#,
        find: r#"        let floor_at = |value: f32| if value <= 1e-4 { 1.0 } else { value };
        let mean = |f: &dyn Fn(&Sample) -> f32| -> f32 {"#,
        replace: r#"        let floor_at = |value: f32| value;
        let mean = |f: &dyn Fn(&Sample) -> f32| -> f32 {"#,
        want: r#"a_shaping_sweep_leaves_the_six_placement_factors_at_exactly_one"#,
    },
    Mutation {
        name: r#"perturb-count-cluster-uses-shaping-floor"#,
        file: r#"src/placement.rs"#,
        find: r#"pub fn cluster_perturbations_per_step(num_perturb_per_step: i32, macro_count: i32) -> i32 {
    macro_count.max(num_perturb_per_step)"#,
        replace: r#"pub fn cluster_perturbations_per_step(num_perturb_per_step: i32, macro_count: i32) -> i32 {
    macro_count.max(num_perturb_per_step / 10)"#,
        want: r#"the_three_perturbation_rules_disagree_on_the_same_cluster"#,
    },
    Mutation {
        name: r#"perturb-count-initialize-rederives"#,
        file: r#"src/anneal.rs"#,
        find: r#"        let perturbations = params.num_perturb_per_step.max(0) as usize;

        let mut samples: Vec<Sample> = Vec::with_capacity(perturbations);"#,
        replace: r#"        let perturbations =
            (params.num_perturb_per_step.max(0) as usize).max(self.macros.len());

        let mut samples: Vec<Sample> = Vec::with_capacity(perturbations);"#,
        want: r#"initialize_runs_exactly_the_perturbation_count_it_is_given"#,
    },
    Mutation {
        name: r#"perturb-count-not-applied-at-call-site"#,
        file: r#"src/placement.rs"#,
        find: r#"        params.num_perturb_per_step = cluster_perturbations_per_step(
            params.num_perturb_per_step,
            problem.macros.len() as i32,
        );"#,
        replace: r#"        params.num_perturb_per_step = params.num_perturb_per_step;"#,
        want: r#"cluster_placement_perturbs_on_the_full_configured_count"#,
    },
    Mutation {
        name: r#"threshold-guard-is-per-field"#,
        file: r#"src/thresholds.rs"#,
        find: r#"    if t.max_macro <= 0 || t.min_macro <= 0 || t.max_std_cell <= 0 || t.min_std_cell <= 0 {"#,
        replace: r#"    if t.max_macro <= 0 && t.min_macro <= 0 && t.max_std_cell <= 0 && t.min_std_cell <= 0 {"#,
        want: r#"a_partially_supplied_threshold_set_derives_all_four"#,
    },
    Mutation {
        name: r#"std-cell-floor-not-applied"#,
        file: r#"src/thresholds.rs"#,
        find: r#"        t.min_std_cell = t.min_std_cell.max(MIN_NUM_STD_CELLS_ALLOWED);"#,
        replace: r#"        t.min_std_cell = t.min_std_cell.max(1);"#,
        want: r#"halos1_matches_upstreams_reported_thresholds"#,
    },
    Mutation {
        name: r#"macro-floor-uses-std-cell-constant"#,
        file: r#"src/thresholds.rs"#,
        find: r#"        if t.min_macro <= 0 {
            t.min_macro = 1;
        }"#,
        replace: r#"        if t.min_macro <= 0 {
            t.min_macro = MIN_NUM_STD_CELLS_ALLOWED;
        }"#,
        want: r#"the_std_cell_minimum_is_floored_at_1000_and_the_macro_minimum_at_1"#,
    },
    Mutation {
        name: r#"coarsening-uses-max-level-not-minus-one"#,
        file: r#"src/thresholds.rs"#,
        find: r#"        let f = (cluster_size_ratio as f64).powi(max_level - 1);"#,
        replace: r#"        let f = (cluster_size_ratio as f64).powi(max_level);"#,
        want: r#"keep_clustering_data2_matches_upstreams_reported_thresholds"#,
    },
    Mutation {
        name: r#"fixed-macros-do-not-force-one-level"#,
        file: r#"src/thresholds.rs"#,
        find: r#"    if has_fixed_macros {
        max_level = 1;
    }"#,
        replace: r#"    if false {
        max_level = 1;
    }"#,
        want: r#"a_fixed_macro_forces_a_single_level"#,
    },
    Mutation {
        name: r#"per-level-std-floor-is-1000"#,
        file: r#"src/thresholds.rs"#,
        find: r#"        t.min_std_cell = 100;"#,
        replace: r#"        t.min_std_cell = 1000;"#,
        want: r#"a_degenerate_std_cell_minimum_becomes_one_hundred_not_one_thousand"#,
    },
    Mutation {
        name: r#"per-level-max-not-recomputed"#,
        file: r#"src/thresholds.rs"#,
        find: r#"    if t.min_macro <= 0 {
        t.min_macro = 1;
        t.max_macro = half_ratio(t.min_macro, cluster_size_ratio);
    }"#,
        replace: r#"    if t.min_macro <= 0 {
        t.min_macro = 1;
    }"#,
        want: r#"a_degenerate_macro_minimum_becomes_one_and_recomputes_its_maximum"#,
    },
    Mutation {
        name: r#"half-ratio-rounds-instead-of-truncating"#,
        file: r#"src/thresholds.rs"#,
        find: r#"    trunc((base as f32 * ratio) as f64 / 2.0)"#,
        replace: r#"    (((base as f32 * ratio) as f64 / 2.0).round()) as i32"#,
        want: r#"an_odd_ratio_truncates_the_half_rather_than_rounding_it"#,
    },
    Mutation {
        name: r#"level-divides-repeatedly"#,
        file: r#"src/thresholds.rs"#,
        find: r#"    let coarse_factor = (cluster_size_ratio as f64).powi(level - 1);"#,
        replace: r#"    let coarse_factor = (cluster_size_ratio as f64) * (level - 1).max(1) as f64;"#,
        want: r#"each_level_divides_by_the_ratio"#,
    },
    Mutation {
        name: r#"break-uses-and-not-or"#,
        file: r#"src/cluster.rs"#,
        find: r#"    cluster.num_std_cell() > max_std_cell || cluster.num_macro() > max_macro"#,
        replace: r#"    cluster.num_std_cell() > max_std_cell && cluster.num_macro() > max_macro"#,
        want: r#"breaking_needs_either_count_over_its_maximum"#,
    },
    Mutation {
        name: r#"merge-uses-or-not-and"#,
        file: r#"src/cluster.rs"#,
        find: r#"        && cluster.num_std_cell() < min_std_cell
        && cluster.num_macro() < min_macro"#,
        replace: r#"        && (cluster.num_std_cell() < min_std_cell
        || cluster.num_macro() < min_macro)"#,
        want: r#"merging_needs_both_counts_under_their_minimum"#,
    },
    Mutation {
        name: r#"io-clusters-can-merge"#,
        file: r#"src/cluster.rs"#,
        find: r#"    !cluster.is_io_cluster()
        && cluster.num_std_cell()"#,
        replace: r#"    true
        && cluster.num_std_cell()"#,
        want: r#"an_io_cluster_is_never_a_merge_candidate"#,
    },
    Mutation {
        name: r#"break-is-greater-or-equal"#,
        file: r#"src/cluster.rs"#,
        find: r#"    cluster.num_std_cell() > max_std_cell || cluster.num_macro() > max_macro"#,
        replace: r#"    cluster.num_std_cell() >= max_std_cell || cluster.num_macro() >= max_macro"#,
        want: r#"breaking_needs_either_count_over_its_maximum"#,
    },
    Mutation {
        name: r#"hard-macro-mask-missing"#,
        file: r#"src/cluster.rs"#,
        find: r#"        if self.cluster_type == ClusterType::HardMacro {
            return 0;
        }"#,
        replace: r#"        if false {
            return 0;
        }"#,
        want: r#"a_hard_macro_cluster_reports_no_standard_cells"#,
    },
    Mutation {
        name: r#"std-cell-mask-missing"#,
        file: r#"src/cluster.rs"#,
        find: r#"        if self.cluster_type == ClusterType::StdCell {
            return 0;
        }"#,
        replace: r#"        if false {
            return 0;
        }"#,
        want: r#"a_std_cell_cluster_reports_no_macros"#,
    },
    Mutation {
        name: r#"logical-module-allows-glue"#,
        file: r#"src/cluster.rs"#,
        find: r#"        self.leaf_std_cells.is_empty() && self.leaf_macros.is_empty() && self.db_modules.len() == 1"#,
        replace: r#"        self.db_modules.len() == 1"#,
        want: r#"glue_instances_stop_a_cluster_corresponding_to_a_logical_module"#,
    },
    Mutation {
        name: r#"logical-module-at-least-one"#,
        file: r#"src/cluster.rs"#,
        find: r#"&& self.db_modules.len() == 1
    }"#,
        replace: r#"&& !self.db_modules.is_empty()
    }"#,
        want: r#"two_modules_stop_it_too"#,
    },
    Mutation {
        name: r#"subtree-collapse-is-lifo"#,
        file: r#"src/cluster.rs"#,
        find: r#"        if cluster.children.is_empty() {
            leaves.push(cluster);"#,
        replace: r#"        if cluster.children.is_empty() {
            leaves.insert(0, cluster);"#,
        want: r#"the_collapse_is_breadth_first_not_depth_first"#,
    },
    Mutation {
        name: r#"subtree-pushes-to-front"#,
        file: r#"src/cluster.rs"#,
        find: r#"                wavefront.push_back(child);"#,
        replace: r#"                wavefront.push_front(child);"#,
        want: r#"the_collapse_is_breadth_first_not_depth_first"#,
    },
    Mutation {
        name: r#"dissolved-ids-not-reported"#,
        file: r#"src/cluster.rs"#,
        find: r#"            dissolved.push(cluster.id);"#,
        replace: r#"            let _ = cluster.id;"#,
        want: r#"every_dissolved_id_is_reported_so_the_id_map_can_be_pruned"#,
    },
    Mutation {
        name: r#"par-gate-uses-masked-counts"#,
        file: r#"src/cluster.rs"#,
        find: r#"        && (cluster.leaf_std_cells.len() as i64 > max_std_cell as i64
            || cluster.leaf_macros.len() as i64 > max_macro as i64)"#,
        replace: r#"        && (cluster.num_std_cell() as i64 > max_std_cell as i64
            || cluster.num_macro() as i64 > max_macro as i64)"#,
        want: r#"the_par_gate_counts_leaf_vectors_not_the_masked_metrics"#,
    },
    Mutation {
        name: r#"par-gate-ignores-modules"#,
        file: r#"src/cluster.rs"#,
        find: r#"    cluster.db_modules.is_empty()
        && (cluster.leaf_std_cells.len()"#,
        replace: r#"    true
        && (cluster.leaf_std_cells.len()"#,
        want: r#"a_large_flat_cluster_is_one_with_no_modules_and_too_many_leaves"#,
    },
    Mutation {
        name: r#"par-refusal-not-collected"#,
        file: r#"src/cluster.rs"#,
        find: r#"        .filter(|c| is_large_flat_cluster(c, max_std_cell, max_macro))"#,
        replace: r#"        .filter(|_c| false)"#,
        want: r#"a_resulting_child_needing_the_partitioner_is_reported_not_approximated"#,
    },
    Mutation {
        name: r#"blockage-cleanup-runs-forwards"#,
        file: r#"src/apply.rs"#,
        find: r#"    (baseline.blockages..now).rev().collect()"#,
        replace: r#"    (baseline.blockages..now).collect()"#,
        want: r#"destruction_runs_highest_index_first"#,
    },
    Mutation {
        name: r#"blockage-cleanup-endpoints-swapped"#,
        file: r#"src/apply.rs"#,
        find: r#"    (baseline.blockages..now).rev().collect()"#,
        replace: r#"    (now..baseline.blockages).rev().collect()"#,
        want: r#"a_shrunken_count_asks_for_nothing_rather_than_producing_garbage_indices"#,
    },
    Mutation {
        name: r#"refusal-commits"#,
        file: r#"src/apply.rs"#,
        find: r#"    if kept {
        Settlement::Committed
    } else {
        Settlement::RolledBack
    }"#,
        replace: r#"    let _ = kept;
    Settlement::Committed"#,
        want: r#"only_success_commits"#,
    },
    Mutation {
        name: r#"overlap-includes-touching"#,
        file: r#"src/design.rs"#,
        find: r#"        self.x_min < other.x_max
            && other.x_min < self.x_max"#,
        replace: r#"        self.x_min <= other.x_max
            && other.x_min <= self.x_max"#,
        want: r#"touching_rectangles_do_not_overlap"#,
    },
    Mutation {
        name: r#"ignore-check-before-block"#,
        file: r#"src/design.rs"#,
        find: r#"        if inst.is_block {
            m.num_macro += 1;"#,
        replace: r#"        if inst.is_block && !is_ignored_inst(inst) {
            m.num_macro += 1;"#,
        want: r#"an_ignorable_macro_STILL_counts_as_a_macro"#,
    },
    Mutation {
        name: r#"cover-not-exempt-from-fixed-error"#,
        file: r#"src/design.rs"#,
        find: r#"        } else if inst.is_fixed
            && !inst.master.is_cover"#,
        replace: r#"        } else if inst.is_fixed
            && true"#,
        want: r#"a_fixed_cover_cell_inside_the_area_is_allowed"#,
    },
    Mutation {
        name: r#"fixed-error-ignores-the-area"#,
        file: r#"src/design.rs"#,
        find: r#"            && inst.bbox.overlaps(placement_area)"#,
        replace: r#"            && true"#,
        want: r#"a_fixed_cell_outside_the_placement_area_is_allowed_and_counted"#,
    },
    Mutation {
        name: r#"ignored-std-cells-counted"#,
        file: r#"src/design.rs"#,
        find: r#"        } else if !is_ignored_inst(inst) {"#,
        replace: r#"        } else if true {"#,
        want: r#"an_ignored_standard_cell_counts_as_neither"#,
    },
    Mutation {
        name: r#"ignorable-flag-applies-to-std-cells"#,
        file: r#"src/design.rs"#,
        find: r#"    if inst.is_block && inst.is_ignorable_macro {"#,
        replace: r#"    if inst.is_ignorable_macro {"#,
        want: r#"the_ignorable_flag_only_applies_to_macros"#,
    },
    Mutation {
        name: r#"metrics-do-not-recurse"#,
        file: r#"src/design.rs"#,
        find: r#"    for &child in &design.modules[module].children {"#,
        replace: r#"    for &child in &[] as &[usize] {"#,
        want: r#"metrics_accumulate_through_the_module_hierarchy"#,
    },
    Mutation {
        name: r#"fence-outside-core-falls-back"#,
        file: r#"src/design.rs"#,
        find: r#"    if shape.area() == 0 {
        None"#,
        replace: r#"    if false {
        None"#,
        want: r#"a_fence_outside_the_core_leaves_nothing_to_place_into"#,
    },
    Mutation {
        name: r#"unfixed-macros-includes-fixed"#,
        file: r#"src/design.rs"#,
        find: r#"        .filter(|(_, i)| i.is_block && !i.is_fixed)"#,
        replace: r#"        .filter(|(_, i)| i.is_block)"#,
        want: r#"only_unfixed_macros_are_the_placers_to_move"#,
    },
    Mutation {
        name: r#"empty-module-still-clustered"#,
        file: r#"src/tree.rs"#,
        find: r#"        if self.module_metrics[module].num_macro == 0
            && self.module_metrics[module].num_std_cell == 0
        {
            return None;
        }"#,
        replace: r#"        if false {
            return None;
        }"#,
        want: r#"a_module_with_no_instances_gets_no_cluster"#,
    },
    Mutation {
        name: r#"empty-module-skip-uses-or"#,
        file: r#"src/tree.rs"#,
        find: r#"        if self.module_metrics[module].num_macro == 0
            && self.module_metrics[module].num_std_cell == 0"#,
        replace: r#"        if self.module_metrics[module].num_macro == 0
            || self.module_metrics[module].num_std_cell == 0"#,
        want: r#"a_module_with_only_macros_still_gets_a_cluster"#,
    },
    Mutation {
        name: r#"cluster-named-by-leaf-name"#,
        file: r#"src/tree.rs"#,
        find: r#"self.design.modules[module].hierarchical_name.clone()"#,
        replace: r#"self.design.modules[module].name.clone()"#,
        want: r#"a_cluster_is_named_by_the_modules_hierarchical_name"#,
    },
    Mutation {
        name: r#"empty-glue-cluster-kept"#,
        file: r#"src/tree.rs"#,
        find: r#"        if c.leaf_std_cells.is_empty() && c.leaf_macros.is_empty() {
            return None;
        }"#,
        replace: r#"        if false {
            return None;
        }"#,
        want: r#"a_glue_cluster_with_no_leaves_is_DISCARDED"#,
    },
    Mutation {
        name: r#"glue-name-without-parens"#,
        file: r#"src/tree.rs"#,
        find: r#"({parent_name})_glue_logic"#,
        replace: r#"{parent_name}_glue_logic"#,
        want: r#"glue_logic_is_named_after_its_parent_in_parentheses"#,
    },
    Mutation {
        name: r#"glue-ignores-the-ignore-check"#,
        file: r#"src/tree.rs"#,
        find: r#"            if is_ignored_inst(inst) {
                continue;
            }"#,
        replace: r#"            if false {
                continue;
            }"#,
        want: r#"a_module_of_only_ignored_cells_produces_no_glue_cluster"#,
    },
    Mutation {
        name: r#"glue-files-macros-as-std-cells"#,
        file: r#"src/tree.rs"#,
        find: r#"            if inst.is_block {
                cluster.leaf_macros.push(i);
            } else {
                cluster.leaf_std_cells.push(i);
            }"#,
        replace: r#"            cluster.leaf_std_cells.push(i);"#,
        want: r#"glue_leaves_are_filed_by_whether_they_are_macros"#,
    },
    Mutation {
        name: r#"metrics-skip-held-modules"#,
        file: r#"src/tree.rs"#,
        find: r#"        for &module in &cluster.db_modules {
            m.num_std_cell += self.module_metrics[module].num_std_cell;"#,
        replace: r#"        for &module in &[] as &[usize] {
            m.num_std_cell += self.module_metrics[module].num_std_cell;"#,
        want: r#"cluster_metrics_count_both_leaves_and_held_modules"#,
    },
    Mutation {
        name: r#"macro-clusters-not-typed-hard"#,
        file: r#"src/tree.rs"#,
        find: r#"            c.cluster_type = ClusterType::HardMacro;"#,
        replace: r#"            c.cluster_type = ClusterType::Mixed;"#,
        want: r#"each_macro_becomes_its_own_hard_macro_cluster"#,
    },
    Mutation {
        name: r#"macro-per-cluster-includes-ignored"#,
        file: r#"src/tree.rs"#,
        find: r#"            if is_ignored_inst(inst) || !inst.is_block {"#,
        replace: r#"            if !inst.is_block {"#,
        want: r#"an_ignored_macro_does_not_become_its_own_cluster"#,
    },
    Mutation {
        name: r#"id-not-advanced"#,
        file: r#"src/tree.rs"#,
        find: r#"        let id = self.next_id;
        self.next_id += 1;
        id"#,
        replace: r#"        self.next_id"#,
        want: r#"ids_are_handed_out_in_creation_order"#,
    },
    Mutation {
        name: r#"root-flat-module-absorbed"#,
        file: r#"src/tree.rs"#,
        find: r#"                if is_root {"#,
        replace: r#"                if false {"#,
        want: r#"a_flat_module_at_the_ROOT_gets_a_glue_child"#,
    },
    Mutation {
        name: r#"nonroot-flat-module-gets-child"#,
        file: r#"src/tree.rs"#,
        find: r#"                if is_root {"#,
        replace: r#"                if true {"#,
        want: r#"the_SAME_flat_module_below_the_root_is_absorbed_instead"#,
    },
    Mutation {
        name: r#"absorbed-module-not-cleared"#,
        file: r#"src/tree.rs"#,
        find: r#"                    parent.db_modules.clear();"#,
        replace: r#"                    let _ = &parent.db_modules;"#,
        want: r#"the_SAME_flat_module_below_the_root_is_absorbed_instead"#,
    },
    Mutation {
        name: r#"glue-created-before-child-modules"#,
        file: r#"src/tree.rs"#,
        find: r#"            for i in 0..self.design.modules[module].children.len() {"#,
        replace: r#"            for i in (0..self.design.modules[module].children.len()).rev() {"#,
        want: r#"a_module_with_children_yields_one_cluster_each_then_the_glue"#,
    },
    Mutation {
        name: r#"merged-cluster-skips-its-modules"#,
        file: r#"src/tree.rs"#,
        find: r#"            for i in 0..parent.db_modules.len() {"#,
        replace: r#"            for i in 0..0 {"#,
        want: r#"a_merged_cluster_splits_by_module_and_then_by_its_own_leaves"#,
    },
    Mutation {
        name: r#"merged-cluster-skips-its-leaves"#,
        file: r#"src/tree.rs"#,
        find: r#"            if !parent.leaf_std_cells.is_empty() || !parent.leaf_macros.is_empty() {"#,
        replace: r#"            if false {"#,
        want: r#"a_merged_cluster_splits_by_module_and_then_by_its_own_leaves"#,
    },
    Mutation {
        name: r#"recursion-ignores-module-check"#,
        file: r#"src/tree.rs"#,
        find: r#"            if !child.db_modules.is_empty()
                && crate::cluster::should_break(child, max_std_cell, max_macro)"#,
        replace: r#"            if crate::cluster::should_break(child, max_std_cell, max_macro)"#,
        want: r#"a_glue_child_with_no_module_is_never_recursed_into"#,
    },
    Mutation {
        name: r#"recursion-always-descends"#,
        file: r#"src/tree.rs"#,
        find: r#"                && crate::cluster::should_break(child, max_std_cell, max_macro)
            {"#,
        replace: r#"            {"#,
        want: r#"a_child_that_fits_is_left_alone"#,
    },
    Mutation {
        name: r#"recursion-passes-is-root"#,
        file: r#"src/tree.rs"#,
        find: r#"                self.break_cluster(child, false, max_std_cell, max_macro, min_std_cell, min_macro);"#,
        replace: r#"                self.break_cluster(child, true, max_std_cell, max_macro, min_std_cell, min_macro);"#,
        want: r#"a_recursed_child_is_never_treated_as_the_root"#,
    },
    Mutation {
        name: r#"merge-candidates-not-collected"#,
        file: r#"src/tree.rs"#,
        find: r#"                .filter(|c| crate::cluster::is_merge_candidate(c, min_std_cell, min_macro))"#,
        replace: r#"                .filter(|_c| false)"#,
        want: r#"small_children_are_reported_in_child_order_and_not_merged"#,
    },
    Mutation {
        name: r#"merge-candidates-ignore-thresholds"#,
        file: r#"src/tree.rs"#,
        find: r#"                .filter(|c| crate::cluster::is_merge_candidate(c, min_std_cell, min_macro))"#,
        replace: r#"                .filter(|_c| true)"#,
        want: r#"a_child_above_the_minimum_is_not_a_merge_candidate"#,
    },
    Mutation {
        name: r#"supply-nets-counted"#,
        file: r#"src/netlist.rs"#,
        find: r#"    if net.is_supply {
        return false;
    }"#,
        replace: r#"    if false {
        return false;
    }"#,
        want: r#"a_supply_net_is_never_valid"#,
    },
    Mutation {
        name: r#"ignored-only-nets-counted"#,
        file: r#"src/netlist.rs"#,
        find: r#"        .any(|t| !is_ignored_inst(&design.instances[t.inst]))"#,
        replace: r#"        .any(|_t| true)"#,
        want: r#"a_net_touching_only_ignored_instances_is_not_valid"#,
    },
    Mutation {
        name: r#"valid-net-needs-all-unignored"#,
        file: r#"src/netlist.rs"#,
        find: r#"        .any(|t| !is_ignored_inst(&design.instances[t.inst]))"#,
        replace: r#"        .all(|t| !is_ignored_inst(&design.instances[t.inst]))"#,
        want: r#"one_unignored_instance_is_enough_to_make_a_net_valid"#,
    },
    Mutation {
        name: r#"port-input-is-a-load"#,
        file: r#"src/netlist.rs"#,
        find: r#"            if p.is_input {
                driver = Some(id);
            } else {
                loads.push(id);
            }"#,
        replace: r#"            if p.is_input {
                loads.push(id);
            } else {
                driver = Some(id);
            }"#,
        want: r#"a_block_INPUT_port_is_the_driver_the_inverse_of_the_instance_rule"#,
    },
    Mutation {
        name: r#"ports-read-despite-io-pads"#,
        file: r#"src/netlist.rs"#,
        find: r#"    if !design_has_io_pads {"#,
        replace: r#"    if true {"#,
        want: r#"ports_are_ignored_entirely_when_the_design_has_io_pads"#,
    },
    Mutation {
        name: r#"first-output-wins"#,
        file: r#"src/netlist.rs"#,
        find: r#"        if t.is_output {
            driver = Some(id);"#,
        replace: r#"        if t.is_output {
            driver = driver.or(Some(id));"#,
        want: r#"the_LAST_output_wins_on_a_multiply_driven_net"#,
    },
    Mutation {
        name: r#"large-net-threshold-is-strict"#,
        file: r#"src/netlist.rs"#,
        find: r#"    if net.loads.is_empty() || net.loads.len() >= large_net_threshold {"#,
        replace: r#"    if net.loads.is_empty() || net.loads.len() > large_net_threshold {"#,
        want: r#"a_large_net_is_dropped_at_the_threshold_not_past_it"#,
    },
    Mutation {
        name: r#"self-connection-kept"#,
        file: r#"src/netlist.rs"#,
        find: r#"        .filter(|&&load| load != driver)"#,
        replace: r#"        .filter(|&&_load| true)"#,
        want: r#"a_load_in_the_drivers_own_cluster_is_skipped"#,
    },
    Mutation {
        name: r#"loads-deduplicated"#,
        file: r#"src/netlist.rs"#,
        find: r#"    net.loads
        .iter()
        .filter(|&&load| load != driver)"#,
        replace: r#"    let mut ded = net.loads.clone(); ded.sort_unstable(); ded.dedup();
    ded
        .iter()
        .filter(|&&load| load != driver)"#,
        want: r#"duplicate_loads_are_NOT_deduplicated"#,
    },
    Mutation {
        name: r#"connection-is-one-sided"#,
        file: r#"src/netlist.rs"#,
        find: r#"        *self.per_cluster.entry(b).or_default().entry(a).or_insert(0.0) += weight;"#,
        replace: r#"        let _ = b;"#,
        want: r#"a_connection_is_recorded_on_both_clusters"#,
    },
    Mutation {
        name: r#"weights-overwrite-not-accumulate"#,
        file: r#"src/netlist.rs"#,
        find: r#"        *self.per_cluster.entry(a).or_default().entry(b).or_insert(0.0) += weight;"#,
        replace: r#"        self.per_cluster.entry(a).or_default().insert(b, weight);"#,
        want: r#"weights_accumulate_across_nets"#,
    },
    Mutation {
        name: r#"strong-conn-no-subtraction"#,
        file: r#"src/merge.rs"#,
        find: r#"    let total = all_connections_weight(conns, a) + all_connections_weight(conns, b) - weight;"#,
        replace: r#"    let total = all_connections_weight(conns, a) + all_connections_weight(conns, b);"#,
        want: r#"the_shared_connection_is_subtracted_from_the_denominator_once"#,
    },
    Mutation {
        name: r#"strong-conn-ratio-strict"#,
        file: r#"src/merge.rs"#,
        find: r#"    weight / total >= MINIMUM_CONNECTION_RATIO"#,
        replace: r#"    weight / total > 1.0"#,
        want: r#"a_sole_connection_is_always_strong"#,
    },
    Mutation {
        name: r#"neighbors-use-pair-denominator"#,
        file: r#"src/merge.rs"#,
        find: r#"    let total = all_connections_weight(conns, target);
    if total <= 0.0 {
        return Vec::new();
    }"#,
        replace: r#"    let total = all_connections_weight(conns, target) * 100.0;
    if total <= 0.0 {
        return Vec::new();
    }"#,
        want: r#"neighbors_use_the_targets_own_total_not_the_pairs"#,
    },
    Mutation {
        name: r#"neighbors-keep-the-ignored"#,
        file: r#"src/merge.rs"#,
        find: r#"        .filter(|&(id, _)| id != ignored)"#,
        replace: r#"        .filter(|&(_id, _)| true)"#,
        want: r#"the_ignored_cluster_is_excluded"#,
    },
    Mutation {
        name: r#"empty-signature-matches"#,
        file: r#"src/merge.rs"#,
        find: r#"    if an.is_empty() {
        return false;
    }"#,
        replace: r#"    if false {
        return false;
    }"#,
        want: r#"two_isolated_clusters_do_NOT_share_a_signature"#,
    },
    Mutation {
        name: r#"signature-ignores-order"#,
        file: r#"src/merge.rs"#,
        find: r#"    an.sort_unstable();
    bn.sort_unstable();
    an == bn"#,
        replace: r#"    an.sort_unstable();
    bn.sort_unstable();
    true"#,
        want: r#"different_neighbours_are_not_the_same_signature"#,
    },
    Mutation {
        name: r#"max-thresholds-strict"#,
        file: r#"src/merge.rs"#,
        find: r#"    (a.num_macro() + b.num_macro()) <= max_macro
        && (a.num_std_cell() + b.num_std_cell()) <= max_std_cell"#,
        replace: r#"    (a.num_macro() + b.num_macro()) < max_macro
        && (a.num_std_cell() + b.num_std_cell()) < max_std_cell"#,
        want: r#"a_merge_landing_exactly_on_a_maximum_is_allowed"#,
    },
    Mutation {
        name: r#"single-candidate-takes-first"#,
        file: r#"src/merge.rs"#,
        find: r#"    if count == 1 {
        found
    } else {
        None
    }"#,
        replace: r#"    found"#,
        want: r#"exactly_one_well_formed_candidate_is_required"#,
    },
    Mutation {
        name: r#"small-candidates-allowed"#,
        file: r#"src/merge.rs"#,
        find: r#"        if small_ids.contains(&candidate) {
            continue;
        }"#,
        replace: r#"        if false {
            continue;
        }"#,
        want: r#"a_small_candidate_is_not_well_formed"#,
    },
    Mutation {
        name: r#"io-candidates-allowed"#,
        file: r#"src/merge.rs"#,
        find: r#"        if candidate == target || is_io_cluster(candidate) {"#,
        replace: r#"        if candidate == target {"#,
        want: r#"an_io_cluster_is_never_the_candidate"#,
    },
    Mutation {
        name: r#"merge-name-not-joined"#,
        file: r#"src/merge.rs"#,
        find: r#"    receiver.name = format!("#,
        replace: r#"    let _unused = format!("#,
        want: r#"merging_joins_names_with_a_double_pipe"#,
    },
    Mutation {
        name: r#"receiver-with-children-absorbs"#,
        file: r#"src/merge.rs"#,
        find: r#"    if !receiver.children.is_empty() {
        receiver.children.push(incomer);
        return false;
    }"#,
        replace: r#"    if false {
        return false;
    }"#,
        want: r#"a_receiver_with_children_ADOPTS_the_incomer_instead_of_dissolving_it"#,
    },
    Mutation {
        name: r#"dust-allows-a-macro"#,
        file: r#"src/merge.rs"#,
        find: r#"    cluster.num_std_cell() <= DUST_CLUSTER_STD_CELL && cluster.num_macro() == 0"#,
        replace: r#"    cluster.num_std_cell() <= DUST_CLUSTER_STD_CELL"#,
        want: r#"dust_is_a_few_cells_and_no_macros"#,
    },
    Mutation {
        name: r#"dust-limit-strict"#,
        file: r#"src/merge.rs"#,
        find: r#"    cluster.num_std_cell() <= DUST_CLUSTER_STD_CELL &&"#,
        replace: r#"    cluster.num_std_cell() < DUST_CLUSTER_STD_CELL &&"#,
        want: r#"dust_is_a_few_cells_and_no_macros"#,
    },
    Mutation {
        name: r#"merge-loop-uses-swap-remove"#,
        file: r#"src/merge.rs"#,
        find: r#"    let incomer = parent.children.remove(ii);"#,
        replace: r#"    let incomer = parent.children.swap_remove(ii);"#,
        want: r#"merging_preserves_sibling_order"#,
    },
    Mutation {
        name: r#"type1-ignores-max-thresholds"#,
        file: r#"src/merge.rs"#,
        find: r#"            if !merge_honors_max_thresholds(
                &parent.children[ci],
                &parent.children[si],
                max_std_cell,
                max_macro,
            ) {
                continue;
            }"#,
        replace: r#"            if false {
                continue;
            }"#,
        want: r#"type_1_is_skipped_when_the_merge_would_break_a_maximum"#,
    },
    Mutation {
        name: r#"type1-disabled"#,
        file: r#"src/merge.rs"#,
        find: r#"        for i in 0..small.len() {
            let Some(close) ="#,
        replace: r#"        for i in 0..0 {
            let Some(close) ="#,
        want: r#"a_small_cluster_merges_into_its_single_well_formed_neighbour"#,
    },
    Mutation {
        name: r#"type1-absorbs-into-the-small-one"#,
        file: r#"src/merge.rs"#,
        find: r#"            if merge_siblings(parent, close, small[i]) {
                absorbed[i] = true;
                report.merged.push((close, small[i]));"#,
        replace: r#"            if merge_siblings(parent, small[i], close) {
                absorbed[i] = true;
                report.merged.push((small[i], close));"#,
        want: r#"type_1_takes_precedence_over_type_2"#,
    },
    Mutation {
        name: r#"type3-ignores-dust-check"#,
        file: r#"src/merge.rs"#,
        find: r#"            if !is_dust(&parent.children[ii]) {
                continue;
            }"#,
        replace: r#"            if false {
                continue;
            }"#,
        want: r#"a_non_dust_receiver_does_not_absorb_dust"#,
    },
    Mutation {
        name: r#"type3-disabled"#,
        file: r#"src/merge.rs"#,
        find: r#"            survivors.push(small[i]);
            let Some(ii) = index_of(parent, small[i]) else { continue };"#,
        replace: r#"            survivors.push(small[i]);
            let Some(ii) = index_of(parent, small[i]) else { continue };
            if true { continue; }"#,
        want: r#"dust_absorbs_dust_when_nothing_else_applies"#,
    },
    Mutation {
        name: r#"connections-built-once"#,
        file: r#"src/merge.rs"#,
        find: r#"        let conns = rebuild_connections(parent);"#,
        replace: r#"        let conns = Connections::new(); let _ = &rebuild_connections;"#,
        want: r#"a_small_cluster_merges_into_its_single_well_formed_neighbour"#,
    },
    Mutation {
        name: r#"empty-small-list-still-loops"#,
        file: r#"src/merge.rs"#,
        find: r#"    if small.is_empty() {
        return report;
    }"#,
        replace: r#"    if false {
        return report;
    }"#,
        want: r#"no_small_children_means_no_rounds"#,
    },
    Mutation {
        name: r#"dump-drops-macro-trailing-comma"#,
        file: r#"src/dump.rs"#,
        find: r#", Macros: {} ({} μ²),"#,
        replace: r#", Macros: {} ({} μ²)"#,
        want: r#"the_macro_field_ends_with_a_trailing_comma"#,
    },
    Mutation {
        name: r#"dump-uses-and-not-or"#,
        file: r#"src/dump.rs"#,
        find: r#"        if cluster.num_std_cell() != 0 || cluster.std_cell_area() != 0 {"#,
        replace: r#"        if cluster.num_std_cell() != 0 && cluster.std_cell_area() != 0 {"#,
        want: r#"a_field_prints_when_the_area_is_nonzero_even_if_the_count_is_zero"#,
    },
    Mutation {
        name: r#"dump-single-space-before-id"#,
        file: r#"src/dump.rs"#,
        find: r#"{}  ({}) Type: {}"#,
        replace: r#"{} ({}) Type: {}"#,
        want: r#"the_dump_matches_output_captured_from_upstream"#,
    },
    Mutation {
        name: r#"dump-indent-wrong"#,
        file: r#"src/dump.rs"#,
        find: r#"        out.push_str("+---");"#,
        replace: r#"        out.push_str("+--");"#,
        want: r#"depth_is_marked_with_one_prefix_per_level"#,
    },
    Mutation {
        name: r#"dump-pin-clusters-print-counts"#,
        file: r#"src/dump.rs"#,
        find: r#"    if cluster.is_cluster_of_unplaced_io_pins || cluster.is_io_bundle {"#,
        replace: r#"    if false {"#,
        want: r#"a_pin_cluster_prints_pins_and_nothing_else"#,
    },
    Mutation {
        name: r#"dump-io-pads-print-counts"#,
        file: r#"src/dump.rs"#,
        find: r#"    } else if !cluster.is_io_pad_cluster {"#,
        replace: r#"    } else if true {"#,
        want: r#"an_io_pad_cluster_prints_neither_pins_nor_counts"#,
    },
    Mutation {
        name: r#"type-string-ignores-fixed-macro"#,
        file: r#"src/cluster.rs"#,
        find: r#"        if self.is_fixed_macro {
            return "Fixed Macro";
        }"#,
        replace: r#"        if false {
            return "Fixed Macro";
        }"#,
        want: r#"the_type_string_checks_io_and_fixed_before_the_ordinary_type"#,
    },
    Mutation {
        name: r#"leaf-string-ignores-children"#,
        file: r#"src/cluster.rs"#,
        find: r#"        if !self.is_io_cluster() && self.children.is_empty() {"#,
        replace: r#"        if !self.is_io_cluster() {"#,
        want: r#"a_non_leaf_keeps_the_space_before_the_comma"#,
    },
    Mutation {
        name: r#"dump-children-reversed"#,
        file: r#"src/dump.rs"#,
        find: r#"    for child in &cluster.children {"#,
        replace: r#"    for child in cluster.children.iter().rev() {"#,
        want: r#"children_print_in_order_after_their_parent"#,
    },
    Mutation {
        name: r#"autocluster-else-recurses-children"#,
        file: r#"src/tree.rs"#,
        find: r#"            let same = self.multilevel_autocluster(
                parent,
                is_root,
                level,"#,
        replace: r#"            let same = self.multilevel_autocluster(
                parent,
                is_root,
                level - 1,"#,
        want: r#"a_cluster_that_fits_descends_a_level_WITHOUT_splitting"#,
    },
    Mutation {
        name: r#"force-split-at-every-level"#,
        file: r#"src/tree.rs"#,
        find: r#"        let force_split_root = if level == 0 {"#,
        replace: r#"        let force_split_root = if true {"#,
        want: r#"force_split_is_only_considered_at_the_top"#,
    },
    Mutation {
        name: r#"force-split-never"#,
        file: r#"src/tree.rs"#,
        find: r#"            parent.num_std_cell() < leaf_max_std_cell"#,
        replace: r#"            false"#,
        want: r#"a_root_smaller_than_a_leaf_is_force_split_anyway"#,
    },
    Mutation {
        name: r#"force-split-uses-current-max"#,
        file: r#"src/tree.rs"#,
        find: r#"                (base.max_std_cell as f64 / (cluster_size_ratio as f64).powi(max_level - 1)) as i32;"#,
        replace: r#"                base.max_std_cell;"#,
        want: r#"force_split_measures_against_the_LEAF_maximum_not_the_base_one"#,
    },
    Mutation {
        name: r#"level-limit-off-by-one"#,
        file: r#"src/tree.rs"#,
        find: r#"        if level >= max_level {"#,
        replace: r#"        if level > max_level {"#,
        want: r#"a_descent_that_reaches_the_level_limit_stops"#,
    },
    Mutation {
        name: r#"thresholds-before-increment"#,
        file: r#"src/tree.rs"#,
        find: r#"        let level = level + 1;
        let t = crate::thresholds::update_size_thresholds(base, level, cluster_size_ratio);"#,
        replace: r#"        let t = crate::thresholds::update_size_thresholds(base, level, cluster_size_ratio);
        let level = level + 1;"#,
        want: r#"a_flat_cluster_needing_the_partitioner_is_reported_up_the_descent"#,
    },
    Mutation {
        name: r#"children-inherit-is-root"#,
        file: r#"src/tree.rs"#,
        find: r#"                let child_outcome = self.multilevel_autocluster(
                    child,
                    false,"#,
        replace: r#"                let child_outcome = self.multilevel_autocluster(
                    child,
                    is_root,"#,
        want: r#"a_recursed_child_that_BREAKS_is_not_treated_as_the_root"#,
    },
    Mutation {
        name: r#"partitioning-not-propagated"#,
        file: r#"src/tree.rs"#,
        find: r#"            outcome.needs_partitioning.extend(sub.needs_partitioning);"#,
        replace: r#"            let _ = &sub.needs_partitioning;"#,
        want: r#"a_flat_cluster_needing_the_partitioner_is_reported_up_the_descent"#,
    },
    Mutation {
        name: r#"merge-candidates-not-propagated"#,
        file: r#"src/tree.rs"#,
        find: r#"            if !breaks.merge_candidates.is_empty() {"#,
        replace: r#"            if false {"#,
        want: r#"merge_candidates_are_reported_per_parent"#,
    },
    Mutation {
        name: r#"right-edge-indexes-forward"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"        Some(per_edge * 2 + div_floor(die.y_max - y_center, spans.y))"#,
        replace: r#"        Some(per_edge * 2 + div_floor(y_center - die.y_min, spans.y))"#,
        want: r#"the_right_edge_indexes_BACKWARD_from_the_top"#,
    },
    Mutation {
        name: r#"bottom-edge-indexes-forward"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"        Some(per_edge * 3 + div_floor(die.x_max - x_center, spans.x))"#,
        replace: r#"        Some(per_edge * 3 + div_floor(x_center - die.x_min, spans.x))"#,
        want: r#"the_bottom_edge_indexes_BACKWARD_from_the_right"#,
    },
    Mutation {
        name: r#"edge-order-B-before-L"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"    if pin.x_min <= die.x_min {"#,
        replace: r#"    if pin.y_min <= die.y_min {"#,
        want: r#"the_left_edge_indexes_FORWARD_from_the_bottom"#,
    },
    Mutation {
        name: r#"bundle-order-not-LTRB"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"[Boundary::L, Boundary::T, Boundary::R, Boundary::B]"#,
        replace: r#"[Boundary::L, Boundary::B, Boundary::R, Boundary::T]"#,
        want: r#"bundles_are_named_and_ordered_L_T_R_B"#,
    },
    Mutation {
        name: r#"offset-uses-wrong-multiplier"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"        Some(per_edge + div_floor(x_center - die.x_min, spans.x))"#,
        replace: r#"        Some(div_floor(x_center - die.x_min, spans.x))"#,
        want: r#"the_top_edge_indexes_FORWARD_from_the_left"#,
    },
    Mutation {
        name: r#"interior-pin-gets-a-bundle"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"    } else {
        None
    }
}"#,
        replace: r#"    } else {
        Some(0)
    }
}"#,
        want: r#"a_pin_touching_no_edge_belongs_to_no_bundle"#,
    },
    Mutation {
        name: r#"zero-span-divides"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"    if b == 0 {
        return 0;
    }"#,
        replace: r#"    if false {
        return 0;
    }"#,
        want: r#"a_degenerate_span_yields_the_first_bundle_rather_than_a_wild_index"#,
    },
    Mutation {
        name: r#"right-rect-advances"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"            y_min: die.y_max - ext * (i + 1),"#,
        replace: r#"            y_min: die.y_min + ext * i,"#,
        want: r#"bundle_rectangles_advance_on_the_left_and_retreat_on_the_right"#,
    },
    Mutation {
        name: r#"bottom-rect-advances"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"            x_min: die.x_max - ext * (i + 1),"#,
        replace: r#"            x_min: die.x_min + ext * i,"#,
        want: r#"bundle_rectangles_advance_on_the_left_and_retreat_on_the_right"#,
    },
    Mutation {
        name: r#"vertical-edge-uses-x-span"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"    let ext = if edge.is_vertical() { spans.y } else { spans.x };"#,
        replace: r#"    let ext = spans.x;"#,
        want: r#"the_bundles_on_an_edge_tile_it_without_gaps_or_overlap"#,
    },
    Mutation {
        name: r#"left-rect-has-thickness"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"            x_max: die.x_min,
            y_max: die.y_min + ext * i + ext,"#,
        replace: r#"            x_max: die.x_min + 10,
            y_max: die.y_min + ext * i + ext,"#,
        want: r#"a_bundle_rectangle_sits_ON_its_edge_with_no_thickness"#,
    },
    Mutation {
        name: r#"spans-not-divided"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"        x: (die.x_max - die.x_min) / IO_BUNDLES_PER_EDGE as i64,"#,
        replace: r#"        x: (die.x_max - die.x_min),"#,
        want: r#"the_die_is_divided_into_five_per_edge"#,
    },
    Mutation {
        name: r#"bundles-without-a-fixed-pin"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"    if any_fixed {
        for edge in BUNDLE_EDGE_ORDER {"#,
        replace: r#"    if true {
        for edge in BUNDLE_EDGE_ORDER {"#,
        want: r#"bundles_are_created_only_when_a_pin_is_FIXED"#,
    },
    Mutation {
        name: r#"empty-bundles-kept"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"    out.bundles.retain(|b| b.num_io_pins > 0);"#,
        replace: r#"    out.bundles.retain(|_b| true);"#,
        want: r#"empty_bundles_are_RELEASED"#,
    },
    Mutation {
        name: r#"bundle-pins-not-counted"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"                b.num_io_pins += 1;"#,
        replace: r#"                let _ = &b.num_io_pins;"#,
        want: r#"several_pins_in_one_bundle_are_counted"#,
    },
    Mutation {
        name: r#"unconstrained-not-shared"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"            None => unconstrained,"#,
        replace: r#"            None => None,"#,
        want: r#"every_unconstrained_pin_shares_ONE_cluster"#,
    },
    Mutation {
        name: r#"constraint-matched-by-overlap"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"                .find(|c| c.constraint_region.as_ref() == Some(region))"#,
        replace: r#"                .find(|c| c.constraint_region.is_some())"#,
        want: r#"pins_with_DIFFERENT_constraints_get_separate_clusters"#,
    },
    Mutation {
        name: r#"constrained-joins-unconstrained"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"            None => {
                unconstrained = Some(c.id);
            }"#,
        replace: r#"            None => {}"#,
        // ⚠️ Dropping the assignment means no cluster is ever REMEMBERED as the unconstrained one,
        // so every unconstrained pin makes its own. That breaks SHARING, not the constrained-pin
        // exclusion the old `want` named — which is why it reported WRONG TEST rather than a hole.
        want: r#"every_unconstrained_pin_shares_ONE_cluster"#,
    },
    Mutation {
        name: r#"no-ports-still-has-io-clusters"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"        out.has_io_clusters = false;"#,
        replace: r#"        out.has_io_clusters = true;"#,
        want: r#"a_design_with_no_ports_has_no_io_clusters"#,
    },
    Mutation {
        name: r#"pin-cluster-named-by-count"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"format!("ios_{}", out.next_id)"#,
        replace: r#"format!("ios_{}", out.pin_clusters.len())"#,
        want: r#"a_pin_cluster_is_named_after_its_own_id"#,
    },
    Mutation {
        name: r#"fixed-pins-not-bundled"#,
        file: r#"src/ioclusters.rs"#,
        find: r#"        if any_fixed && pin.is_fixed {"#,
        replace: r#"        if false {"#,
        want: r#"a_fixed_pin_lands_in_the_bundle_its_position_selects"#,
    },
    Mutation {
        name: r#"size-compares-area"#,
        file: r#"src/macroclass.rs"#,
        find: r#"                if class[j] == -1 && sizes[i] == sizes[j] {"#,
        replace: r#"                if class[j] == -1 && sizes[i].width * sizes[i].height == sizes[j].width * sizes[j].height {"#,
        want: r#"size_compares_width_AND_height_not_area"#,
    },
    Mutation {
        name: r#"size-assigns-in-first-pass"#,
        file: r#"src/macroclass.rs"#,
        find: r#"        .map(|(i, &c)| if c == -1 { i } else { c as usize })"#,
        replace: r#"        .map(|(_i, &c)| if c == -1 { 0 } else { c as usize })"#,
        want: r#"an_unmatched_macro_represents_itself"#,
    },
    Mutation {
        name: r#"signature-empty-matches"#,
        file: r#"src/macroclass.rs"#,
        find: r#"            if class[j] == -1 && same_connection_signature(conns, ids[i], ids[j]) {"#,
        replace: r#"            if class[j] == -1 {"#,
        want: r#"unconnected_macros_do_NOT_share_a_signature"#,
    },
    Mutation {
        name: r#"interconn-scans-only-later"#,
        file: r#"src/macroclass.rs"#,
        find: r#"        for j in 0..ids.len() {
            if i == j {
                continue;
            }"#,
        replace: r#"        for j in (i + 1)..ids.len() {
            if i == j {
                continue;
            }"#,
        want: r#"the_interconnection_pass_scans_EARLIER_macros_too"#,
    },
    Mutation {
        name: r#"interconn-does-not-adopt"#,
        file: r#"src/macroclass.rs"#,
        find: r#"                if class[j] != -1 {
                    // Adopt the neighbour's group and stop looking.
                    class[i] = class[j];
                    break;
                }"#,
        replace: r#"                if class[j] != -1 {
                    break;
                }"#,
        want: r#"a_macro_can_ADOPT_a_class_led_by_a_higher_index"#,
    },
    Mutation {
        name: r#"grouping-ignores-size"#,
        file: r#"src/macroclass.rs"#,
        find: r#"            if macro_class[j] != -1 || size_class[i] != size_class[j] {"#,
        replace: r#"            if macro_class[j] != -1 {"#,
        want: r#"a_merge_ALWAYS_requires_the_same_size"#,
    },
    Mutation {
        name: r#"grouping-does-not-clear-interconn"#,
        file: r#"src/macroclass.rs"#,
        find: r#"                interconn[i] = -1;"#,
        replace: r#"                let _ = &interconn;"#,
        want: r#"meeting_a_different_interconnection_CLEARS_the_leaders_own_class"#,
    },
    Mutation {
        name: r#"grouping-signature-not-checked"#,
        file: r#"src/macroclass.rs"#,
        find: r#"                if signature_class[i] == signature_class[j] {"#,
        replace: r#"                if false {"#,
        want: r#"same_size_and_same_signature_merges_when_the_interconnection_differs"#,
    },
    Mutation {
        name: r#"grouping-interconn-not-checked"#,
        file: r#"src/macroclass.rs"#,
        find: r#"            if interconn[i] == interconn[j] {"#,
        replace: r#"            if false {"#,
        want: r#"same_size_and_same_interconnection_makes_an_interconnected_array"#,
    },
    Mutation {
        name: r#"single-macro-is-an-array"#,
        file: r#"src/macroclass.rs"#,
        find: r#"    Grouping { macro_class: vec![0], interconn_class: vec![-1], merges: Vec::new() }"#,
        replace: r#"    Grouping { macro_class: vec![0], interconn_class: vec![0], merges: Vec::new() }"#,
        want: r#"a_single_movable_macro_is_never_an_array_of_one"#,
    },
    Mutation {
        name: r#"fixed-macros-classified"#,
        file: r#"src/macroclass.rs"#,
        find: r#"    let movable: Vec<&MacroCluster> = macros.iter().filter(|m| !m.is_fixed).collect();"#,
        replace: r#"    let movable: Vec<&MacroCluster> = macros.iter().collect();"#,
        want: r#"a_FIXED_macro_is_never_folded_into_an_array"#,
    },
    Mutation {
        name: r#"fixed-clusters-not-separated"#,
        file: r#"src/macroclass.rs"#,
        find: r#"        macros.iter().filter(|m| m.is_fixed).map(|m| m.id).collect();"#,
        replace: r#"        Vec::new();"#,
        want: r#"a_FIXED_macro_is_never_folded_into_an_array"#,
    },
    Mutation {
        name: r#"single-macro-not-special-cased"#,
        file: r#"src/macroclass.rs"#,
        find: r#"    let grouping = if movable.len() == 1 {"#,
        replace: r#"    let grouping = if false {"#,
        want: r#"a_single_movable_macro_is_not_an_interconnected_array"#,
    },
    Mutation {
        name: r#"non-leaders-survive"#,
        file: r#"src/macroclass.rs"#,
        find: r#"        if grouping.macro_class.get(i) != Some(&i) {
            continue;
        }"#,
        replace: r#"        if false {
            continue;
        }"#,
        want: r#"merged_macros_contribute_ONE_virtual_connection_not_one_each"#,
    },
    Mutation {
        name: r#"interconnected-flag-inverted"#,
        file: r#"src/macroclass.rs"#,
        find: r#"            is_interconnected_array: grouping.interconn_class.get(i).copied().unwrap_or(-1) != -1,"#,
        replace: r#"            is_interconnected_array: grouping.interconn_class.get(i).copied().unwrap_or(-1) == -1,"#,
        want: r#"a_wired_group_is_marked_as_an_interconnected_array"#,
    },
    Mutation {
        name: r#"std-cell-cluster-not-connected"#,
        file: r#"src/macroclass.rs"#,
        find: r#"    let mut virtual_members = vec![mixed_leaf];"#,
        replace: r#"    let mut virtual_members: Vec<ClusterId> = vec![];"#,
        want: r#"virtual_connections_join_every_pair"#,
    },
    Mutation {
        name: r#"virtual-pairs-only-adjacent"#,
        file: r#"src/macroclass.rs"#,
        find: r#"        for j in (i + 1)..virtual_members.len() {"#,
        replace: r#"        for j in (i + 1)..(i + 2).min(virtual_members.len()) {"#,
        want: r#"virtual_connections_join_every_pair"#,
    },
    Mutation {
        name: r#"fixed-clusters-not-virtually-connected"#,
        file: r#"src/macroclass.rs"#,
        find: r#"    virtual_members.extend(fixed_clusters.iter().copied());"#,
        replace: r#"    let _ = &fixed_clusters;"#,
        want: r#"virtual_connections_join_every_pair"#,
    },
    Mutation {
        name: r#"a-stage-dropped-from-order"#,
        file: r#"src/pipeline.rs"#,
        find: r#"    StageId::ComputeWireLength,
];"#,
        replace: r#"];"#,
        want: r#"the_pipeline_matches_the_spec_table"#,
    },
    Mutation {
        name: r#"macros-of-drops-own-leaf-macros"#,
        file: r#"src/tree.rs"#,
        find: r#"    let mut out = cluster.leaf_macros.clone();
    for &m in &cluster.db_modules {"#,
        replace: r#"    let mut out = Vec::new();
    for &m in &cluster.db_modules {"#,
        want: r#"a_clusters_macros_are_its_own_then_its_modules_depth_first"#,
    },
    Mutation {
        name: r#"module-children-not-walked"#,
        file: r#"src/tree.rs"#,
        find: r#"        hard_macros_of_module(c, design, out);"#,
        replace: r#"        let _ = c;"#,
        want: r#"a_clusters_macros_are_its_own_then_its_modules_depth_first"#,
    },
    Mutation {
        name: r#"std-cell-cluster-claims-macros"#,
        file: r#"src/tree.rs"#,
        find: r#"        if !include_macro && inst.is_block {"#,
        replace: r#"        if false && inst.is_block {"#,
        want: r#"a_std_cell_cluster_never_claims_a_macro_in_its_module"#,
    },
    Mutation {
        name: r#"fixed-macro-not-lifted-to-root"#,
        file: r#"src/tree.rs"#,
        find: r#"            if c.is_fixed_macro && !is_root {"#,
        replace: r#"            if false {"#,
        want: r#"a_fixed_macro_is_lifted_to_the_root_rather_than_left_beside_its_siblings"#,
    },
    Mutation {
        name: r#"merged-name-uses-ids"#,
        file: r#"src/tree.rs"#,
        find: r#"                    .map(|d| d.name.clone())"#,
        replace: r#"                    .map(|d| d.id.to_string())"#,
        want: r#"merged_macros_leave_one_cluster_carrying_both_names_and_both_areas"#,
    },
    Mutation {
        name: r#"merged-macro-count-not-summed"#,
        file: r#"src/tree.rs"#,
        find: r#"                    c.metrics.num_macro += 1;"#,
        replace: r#"                    c.metrics.num_macro = 1;"#,
        want: r#"merged_macros_leave_one_cluster_carrying_both_names_and_both_areas"#,
    },
    Mutation {
        name: r#"mixed-leaf-not-retyped"#,
        file: r#"src/tree.rs"#,
        find: r#"        child.cluster_type = ClusterType::StdCell;
        child.metrics.num_macro = 0;"#,
        replace: r#"        child.metrics.num_macro = 0;"#,
        want: r#"a_mixed_leaf_becomes_a_std_cell_cluster_and_its_macros_become_siblings"#,
    },
    Mutation {
        name: r#"absorbed-cluster-kept-in-tree"#,
        file: r#"src/tree.rs"#,
        find: r#"            if !survivors.contains(&c.id) {
                continue;
            }"#,
        replace: r#"            if false {
                continue;
            }"#,
        want: r#"merged_macros_leave_one_cluster_carrying_both_names_and_both_areas"#,
    },
    Mutation {
        name: r#"blockages-summed-not-unioned"#,
        file: r#"src/feasibility.rs"#,
        find: r#"    occupied += union_area(&clipped);"#,
        replace: r#"    occupied += clipped.iter().map(|r| r.area()).sum::<i64>();"#,
        want: r#"overlapping_blockages_are_unioned_by_the_fit_test_itself"#,
    },
    Mutation {
        name: r#"fixed-cell-counted-whole"#,
        file: r#"src/feasibility.rs"#,
        find: r#"            occupied += intersection(&inst.bbox, area).map_or(0, |r| r.area());"#,
        replace: r#"            occupied += inst.bbox.area();"#,
        want: r#"a_fixed_cell_outside_the_area_contributes_only_the_part_inside"#,
    },
    Mutation {
        name: r#"fixed-cell-skipped-entirely"#,
        file: r#"src/feasibility.rs"#,
        find: r#"            occupied += intersection(&inst.bbox, area).map_or(0, |r| r.area());
            continue;"#,
        replace: r#"            continue;"#,
        want: r#"a_fixed_cell_INSIDE_the_area_still_occupies_it"#,
    },
    Mutation {
        name: r#"macro-measured-without-halo"#,
        file: r#"src/feasibility.rs"#,
        find: r#"        occupied += if inst.is_block { macro_area_with_halo(i) } else { inst.bbox.area() };"#,
        replace: r#"        occupied += inst.bbox.area();"#,
        want: r#"an_unfixed_macro_is_measured_with_its_halo_not_its_box"#,
    },
    Mutation {
        name: r#"fit-test-is-strict"#,
        file: r#"src/feasibility.rs"#,
        find: r#"    occupied <= area.area()"#,
        replace: r#"    occupied < area.area()"#,
        want: r#"exactly_filling_the_area_still_fits"#,
    },
    Mutation {
        name: r#"blockages-not-clipped"#,
        file: r#"src/feasibility.rs"#,
        find: r#"    let clipped: Vec<Rect> = blockages.iter().filter_map(|b| intersection(b, area)).collect();"#,
        replace: r#"    let clipped: Vec<Rect> = blockages.to_vec();"#,
        want: r#"blockages_are_clipped_to_the_placement_area_before_they_count"#,
    },
    Mutation {
        name: r#"core-fit-is-strict"#,
        file: r#"src/feasibility.rs"#,
        find: r#"    width_with_halo <= core.x_max - core.x_min && height_with_halo <= core.y_max - core.y_min"#,
        replace: r#"    width_with_halo < core.x_max - core.x_min && height_with_halo < core.y_max - core.y_min"#,
        want: r#"a_macro_exactly_as_wide_as_the_core_fits"#,
    },
    Mutation {
        name: r#"core-fit-ignores-height"#,
        file: r#"src/feasibility.rs"#,
        find: r#"    width_with_halo <= core.x_max - core.x_min && height_with_halo <= core.y_max - core.y_min"#,
        replace: r#"    width_with_halo <= core.x_max - core.x_min"#,
        want: r#"the_core_test_uses_both_dimensions_independently"#,
    },
    Mutation {
        name: r#"halo-area-drops-std-cells"#,
        file: r#"src/feasibility.rs"#,
        find: r#"    let inst_area_with_halos = macro_with_halo_area as f32 + std_cell_area as f32;"#,
        replace: r#"    let inst_area_with_halos = macro_with_halo_area as f32;"#,
        want: r#"the_halo_area_test_adds_the_macros_to_the_standard_cells"#,
    },
    Mutation {
        name: r#"union-ignores-x-spans"#,
        file: r#"src/feasibility.rs"#,
        find: r#"        total += covered * (x1 - x0);"#,
        replace: r#"        total += covered;"#,
        want: r#"disjoint_blockages_add_up"#,
    },
    Mutation {
        name: r#"tilings-skip-the-last-factor"#,
        file: r#"src/shaping.rs"#,
        find: r#"    for cols in 1..=number_of_macros {"#,
        replace: r#"    for cols in 1..number_of_macros {"#,
        want: r#"every_factorisation_that_fits_is_a_tiling_in_column_order"#,
    },
    Mutation {
        name: r#"tilings-ignore-non-divisors"#,
        file: r#"src/shaping.rs"#,
        find: r#"        if number_of_macros % cols != 0 {"#,
        replace: r#"        if false {"#,
        want: r#"every_factorisation_that_fits_is_a_tiling_in_column_order"#,
    },
    Mutation {
        name: r#"tiling-fit-checks-width-only"#,
        file: r#"src/shaping.rs"#,
        find: r#"        if w <= outline.x_max - outline.x_min && h <= outline.y_max - outline.y_min {"#,
        replace: r#"        if w <= outline.x_max - outline.x_min {"#,
        want: r#"a_tiling_must_fit_in_BOTH_dimensions"#,
    },
    Mutation {
        name: r#"tiling-fit-is-strict"#,
        file: r#"src/shaping.rs"#,
        find: r#"        if w <= outline.x_max - outline.x_min && h <= outline.y_max - outline.y_min {"#,
        replace: r#"        if w < outline.x_max - outline.x_min && h < outline.y_max - outline.y_min {"#,
        want: r#"a_tiling_exactly_filling_the_outline_is_kept"#,
    },
    Mutation {
        name: r#"tiling-rows-and-cols-swapped"#,
        file: r#"src/shaping.rs"#,
        find: r#"        let (w, h) = (cols * macro_width, rows * macro_height);"#,
        replace: r#"        let (w, h) = (rows * macro_width, cols * macro_height);"#,
        want: r#"every_factorisation_that_fits_is_a_tiling_in_column_order"#,
    },
    Mutation {
        name: r#"no-retry-with-one-more-macro"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if tilings.is_empty() {
        tilings = generate_tilings_for_macro_cluster("#,
        replace: r#"    if false {
        tilings = generate_tilings_for_macro_cluster("#,
        want: r#"when_nothing_fits_the_search_is_retried_with_one_more_macro"#,
    },
    Mutation {
        name: r#"retry-runs-unconditionally"#,
        file: r#"src/shaping.rs"#,
        find: r#"    let mut tilings =
        generate_tilings_for_macro_cluster(macro_width, macro_height, number_of_macros, outline);
    if tilings.is_empty() {"#,
        replace: r#"    let mut tilings: Vec<Tiling> = Vec::new();
    if tilings.is_empty() {"#,
        want: r#"the_retry_is_only_reached_when_the_first_search_found_nothing"#,
    },
    Mutation {
        name: r#"unshapeable-reports-the-retried-count"#,
        file: r#"src/shaping.rs"#,
        find: r#"        return Err(Unshapeable { macro_width, macro_height, number_of_macros });"#,
        replace: r#"        return Err(Unshapeable { macro_width, macro_height, number_of_macros: number_of_macros + 1 });"#,
        want: r#"a_cluster_that_fits_neither_count_is_unshapeable"#,
    },
    Mutation {
        name: r#"intervals-not-degenerate"#,
        file: r#"src/shaping.rs"#,
        find: r#"        tilings.iter().map(|t| Interval { min: t.width, max: t.width }).collect();"#,
        replace: r#"        tilings.iter().map(|t| Interval { min: t.width, max: t.height }).collect();"#,
        want: r#"each_tiling_becomes_one_degenerate_interval_sorted_by_width"#,
    },
    Mutation {
        name: r#"intervals-unsorted"#,
        file: r#"src/shaping.rs"#,
        find: r#"    out.sort_by_key(|i| i.min);"#,
        replace: r#"    out.reverse();"#,
        want: r#"each_tiling_becomes_one_degenerate_interval_sorted_by_width"#,
    },
    Mutation {
        name: r#"aspect-ratio-inverted"#,
        file: r#"src/shaping.rs"#,
        find: r#"        self.height as f32 / self.width as f32"#,
        replace: r#"        self.width as f32 / self.height as f32"#,
        want: r#"aspect_ratio_is_height_over_width"#,
    },
    Mutation {
        name: r#"root-width-from-height"#,
        file: r#"src/shaping.rs"#,
        find: r#"    let width = floorplan.x_max - floorplan.x_min;"#,
        replace: r#"    let width = floorplan.y_max - floorplan.y_min;"#,
        want: r#"the_root_takes_the_floorplan_shape_exactly"#,
    },
    Mutation {
        name: r#"shaping-base-case-is-leafness"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if parent.num_macro() == 0 {
        return Ok(());
    }"#,
        replace: r#"    if parent.children.is_empty() && parent.leaf_macros.is_empty() {
        return Ok(());
    }"#,
        want: r#"a_cluster_with_no_macros_is_not_shaped_and_neither_is_anything_below_it"#,
    },
    Mutation {
        name: r#"fixed-macro-cluster-is-shaped"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if cluster.is_fixed_macro {
        return Ok(());
    }"#,
        replace: r#"    if false {
        return Ok(());
    }"#,
        want: r#"a_FIXED_macro_cluster_is_left_with_no_tilings"#,
    },
    Mutation {
        name: r#"shortcut-applies-to-any-count"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if contributors.len() == 1 {"#,
        replace: r#"    if contributors.len() <= 2 {"#,
        want: r#"two_macro_bearing_children_are_shaped_by_the_search"#,
    },
    Mutation {
        name: r#"parent-shaped-before-its-children"#,
        file: r#"src/shaping.rs"#,
        find: r#"                calculate_children_tilings_traced(child, ctx, trace)?;"#,
        replace: r#"                let _skipped = child;"#,
        want: r#"children_are_shaped_before_the_parent_reads_them"#,
    },
    Mutation {
        name: r#"macro-cluster-type-ignored"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if parent.cluster_type == ClusterType::HardMacro {
        trace.is_macro_cluster(&parent.name);
        return macro_cluster_tilings(parent, ctx, trace);
    }"#,
        replace: r#"    if false {
        trace.is_macro_cluster(&parent.name);
        return macro_cluster_tilings(parent, ctx, trace);
    }"#,
        want: r#"a_hard_macro_cluster_gets_the_tilings_of_its_macro_count"#,
    },
    Mutation {
        name: r#"boundary-height-tested-first"#,
        file: r#"src/regions.rs"#,
        find: r#"    if region.x_max - region.x_min == 0 {"#,
        replace: r#"    if region.y_max - region.y_min == 0 && false {"#,
        want: r#"width_is_tested_before_height"#,
    },
    Mutation {
        name: r#"boundary-left-and-right-swapped"#,
        file: r#"src/regions.rs"#,
        find: r#"        return if region.x_min == die.x_min { Boundary::L } else { Boundary::R };"#,
        replace: r#"        return if region.x_min == die.x_min { Boundary::R } else { Boundary::L };"#,
        want: r#"a_zero_width_region_is_left_only_at_the_left_edge"#,
    },
    Mutation {
        name: r#"boundary-bottom-and-top-swapped"#,
        file: r#"src/regions.rs"#,
        find: r#"    if region.y_min == die.y_min {
        Boundary::B
    } else {
        Boundary::T
    }"#,
        replace: r#"    if region.y_min == die.y_min {
        Boundary::T
    } else {
        Boundary::B
    }"#,
        want: r#"a_zero_height_region_is_bottom_only_at_the_bottom_edge"#,
    },
    Mutation {
        name: r#"boundary-rect-left-collapses-to-the-far-side"#,
        file: r#"src/regions.rs"#,
        find: r#"        Boundary::L => r.x_max = die.x_min,"#,
        replace: r#"        Boundary::L => r.x_min = die.x_max,"#,
        want: r#"each_boundary_rect_is_the_whole_edge_collapsed_to_a_line"#,
    },
    Mutation {
        name: r#"boundary-rect-top-uses-the-bottom"#,
        file: r#"src/regions.rs"#,
        find: r#"        Boundary::T => r.y_min = die.y_max,"#,
        replace: r#"        Boundary::T => r.y_max = die.y_min,"#,
        want: r#"each_boundary_rect_is_the_whole_edge_collapsed_to_a_line"#,
    },
    Mutation {
        name: r#"subtract-cuts-the-wrong-axis"#,
        file: r#"src/regions.rs"#,
        find: r#"    if boundary.is_vertical() {
        a.y_max = overlay.y_min;
        b.y_min = overlay.y_max;
    } else {
        a.x_max = overlay.x_min;
        b.x_min = overlay.x_max;
    }"#,
        replace: r#"    if boundary.is_vertical() {
        a.x_max = overlay.x_min;
        b.x_min = overlay.x_max;
    } else {
        a.y_max = overlay.y_min;
        b.y_min = overlay.y_max;
    }"#,
        want: r#"a_horizontal_edge_is_cut_along_x_and_a_vertical_one_along_y"#,
    },
    Mutation {
        name: r#"subtract-drops-every-line"#,
        file: r#"src/regions.rs"#,
        find: r#"        if piece.x_max - piece.x_min != 0 || piece.y_max - piece.y_min != 0 {"#,
        replace: r#"        if piece.x_max - piece.x_min != 0 && piece.y_max - piece.y_min != 0 {"#,
        want: r#"a_zero_width_piece_is_kept_because_the_test_is_an_OR"#,
    },
    Mutation {
        name: r#"subtract-across-boundaries-allowed"#,
        file: r#"src/regions.rs"#,
        find: r#"    if boundary != boundary_of(die, overlay) {
        return None;
    }"#,
        replace: r#"    if false {
        return None;
    }"#,
        want: r#"subtracting_across_boundaries_is_refused"#,
    },
    Mutation {
        name: r#"available-boundary-order-changed"#,
        file: r#"src/regions.rs"#,
        find: r#"pub const BOUNDARY_ORDER: [Boundary; 4] = [Boundary::B, Boundary::L, Boundary::T, Boundary::R];"#,
        replace: r#"pub const BOUNDARY_ORDER: [Boundary; 4] = [Boundary::L, Boundary::B, Boundary::T, Boundary::R];"#,
        want: r#"with_nothing_blocked_every_edge_is_available_in_enum_order"#,
    },
    Mutation {
        name: r#"available-subtracts-on-overlap"#,
        file: r#"src/regions.rs"#,
        find: r#"                match contains(region, block).then(|| subtract_overlap(die, region, block)) {"#,
        replace: r#"                match true.then(|| subtract_overlap(die, region, block)) {"#,
        want: r#"a_region_that_only_OVERLAPS_is_left_alone"#,
    },
    Mutation {
        name: r#"contains-is-strict"#,
        file: r#"src/regions.rs"#,
        find: r#"    outer.x_min <= inner.x_min"#,
        replace: r#"    outer.x_min < inner.x_min"#,
        want: r#"a_blocked_region_flush_with_the_edge_start_is_still_contained"#,
    },
    Mutation {
        name: r#"depth-max-uses-the-min-proportion"#,
        file: r#"src/shaping.rs"#,
        find: r#"        x_max: (0.10_f32 * dx as f32) as i64,"#,
        replace: r#"        x_max: (0.04_f32 * dx as f32) as i64,"#,
        want: r#"the_depth_limits_are_ten_and_four_percent_of_the_die"#,
    },
    Mutation {
        name: r#"depth-y-computed-from-x"#,
        file: r#"src/shaping.rs"#,
        find: r#"        y_max: (0.10_f32 * dy as f32) as i64,"#,
        replace: r#"        y_max: (0.10_f32 * dx as f32) as i64,"#,
        want: r#"the_limits_are_per_axis_and_a_square_die_hides_that"#,
    },
    Mutation {
        name: r#"tight-override-is-per-axis"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if tiling_min_width < limits.x_min && tiling_min_height < limits.y_min {"#,
        replace: r#"    if tiling_min_width < limits.x_min || tiling_min_height < limits.y_min {"#,
        want: r#"a_design_tight_in_ONE_direction_keeps_both_proportional_minima"#,
    },
    Mutation {
        name: r#"tight-override-never-fires"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if tiling_min_width < limits.x_min && tiling_min_height < limits.y_min {"#,
        replace: r#"    if false {"#,
        want: r#"a_design_tight_in_BOTH_directions_replaces_BOTH_minima"#,
    },
    Mutation {
        name: r#"tiling-margin-not-halved"#,
        file: r#"src/shaping.rs"#,
        find: r#"    let tiling_min_width = (dx - root_tiling.width) / 2;"#,
        replace: r#"    let tiling_min_width = dx - root_tiling.width;"#,
        want: r#"a_design_tight_in_BOTH_directions_replaces_BOTH_minima"#,
    },
    Mutation {
        name: r#"mixed-area-always-added"#,
        file: r#"src/shaping.rs"#,
        find: r#"    let std_cell_area =
        if std_cell_area_of_children == 0 { mixed_area_of_children } else { std_cell_area_of_children };"#,
        replace: r#"    let std_cell_area = std_cell_area_of_children + mixed_area_of_children;"#,
        want: r#"the_mixed_children_are_used_ONLY_when_there_are_no_std_cell_ones"#,
    },
    Mutation {
        name: r#"macro-dominance-not-squared"#,
        file: r#"src/shaping.rs"#,
        find: r#"    Ok((std_cell_area as f64 / io_span as f64 * (1.0 - macro_dominance_factor).powi(2)) as i64)"#,
        replace: r#"    Ok((std_cell_area as f64 / io_span as f64 * (1.0 - macro_dominance_factor)) as i64)"#,
        want: r#"macro_dominance_is_SQUARED_so_it_bites_hard"#,
    },
    Mutation {
        name: r#"zero-root-area-divided-by"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if root_area == 0 {
        return Err(RootAreaIsZero);
    }"#,
        replace: r#"    if false {
        return Err(RootAreaIsZero);
    }"#,
        want: r#"a_root_of_zero_area_is_refused_rather_than_divided_by"#,
    },
    Mutation {
        name: r#"region-length-halved"#,
        file: r#"src/regions.rs"#,
        find: r#"    (line.x_max - line.x_min) + (line.y_max - line.y_min)
}"#,
        replace: r#"    ((line.x_max - line.x_min) + (line.y_max - line.y_min)) / 2
}"#,
        want: r#"a_regions_length_is_the_side_that_is_not_zero"#,
    },
    Mutation {
        name: r#"density-factor-has-no-base"#,
        file: r#"src/regions.rs"#,
        find: r#"    1.0 + (ios_here as f32 / ios_total as f32)"#,
        replace: r#"    ios_here as f32 / ios_total as f32"#,
        want: r#"a_region_with_none_of_the_ios_still_gets_its_base_depth"#,
    },
    Mutation {
        name: r#"density-factor-inverted"#,
        file: r#"src/regions.rs"#,
        find: r#"    1.0 + (ios_here as f32 / ios_total as f32)"#,
        replace: r#"    1.0 + (ios_total as f32 / ios_here as f32)"#,
        want: r#"a_region_with_none_of_the_ios_still_gets_its_base_depth"#,
    },
    Mutation {
        name: r#"scale-depth-rounds"#,
        file: r#"src/regions.rs"#,
        find: r#"    (base_depth as f32 * factor) as i64"#,
        replace: r#"    (base_depth as f32 * factor).round() as i64"#,
        want: r#"scaling_a_depth_truncates"#,
    },
    Mutation {
        name: r#"clamp-axes-swapped"#,
        file: r#"src/regions.rs"#,
        find: r#"    let (min, max) = if boundary.is_vertical() {
        (limits.x_min, limits.x_max)
    } else {
        (limits.y_min, limits.y_max)
    };"#,
        replace: r#"    let (min, max) = if boundary.is_vertical() {
        (limits.y_min, limits.y_max)
    } else {
        (limits.x_min, limits.x_max)
    };"#,
        want: r#"a_vertical_boundary_is_clamped_by_the_X_limits"#,
    },
    Mutation {
        name: r#"clamp-max-is-inclusive"#,
        file: r#"src/regions.rs"#,
        find: r#"    if depth > max {
        max
    } else if depth < min {"#,
        replace: r#"    if depth >= max {
        max
    } else if depth <= min {"#,
        want: r#"the_clamp_is_an_else_if_and_the_ORDER_of_the_two_tests_shows"#,
    },
    Mutation {
        name: r#"clamp-tests-are-independent"#,
        file: r#"src/regions.rs"#,
        find: r#"    if depth > max {
        max
    } else if depth < min {
        min
    } else {
        depth
    }"#,
        replace: r#"    let mut d = depth;
    if d < min {
        d = min;
    }
    if d > max {
        d = max;
    }
    d"#,
        want: r#"the_clamp_is_an_else_if_and_the_ORDER_of_the_two_tests_shows"#,
    },
    Mutation {
        name: r#"blockage-left-grows-outward"#,
        file: r#"src/regions.rs"#,
        find: r#"        Boundary::L => r.x_max = r.x_min + d,"#,
        replace: r#"        Boundary::L => r.x_min = r.x_min - d,"#,
        want: r#"a_blockage_grows_INWARD_from_its_edge"#,
    },
    Mutation {
        name: r#"blockage-top-grows-outward"#,
        file: r#"src/regions.rs"#,
        find: r#"        Boundary::T => r.y_min = r.y_max - d,"#,
        replace: r#"        Boundary::T => r.y_max = r.y_max + d,"#,
        want: r#"a_blockage_grows_INWARD_from_its_edge"#,
    },
    Mutation {
        name: r#"blockage-not-clamped"#,
        file: r#"src/regions.rs"#,
        find: r#"    let d = clamp_depth(depth, region.boundary, limits);"#,
        replace: r#"    let d = depth;"#,
        want: r#"a_blockage_is_clamped_before_it_is_drawn"#,
    },
    Mutation {
        name: r#"span-is-per-region"#,
        file: r#"src/regions.rs"#,
        find: r#"    let span: i64 = regions.iter().map(|r| region_length(&r.region.line)).sum();
    let base = base_depth_for_span(span);"#,
        replace: r#"    let base = 0;"#,
        want: r#"the_span_is_summed_over_ALL_regions_and_the_base_depth_computed_once"#,
    },
    Mutation {
        name: r#"density-not-applied-per-region"#,
        file: r#"src/regions.rs"#,
        find: r#"            let depth = scale_depth(base, io_density_factor(r.ios, ios_total));"#,
        replace: r#"            let depth = base;"#,
        want: r#"a_region_with_more_ios_gets_a_deeper_blockage"#,
    },
    Mutation {
        name: r#"empty-regions-not-short-circuited"#,
        file: r#"src/regions.rs"#,
        find: r#"    if regions.is_empty() {
        return Vec::new();
    }"#,
        replace: r#"    if false {
        return Vec::new();
    }"#,
        want: r#"no_regions_means_the_base_depth_is_never_COMPUTED"#,
    },
    Mutation {
        name: r#"available-regions-get-a-density-factor"#,
        file: r#"src/regions.rs"#,
        find: r#"    available.iter().map(|r| pin_access_blockage(r, base, limits)).collect()"#,
        replace: r#"    available
        .iter()
        .map(|r| pin_access_blockage(r, region_length(&r.line), limits))
        .collect()"#,
        want: r#"available_regions_all_get_the_SAME_depth"#,
    },
    Mutation {
        name: r#"available-guard-on-the-wrong-list"#,
        file: r#"src/regions.rs"#,
        find: r#"    if !any_blocked {
        return Vec::new();
    }"#,
        replace: r#"    if available.is_empty() {
        return Vec::new();
    }"#,
        want: r#"with_nothing_blocked_no_available_region_casts_a_blockage"#,
    },
    Mutation {
        name: r#"placement-blockages-deduplicated"#,
        file: r#"src/shaping.rs"#,
        find: r#"pub fn placement_blockages(blockages: &[Rect]) -> Vec<Rect> {
    blockages.to_vec()
}"#,
        replace: r#"pub fn placement_blockages(blockages: &[Rect]) -> Vec<Rect> {
    blockages.iter().take(1).copied().collect()
}"#,
        want: r#"placement_blockages_are_taken_as_they_stand"#,
    },
    Mutation {
        name: r#"only-macros-does-not-return"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if input.has_only_macros {
        root.cluster_type = ClusterType::HardMacro;"#,
        replace: r#"    if false {
        root.cluster_type = ClusterType::HardMacro;"#,
        want: r#"a_design_of_only_macros_stops_after_the_root_shape"#,
    },
    Mutation {
        name: r#"only-macros-does-not-retype"#,
        file: r#"src/shaping.rs"#,
        find: r#"        root.cluster_type = ClusterType::HardMacro;
        return Ok(CoarseShaping {"#,
        replace: r#"        return Ok(CoarseShaping {"#,
        want: r#"a_design_of_only_macros_stops_after_the_root_shape"#,
    },
    Mutation {
        name: r#"io-pads-still-get-blockages"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if input.has_io_pads || input.top_std_cell_area == 0 {"#,
        replace: r#"    if input.top_std_cell_area == 0 {"#,
        want: r#"a_design_with_io_pads_casts_no_pin_access_blockages"#,
    },
    Mutation {
        name: r#"zero-std-cell-area-still-gets-blockages"#,
        file: r#"src/shaping.rs"#,
        find: r#"    if input.has_io_pads || input.top_std_cell_area == 0 {"#,
        replace: r#"    if input.has_io_pads {"#,
        want: r#"a_design_with_no_standard_cells_casts_no_pin_access_blockages"#,
    },
    Mutation {
        name: r#"builders-run-in-the-wrong-order"#,
        file: r#"src/shaping.rs"#,
        find: r#"    let mut out = crate::regions::blockages_for_regions_traced(
        input.io_bundles,
        input.fixed_ios,"#,
        replace: r#"    let mut out = crate::regions::blockages_for_regions_traced(
        input.constrained_regions,
        input.fixed_ios,"#,
        want: r#"the_three_builders_append_in_upstreams_order"#,
    },
    Mutation {
        name: r#"root-shape-from-the-die-not-the-floorplan"#,
        file: r#"src/shaping.rs"#,
        find: r#"    let root_shape = root_shape(&input.floorplan);"#,
        replace: r#"    let root_shape = root_shape(&input.die);"#,
        want: r#"the_root_takes_the_FLOORPLAN_shape_not_the_die"#,
    },
    Mutation {
        name: r#"placement-blockages-dropped"#,
        file: r#"src/shaping.rs"#,
        find: r#"        placement_blockages: placement_blockages(input.blockages),"#,
        replace: r#"        placement_blockages: Vec::new(),"#,
        want: r#"the_placement_blockages_come_through_untouched"#,
    },
];
