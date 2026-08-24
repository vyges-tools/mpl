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

SEP=$'\x1f'   # unit separator: cannot occur in Rust source, unlike | or ,

mutations() {
  # name <SEP> file <SEP> find <SEP> replace <SEP> test-that-must-fail
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
"a-stage-dropped-from-order${SEP}src/pipeline.rs${SEP}    StageId::ComputeWireLength,\n];${SEP}];${SEP}the_pipeline_matches_the_spec_table"
}

# ⚠️ `mv` restores the BACKUP's mtime, which can be older than the artifact built from the
# mutated source -- cargo then decides nothing changed and keeps the MUTATED binary, so the next
# mutation is measured against the previous one. Found by this script's own post-run green check
# on 2026-08-24, which is the only reason it was not silently wrong for the whole run.
restore() { mv "$1.teeth-backup" "$1"; touch "$1"; }

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

  out=$(cargo test --offline 2>&1)
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
