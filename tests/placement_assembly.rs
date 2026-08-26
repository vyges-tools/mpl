// SPDX-License-Identifier: Apache-2.0
//! Assembling one parent's placement problem: which macro gets which id, and what falls outside
//! the sequence pair.

use vyges_mpl::anneal::SoftMacro;
use vyges_mpl::placement::{assemble, AreaKind, AssemblyChild};

fn sm(x: i32, y: i32, w: i32, h: i32) -> SoftMacro {
    SoftMacro { x, y, width: w, height: h, fixed: false, area: w as i64 * h as i64, is_macro_cluster: false }
}

fn child(name: &str, kind: AreaKind) -> AssemblyChild {
    AssemblyChild { name: name.into(), kind, macro_: sm(0, 0, 10, 10), fence: None, guide: None }
}

const OUTLINE: (i32, i32, i32, i32) = (1000, 1000, 3000, 3000);

/// 🔑 **Blockages take the LOWEST ids**, so every cluster's id is offset by their count. The
/// sequence pair, the nets and the blockage list all index into this one list.
#[test]
fn blockages_come_first_and_offset_every_cluster() {
    let blockages = [sm(0, 0, 50, 50), sm(100, 100, 50, 50)];
    let children = [child("A", AreaKind::HardMacroCluster), child("B", AreaKind::MixedCluster)];
    let got = assemble(&blockages, &children, OUTLINE, &[]);

    assert_eq!(got.id("A"), Some(2), "two blockages ahead of it");
    assert_eq!(got.id("B"), Some(3));
    assert_eq!(got.macros.len(), 4);
}

/// ⛔ **An IO cluster is DEFERRED, not dropped** — it is appended after every cluster being
/// placed, which is what puts it outside the sequence pair.
#[test]
fn io_clusters_are_appended_after_the_placeable_clusters() {
    let children = [
        child("io_1", AreaKind::IoCluster),
        child("A", AreaKind::HardMacroCluster),
        child("io_2", AreaKind::IoCluster),
        child("B", AreaKind::StdCellCluster),
    ];
    let got = assemble(&[], &children, OUTLINE, &[]);

    assert_eq!(got.id("A"), Some(0), "the first PLACEABLE cluster, though it is second in order");
    assert_eq!(got.id("B"), Some(1));
    assert_eq!(got.id("io_1"), Some(2), "deferred, and in the order it was skipped");
    assert_eq!(got.id("io_2"), Some(3));
}

/// ⛔ **The sequence-pair count is taken BEFORE the IO clusters and terminals.** Everything past it
/// is in the macro list but immovable.
#[test]
fn the_sequence_pair_stops_before_the_io_clusters() {
    let blockages = [sm(0, 0, 50, 50)];
    let children = [
        child("A", AreaKind::HardMacroCluster),
        child("io_1", AreaKind::IoCluster),
    ];
    let terminals = [("sibling".to_string(), sm(500, 500, 0, 0))];
    let got = assemble(&blockages, &children, OUTLINE, &terminals);

    assert_eq!(got.number_of_sequence_pair_macros, 2, "the blockage and the cluster");
    assert_eq!(got.macros.len(), 4, "plus the IO cluster and the terminal");
    assert_eq!(got.id("io_1"), Some(2));
    assert_eq!(got.id("sibling"), Some(3));
}

/// ⚠️ **A blockage IS in the sequence pair.** It is fixed, so the packer pins it — but it is inside
/// the range the annealer permutes, which is why a blockage displaces its neighbours.
#[test]
fn a_blockage_is_inside_the_sequence_pair() {
    let blockages = [sm(0, 0, 50, 50)];
    let got = assemble(&blockages, &[], OUTLINE, &[]);
    assert_eq!(got.number_of_sequence_pair_macros, 1);
}

/// ⚠️ **A fixed macro cluster still takes an id** — the name is recorded before the branch that
/// skips building a cluster-backed soft macro for it.
#[test]
fn a_fixed_macro_cluster_still_takes_an_id() {
    let children = [child("FIXED", AreaKind::FixedMacro), child("A", AreaKind::MixedCluster)];
    let got = assemble(&[], &children, OUTLINE, &[]);
    assert_eq!(got.id("FIXED"), Some(0));
    assert_eq!(got.id("A"), Some(1), "and it did not take the fixed one's place");
}

