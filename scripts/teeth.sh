#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# ⛔ A test that cannot FAIL proves nothing. Two of pad's first order probes were inert when
# written and nobody noticed until the runner was taught to check. This is that check: each
# entry breaks ONE rule and names the test that must notice.
#
# Three distinct outcomes, deliberately -- they mean different things:
#   caught        the named test failed. The rule is pinned.
#   WRONG TEST    the suite went red but not where predicted. The rule is covered by SOMETHING,
#                 and our belief about which test covers it was wrong. Fix the expectation.
#   NOT CAUGHT    the suite stayed green. A real hole.
#
# Usage:  bash scripts/teeth.sh
set -uo pipefail
cd "$(dirname "$0")/.."

# ⛔ NEVER run two of these at once. It mutates the working tree in place, so a second instance
# sees the first one's mutation and reports a failure in an unrelated file. That produced exactly
# one confusing WRONG TEST on 2026-08-24 -- a halo mutation blamed for a cluster-tree failure --
# and the result did not reproduce, which is the worst kind of finding to leave unexplained.
LOCK=".teeth.lock"
if ! mkdir "$LOCK" 2>/dev/null; then
  echo "ERROR: another teeth.sh is running (remove $LOCK if that is stale)." >&2
  exit 2
fi
cleanup() {
  rmdir "$LOCK" 2>/dev/null
  # Restore anything a kill left mutated, so an interrupted run never leaves a broken tree.
  for b in src/*.teeth-backup tests/*.teeth-backup; do
    [ -e "$b" ] || continue
    mv "$b" "${b%.teeth-backup}"
  done
}
trap cleanup EXIT INT TERM

SEP=$'\x1f'   # unit separator: cannot occur in Rust source, unlike | or ,

mutations() {
  # name <SEP> file <SEP> find <SEP> replace <SEP> test-that-must-fail
  #
  # ⛔ NO DOUBLE QUOTES IN A PATTERN. Each row is one bash double-quoted string, so an
  # embedded " ends it and the script fails to parse. Anchor on a quote-free fragment
  # instead -- it only has to be unique in the file, not complete. Cost an hour twice.
  printf '%s\n' \
"halo-order-lrbt${SEP}src/options.rs${SEP}4 => (values[0], values[1], values[2], values[3]),${SEP}4 => (values[0], values[2], values[1], values[3]),${SEP}a_four_value_halo_is_left_bottom_right_top" \
"halo-two-value-not-mirrored${SEP}src/options.rs${SEP}2 => (values[0], values[1], values[0], values[1]),${SEP}2 => (values[0], values[1], 0, 0),${SEP}a_two_value_halo_mirrors_into_four" \
"negative-halo-allowed${SEP}src/options.rs${SEP}if v < 0 {${SEP}if false {${SEP}a_negative_halo_value_is_mpl73" \
"both-blockage-weights-allowed${SEP}src/options.rs${SEP}if saw_macro_blockage_weight && saw_soft_blockage_weight {${SEP}if false {${SEP}giving_both_blockage_weights_is_mpl69" \
"macro-blockage-does-not-alias${SEP}src/options.rs${SEP}                saw_macro_blockage_weight = true;\n                warnings.push(MplWarning {\n                    code: 70,${SEP}                saw_macro_blockage_weight = true;\n                warnings.push(MplWarning {\n                    code: 700,${SEP}macro_blockage_weight_aliases_soft_and_warns_mpl70" \
"macro-blockage-weight-not-applied${SEP}src/options.rs${SEP}                });\n                o.soft_blockage_weight = num(value)?;${SEP}                });\n                let _ = num(value)?;${SEP}macro_blockage_weight_aliases_soft_and_warns_mpl70" \
"halo-width-alone-loses-height${SEP}src/options.rs${SEP}let h = halo_height.or(halo_width).unwrap_or(0);${SEP}let h = halo_height.unwrap_or(0);${SEP}halo_width_alone_sets_height_to_it_and_warns_mpl74" \
"halo-height-alone-loses-width${SEP}src/options.rs${SEP}let w = halo_width.or(halo_height).unwrap_or(0);${SEP}let w = halo_width.unwrap_or(0);${SEP}halo_height_alone_sets_width_to_it" \
"mpl74-warning-dropped${SEP}src/options.rs${SEP}code: 74,${SEP}code: 0,${SEP}halo_width_alone_sets_height_to_it_and_warns_mpl74" \
"target-util-default-wrong${SEP}src/options.rs${SEP}target_util: 0.25,${SEP}target_util: 0.30,${SEP}every_default_matches_upstreams_tcl" \
"report-dir-default-wrong${SEP}src/options.rs${SEP}report_directory: \"hier_rtlmp\".to_string(),${SEP}report_directory: \"mpl\".to_string(),${SEP}every_default_matches_upstreams_tcl" \
"region-inversion-unchecked${SEP}src/options.rs${SEP}if x1 > x2 {${SEP}if false {${SEP}a_region_is_four_values_and_must_not_be_inverted" \
"vacuous-reads-as-applied${SEP}src/status.rs${SEP}if placed == 0 {${SEP}if false {${SEP}placing_nothing_is_never_applied" \
"refusal-does-not-outrank${SEP}src/status.rs${SEP}if refusal.is_some() {${SEP}if false {${SEP}a_refusal_outranks_the_count" \
"stop-after-uses-first-occurrence${SEP}src/pipeline.rs${SEP}seq.iter().rposition(${SEP}seq.iter().position(${SEP}repeat_duplicates_in_place_and_composes_with_stop_after" \
"only-reorders-as-asked${SEP}src/pipeline.rs${SEP}ORDER.iter().copied().filter(|s| only.contains(s)).collect()${SEP}only.clone()${SEP}only_keeps_upstreams_relative_order_not_the_order_asked_for" \
"boundary-order-l-after-r${SEP}src/halo.rs${SEP}    B = 0,\n    L = 1,\n    T = 2,\n    R = 3,${SEP}    B = 0,\n    L = 3,\n    T = 2,\n    R = 1,${SEP}equidistant_same_direction_falls_back_to_the_enum_order" \
"boundary-order-b-after-t${SEP}src/halo.rs${SEP}    B = 0,\n    L = 1,\n    T = 2,\n    R = 3,${SEP}    B = 2,\n    L = 1,\n    T = 0,\n    R = 3,${SEP}a_centred_pin_prefers_bottom_over_top" \
"is-vertical-inverted${SEP}src/halo.rs${SEP}        matches!(self, Boundary::L | Boundary::R)${SEP}        matches!(self, Boundary::B | Boundary::T)${SEP}a_corner_pin_is_decided_by_its_layer_direction" \
"corner-rule-layer-dir-swapped${SEP}src/halo.rs${SEP}        LayerDir::Vertical => {\n            if first.1.is_vertical() { second.1 } else { first.1 }\n        }${SEP}        LayerDir::Vertical => {\n            if first.1.is_vertical() { first.1 } else { second.1 }\n        }${SEP}a_corner_pin_is_decided_by_its_layer_direction" \
"right-distance-uses-x-min${SEP}src/halo.rs${SEP}        (master_width - pin.x_max, Boundary::R),${SEP}        (pin.x_min, Boundary::R),${SEP}right_and_bottom_distances_use_the_master_extents" \
"soft-halo-floored-to-base${SEP}src/halo.rs${SEP}        Some((h, true)) => h,${SEP}        Some((h, true)) => Halo { left: h.left.max(base.left), bottom: h.bottom.max(base.bottom), right: h.right.max(base.right), top: h.top.max(base.top) },${SEP}a_soft_instance_halo_is_taken_as_is_and_is_not_floored" \
"unfixed-macro-reoriented${SEP}src/halo.rs${SEP}    if is_fixed {\n        return reorient_for_fixed(halo, orient);\n    }${SEP}    return reorient_for_fixed(halo, orient);\n    #[allow(unreachable_code)]${SEP}an_unfixed_macro_is_not_reoriented" \
"mx-swaps-left-right${SEP}src/halo.rs${SEP}    if matches!(orient, Orient::Mx | Orient::R180) {\n        std::mem::swap(&mut h.bottom, &mut h.top);${SEP}    if matches!(orient, Orient::Mx | Orient::R180) {\n        std::mem::swap(&mut h.left, &mut h.right);${SEP}a_fixed_macro_has_its_halo_reoriented" \
"minimum-spacing-ignored${SEP}src/halo.rs${SEP}    let mut halo = Halo {\n        left: minimum_spacing,${SEP}    let mut halo = Halo {\n        left: 0,${SEP}no_pins_at_all_leaves_the_minimum_spacing_on_every_side" \
"explicit-halo-not-short-circuited${SEP}src/halo.rs${SEP}    if let Some(h) = explicit {\n        return h;\n    }\n\n    let full = full_halo(None, inst_halo, base);${SEP}    let full = full_halo(explicit, inst_halo, base);${SEP}an_explicit_halo_bypasses_use_full_halo_and_reorientation" \
"level-reset-outside-derivation${SEP}src/thresholds.rs${SEP}        if metrics.num_macro <= MIN_NUM_MACROS_FOR_MULTILEVEL {\n            max_level = 1;\n        }${SEP}    }\n    if metrics.num_macro <= MIN_NUM_MACROS_FOR_MULTILEVEL {\n        max_level = 1;\n    }\n    if false {${SEP}keep_clustering_data2_matches_upstreams_reported_thresholds" \
"threshold-guard-is-per-field${SEP}src/thresholds.rs${SEP}    if t.max_macro <= 0 || t.min_macro <= 0 || t.max_std_cell <= 0 || t.min_std_cell <= 0 {${SEP}    if t.max_macro <= 0 && t.min_macro <= 0 && t.max_std_cell <= 0 && t.min_std_cell <= 0 {${SEP}a_partially_supplied_threshold_set_derives_all_four" \
"std-cell-floor-not-applied${SEP}src/thresholds.rs${SEP}        t.min_std_cell = t.min_std_cell.max(MIN_NUM_STD_CELLS_ALLOWED);${SEP}        t.min_std_cell = t.min_std_cell.max(1);${SEP}halos1_matches_upstreams_reported_thresholds" \
"macro-floor-uses-std-cell-constant${SEP}src/thresholds.rs${SEP}        if t.min_macro <= 0 {\n            t.min_macro = 1;\n        }${SEP}        if t.min_macro <= 0 {\n            t.min_macro = MIN_NUM_STD_CELLS_ALLOWED;\n        }${SEP}the_std_cell_minimum_is_floored_at_1000_and_the_macro_minimum_at_1" \
"coarsening-uses-max-level-not-minus-one${SEP}src/thresholds.rs${SEP}        let f = (cluster_size_ratio as f64).powi(max_level - 1);${SEP}        let f = (cluster_size_ratio as f64).powi(max_level);${SEP}keep_clustering_data2_matches_upstreams_reported_thresholds" \
"fixed-macros-do-not-force-one-level${SEP}src/thresholds.rs${SEP}    if has_fixed_macros {\n        max_level = 1;\n    }${SEP}    if false {\n        max_level = 1;\n    }${SEP}a_fixed_macro_forces_a_single_level" \
"per-level-std-floor-is-1000${SEP}src/thresholds.rs${SEP}        t.min_std_cell = 100;${SEP}        t.min_std_cell = 1000;${SEP}a_degenerate_std_cell_minimum_becomes_one_hundred_not_one_thousand" \
"per-level-max-not-recomputed${SEP}src/thresholds.rs${SEP}    if t.min_macro <= 0 {\n        t.min_macro = 1;\n        t.max_macro = half_ratio(t.min_macro, cluster_size_ratio);\n    }${SEP}    if t.min_macro <= 0 {\n        t.min_macro = 1;\n    }${SEP}a_degenerate_macro_minimum_becomes_one_and_recomputes_its_maximum" \
"half-ratio-rounds-instead-of-truncating${SEP}src/thresholds.rs${SEP}    trunc((base as f32 * ratio) as f64 / 2.0)${SEP}    (((base as f32 * ratio) as f64 / 2.0).round()) as i32${SEP}an_odd_ratio_truncates_the_half_rather_than_rounding_it" \
"level-divides-repeatedly${SEP}src/thresholds.rs${SEP}    let coarse_factor = (cluster_size_ratio as f64).powi(level - 1);${SEP}    let coarse_factor = (cluster_size_ratio as f64) * (level - 1).max(1) as f64;${SEP}each_level_divides_by_the_ratio" \
"break-uses-and-not-or${SEP}src/cluster.rs${SEP}    cluster.num_std_cell() > max_std_cell || cluster.num_macro() > max_macro${SEP}    cluster.num_std_cell() > max_std_cell && cluster.num_macro() > max_macro${SEP}breaking_needs_either_count_over_its_maximum" \
"merge-uses-or-not-and${SEP}src/cluster.rs${SEP}        && cluster.num_std_cell() < min_std_cell\n        && cluster.num_macro() < min_macro${SEP}        && (cluster.num_std_cell() < min_std_cell\n        || cluster.num_macro() < min_macro)${SEP}merging_needs_both_counts_under_their_minimum" \
"io-clusters-can-merge${SEP}src/cluster.rs${SEP}    !cluster.is_io_cluster()\n        && cluster.num_std_cell()${SEP}    true\n        && cluster.num_std_cell()${SEP}an_io_cluster_is_never_a_merge_candidate" \
"break-is-greater-or-equal${SEP}src/cluster.rs${SEP}    cluster.num_std_cell() > max_std_cell || cluster.num_macro() > max_macro${SEP}    cluster.num_std_cell() >= max_std_cell || cluster.num_macro() >= max_macro${SEP}breaking_needs_either_count_over_its_maximum" \
"hard-macro-mask-missing${SEP}src/cluster.rs${SEP}        if self.cluster_type == ClusterType::HardMacro {\n            return 0;\n        }${SEP}        if false {\n            return 0;\n        }${SEP}a_hard_macro_cluster_reports_no_standard_cells" \
"std-cell-mask-missing${SEP}src/cluster.rs${SEP}        if self.cluster_type == ClusterType::StdCell {\n            return 0;\n        }${SEP}        if false {\n            return 0;\n        }${SEP}a_std_cell_cluster_reports_no_macros" \
"logical-module-allows-glue${SEP}src/cluster.rs${SEP}        self.leaf_std_cells.is_empty() && self.leaf_macros.is_empty() && self.db_modules.len() == 1${SEP}        self.db_modules.len() == 1${SEP}glue_instances_stop_a_cluster_corresponding_to_a_logical_module" \
"logical-module-at-least-one${SEP}src/cluster.rs${SEP}&& self.db_modules.len() == 1\n    }${SEP}&& !self.db_modules.is_empty()\n    }${SEP}two_modules_stop_it_too" \
"subtree-collapse-is-lifo${SEP}src/cluster.rs${SEP}        if cluster.children.is_empty() {\n            leaves.push(cluster);${SEP}        if cluster.children.is_empty() {\n            leaves.insert(0, cluster);${SEP}the_collapse_is_breadth_first_not_depth_first" \
"subtree-pushes-to-front${SEP}src/cluster.rs${SEP}                wavefront.push_back(child);${SEP}                wavefront.push_front(child);${SEP}the_collapse_is_breadth_first_not_depth_first" \
"dissolved-ids-not-reported${SEP}src/cluster.rs${SEP}            dissolved.push(cluster.id);${SEP}            let _ = cluster.id;${SEP}every_dissolved_id_is_reported_so_the_id_map_can_be_pruned" \
"par-gate-uses-masked-counts${SEP}src/cluster.rs${SEP}        && (cluster.leaf_std_cells.len() as i64 > max_std_cell as i64\n            || cluster.leaf_macros.len() as i64 > max_macro as i64)${SEP}        && (cluster.num_std_cell() as i64 > max_std_cell as i64\n            || cluster.num_macro() as i64 > max_macro as i64)${SEP}the_par_gate_counts_leaf_vectors_not_the_masked_metrics" \
"par-gate-ignores-modules${SEP}src/cluster.rs${SEP}    cluster.db_modules.is_empty()\n        && (cluster.leaf_std_cells.len()${SEP}    true\n        && (cluster.leaf_std_cells.len()${SEP}a_large_flat_cluster_is_one_with_no_modules_and_too_many_leaves" \
"par-refusal-not-collected${SEP}src/cluster.rs${SEP}        .filter(|c| is_large_flat_cluster(c, max_std_cell, max_macro))${SEP}        .filter(|_c| false)${SEP}a_resulting_child_needing_the_partitioner_is_reported_not_approximated" \
"blockage-cleanup-runs-forwards${SEP}src/apply.rs${SEP}    (baseline.blockages..now).rev().collect()${SEP}    (baseline.blockages..now).collect()${SEP}destruction_runs_highest_index_first" \
"blockage-cleanup-endpoints-swapped${SEP}src/apply.rs${SEP}    (baseline.blockages..now).rev().collect()${SEP}    (now..baseline.blockages).rev().collect()${SEP}a_shrunken_count_asks_for_nothing_rather_than_producing_garbage_indices" \
"refusal-commits${SEP}src/apply.rs${SEP}    if kept {\n        Settlement::Committed\n    } else {\n        Settlement::RolledBack\n    }${SEP}    let _ = kept;\n    Settlement::Committed${SEP}only_success_commits" \
"overlap-includes-touching${SEP}src/design.rs${SEP}        self.x_min < other.x_max\n            && other.x_min < self.x_max${SEP}        self.x_min <= other.x_max\n            && other.x_min <= self.x_max${SEP}touching_rectangles_do_not_overlap" \
"ignore-check-before-block${SEP}src/design.rs${SEP}        if inst.is_block {\n            m.num_macro += 1;${SEP}        if inst.is_block && !is_ignored_inst(inst) {\n            m.num_macro += 1;${SEP}an_ignorable_macro_STILL_counts_as_a_macro" \
"cover-not-exempt-from-fixed-error${SEP}src/design.rs${SEP}        } else if inst.is_fixed\n            && !inst.master.is_cover${SEP}        } else if inst.is_fixed\n            && true${SEP}a_fixed_cover_cell_inside_the_area_is_allowed" \
"fixed-error-ignores-the-area${SEP}src/design.rs${SEP}            && inst.bbox.overlaps(placement_area)${SEP}            && true${SEP}a_fixed_cell_outside_the_placement_area_is_allowed_and_counted" \
"ignored-std-cells-counted${SEP}src/design.rs${SEP}        } else if !is_ignored_inst(inst) {${SEP}        } else if true {${SEP}an_ignored_standard_cell_counts_as_neither" \
"ignorable-flag-applies-to-std-cells${SEP}src/design.rs${SEP}    if inst.is_block && inst.is_ignorable_macro {${SEP}    if inst.is_ignorable_macro {${SEP}the_ignorable_flag_only_applies_to_macros" \
"metrics-do-not-recurse${SEP}src/design.rs${SEP}    for &child in &design.modules[module].children {${SEP}    for &child in &[] as &[usize] {${SEP}metrics_accumulate_through_the_module_hierarchy" \
"fence-outside-core-falls-back${SEP}src/design.rs${SEP}    if shape.area() == 0 {\n        None${SEP}    if false {\n        None${SEP}a_fence_outside_the_core_leaves_nothing_to_place_into" \
"unfixed-macros-includes-fixed${SEP}src/design.rs${SEP}        .filter(|(_, i)| i.is_block && !i.is_fixed)${SEP}        .filter(|(_, i)| i.is_block)${SEP}only_unfixed_macros_are_the_placers_to_move" \
"empty-module-still-clustered${SEP}src/tree.rs${SEP}        if self.module_metrics[module].num_macro == 0\n            && self.module_metrics[module].num_std_cell == 0\n        {\n            return None;\n        }${SEP}        if false {\n            return None;\n        }${SEP}a_module_with_no_instances_gets_no_cluster" \
"empty-module-skip-uses-or${SEP}src/tree.rs${SEP}        if self.module_metrics[module].num_macro == 0\n            && self.module_metrics[module].num_std_cell == 0${SEP}        if self.module_metrics[module].num_macro == 0\n            || self.module_metrics[module].num_std_cell == 0${SEP}a_module_with_only_macros_still_gets_a_cluster" \
"cluster-named-by-leaf-name${SEP}src/tree.rs${SEP}self.design.modules[module].hierarchical_name.clone()${SEP}self.design.modules[module].name.clone()${SEP}a_cluster_is_named_by_the_modules_hierarchical_name" \
"empty-glue-cluster-kept${SEP}src/tree.rs${SEP}        if c.leaf_std_cells.is_empty() && c.leaf_macros.is_empty() {\n            return None;\n        }${SEP}        if false {\n            return None;\n        }${SEP}a_glue_cluster_with_no_leaves_is_DISCARDED" \
"glue-name-without-parens${SEP}src/tree.rs${SEP}({parent_name})_glue_logic${SEP}{parent_name}_glue_logic${SEP}glue_logic_is_named_after_its_parent_in_parentheses" \
"glue-ignores-the-ignore-check${SEP}src/tree.rs${SEP}            if is_ignored_inst(inst) {\n                continue;\n            }${SEP}            if false {\n                continue;\n            }${SEP}a_module_of_only_ignored_cells_produces_no_glue_cluster" \
"glue-files-macros-as-std-cells${SEP}src/tree.rs${SEP}            if inst.is_block {\n                cluster.leaf_macros.push(i);\n            } else {\n                cluster.leaf_std_cells.push(i);\n            }${SEP}            cluster.leaf_std_cells.push(i);${SEP}glue_leaves_are_filed_by_whether_they_are_macros" \
"metrics-skip-held-modules${SEP}src/tree.rs${SEP}        for &module in &cluster.db_modules {\n            m.num_std_cell += self.module_metrics[module].num_std_cell;${SEP}        for &module in &[] as &[usize] {\n            m.num_std_cell += self.module_metrics[module].num_std_cell;${SEP}cluster_metrics_count_both_leaves_and_held_modules" \
"macro-clusters-not-typed-hard${SEP}src/tree.rs${SEP}            c.cluster_type = ClusterType::HardMacro;${SEP}            c.cluster_type = ClusterType::Mixed;${SEP}each_macro_becomes_its_own_hard_macro_cluster" \
"macro-per-cluster-includes-ignored${SEP}src/tree.rs${SEP}            if is_ignored_inst(inst) || !inst.is_block {${SEP}            if !inst.is_block {${SEP}an_ignored_macro_does_not_become_its_own_cluster" \
"id-not-advanced${SEP}src/tree.rs${SEP}        let id = self.next_id;\n        self.next_id += 1;\n        id${SEP}        self.next_id${SEP}ids_are_handed_out_in_creation_order" \
"a-stage-dropped-from-order${SEP}src/pipeline.rs${SEP}    StageId::ComputeWireLength,\n];${SEP}];${SEP}the_pipeline_matches_the_spec_table"
}

# ⚠️ `mv` restores the BACKUP's mtime, which can be older than the artifact built from the
# mutated source -- cargo then decides nothing changed and keeps the MUTATED binary, so the next
# mutation is measured against the previous one. Found by this script's own post-run green check
# on 2026-08-24, which is the only reason it was not silently wrong for the whole run.
restore() { mv "$1.teeth-backup" "$1"; touch "$1"; }

# Which cargo target holds a given test name? Integration tests live in tests/<name>.rs; unit
# tests live in the lib. Returns the narrowest --test/--lib flag, or nothing if it cannot tell
# (in which case the full suite runs, which is the safe default).
target_flag() {
  local want="$1" f
  for f in tests/*.rs; do
    [ -e "$f" ] || continue
    if grep -qE "fn +${want}\\(" "$f"; then
      printf -- "--test %s" "$(basename "$f" .rs)"; return
    fi
  done
  if grep -rqE "fn +${want}\\(" src/ 2>/dev/null; then printf -- "--lib"; fi
}

caught=0; wrong=0; hole=0; stale=0; total=0
while IFS="$SEP" read -r name file find replace want; do
  total=$((total+1))
  cp "$file" "$file.teeth-backup"
  FIND="$find" REPLACE="$replace" perl -0pi -e '
     my $f = $ENV{FIND}; my $r = $ENV{REPLACE};
     $f =~ s/\\n/\n/g; $r =~ s/\\n/\n/g;
     my $i = index($_, $f);
     substr($_, $i, length($f)) = $r if $i >= 0;
  ' "$file"

  if cmp -s "$file" "$file.teeth-backup"; then
    printf '  %-34s \033[33mSTALE PATTERN\033[0m (mutation did not apply)\n' "$name"
    stale=$((stale+1)); restore "$file"; continue
  fi

  # ⚡ Fast path: build and run ONLY the target holding the expected test. Most mutations are
  # caught, and this skips linking the other test binaries. If it is NOT caught here we fall
  # through to the full suite, because distinguishing WRONG TEST from NOT CAUGHT needs to see
  # every test -- and getting that distinction wrong is worse than being slow.
  out=$(cargo test --offline $(target_flag "$want") 2>&1)
  if ! echo "$out" | grep -qE "^test .*\b${want}\b.* FAILED"; then
    out=$(cargo test --offline 2>&1)
  fi
  if echo "$out" | grep -qE "^test .*\b${want}\b.* FAILED"; then
    printf '  %-34s \033[32mcaught\033[0m by %s\n' "$name" "$want"
    caught=$((caught+1))
  elif echo "$out" | grep -qE "^error(\[|:)|test result: FAILED"; then
    other=$(echo "$out" | grep -E "^test .* FAILED" | sed 's/^test //;s/ \.\.\..*//' | paste -sd, - | cut -c1-60)
    printf '  %-34s \033[33mWRONG TEST\033[0m expected %s, red: %s\n' "$name" "$want" "${other:-compile error}"
    wrong=$((wrong+1))
  else
    printf '  %-34s \033[31mNOT CAUGHT\033[0m -- suite stayed green\n' "$name"
    hole=$((hole+1))
  fi
  restore "$file"
done < <(mutations)

echo
echo "teeth: $caught caught, $wrong wrong-test, $hole holes, $stale stale, of $total"
cargo test --offline >/dev/null 2>&1 || { echo "ERROR: suite not green after restore"; exit 2; }
[ $((hole + stale + wrong)) -eq 0 ] || exit 1