// ---------------------------------------------------------------- fences and guides

/// 🔑 A fence is clipped to the outline and rebased onto it, and keyed by the macro's own id.
#[test]
fn a_fence_is_clipped_rebased_and_keyed_by_id() {
    let mut a = child("A", AreaKind::HardMacroCluster);
    // Half inside the 1000..3000 outline.
    a.fence = Some((0, 0, 2000, 2000));
    let got = assemble(&[sm(0, 0, 5, 5)], &[a], OUTLINE, &[]);

    assert_eq!(got.fences, vec![(1usize, (0, 0, 1000, 1000))], "clipped, then rebased on the outline");
    assert!(got.guides.is_empty());
}

/// ⚠️ A fence wholly outside the outline is dropped rather than recorded as empty.
#[test]
fn a_fence_outside_the_outline_is_dropped() {
    let mut a = child("A", AreaKind::HardMacroCluster);
    a.fence = Some((0, 0, 500, 500));
    let got = assemble(&[], &[a], OUTLINE, &[]);
    assert!(got.fences.is_empty());
}

/// ⛔ **A standard-cell cluster gets no fence and no guide**, whatever is declared for it.
#[test]
fn a_std_cell_cluster_gets_no_fence_or_guide() {
    let mut a = child("CELLS", AreaKind::StdCellCluster);
    a.fence = Some((0, 0, 2000, 2000));
    a.guide = Some((0, 0, 2000, 2000));
    let got = assemble(&[], &[a], OUTLINE, &[]);
    assert_eq!(got.id("CELLS"), Some(0), "it is still placed");
    assert!(got.fences.is_empty(), "but never fenced");
    assert!(got.guides.is_empty());
}

/// ⛔ **A fixed macro cluster takes the same exit, one branch earlier.**
#[test]
fn a_fixed_macro_cluster_gets_no_fence_or_guide() {
    let mut a = child("FIXED", AreaKind::FixedMacro);
    a.fence = Some((0, 0, 2000, 2000));
    a.guide = Some((0, 0, 2000, 2000));
    let got = assemble(&[], &[a], OUTLINE, &[]);
    assert!(got.fences.is_empty());
    assert!(got.guides.is_empty());
}

/// ⚠️ Guides follow the same path as fences and are kept separate from them.
#[test]
fn guides_are_recorded_separately_from_fences() {
    let mut a = child("A", AreaKind::MixedCluster);
    a.guide = Some((1500, 1500, 2500, 2500));
    let got = assemble(&[], &[a], OUTLINE, &[]);
    assert_eq!(got.guides, vec![(0usize, (500, 500, 1500, 1500))]);
    assert!(got.fences.is_empty());
}

/// ⚠️ **Blockages get ids but no NAMES** — they are addressable only by position.
#[test]
fn a_blockage_has_no_name() {
    let got = assemble(&[sm(0, 0, 50, 50)], &[child("A", AreaKind::MixedCluster)], OUTLINE, &[]);
    assert_eq!(got.id_of.len(), 1, "only the cluster is named");
    assert_eq!(got.id("A"), Some(1));
}

/// ⚠️ **The last binding wins**, because upstream assigns into a `std::map`. ℹ️ Nothing in the
/// suite repeats a name; this pins the map's rule, not a situation known to arise.
#[test]
fn a_repeated_name_keeps_the_last_id() {
    let children = [child("A", AreaKind::MixedCluster)];
    let terminals = [("A".to_string(), sm(500, 500, 0, 0))];
    let got = assemble(&[], &children, OUTLINE, &terminals);
    assert_eq!(got.id("A"), Some(1), "the terminal's id, not the cluster's");
}

/// ℹ️ A parent with nothing under it assembles to nothing, rather than to a one-macro problem.
#[test]
fn an_empty_parent_assembles_to_an_empty_problem() {
    let got = assemble(&[], &[], OUTLINE, &[]);
    assert!(got.macros.is_empty());
    assert_eq!(got.number_of_sequence_pair_macros, 0);
}
