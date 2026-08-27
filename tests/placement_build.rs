// SPDX-License-Identifier: Apache-2.0
//! Building one parent's placement problem from a real cluster tree.

use vyges_mpl::anneal::SoftWeights;
use vyges_mpl::cluster::{Cluster, ClusterType};
use vyges_mpl::placement::{
    area_kind_of, build_parent_problem, AreaKind, ParentContext, Root, VIRTUAL_CONNECTION_WEIGHT,
};
use vyges_mpl::shaping::Tiling;

const OUTLINE: (i32, i32, i32, i32) = (1000, 1000, 3000, 3000);

fn ctx<'a>(
    connections: &'a dyn Fn(i32) -> Vec<(i32, f32)>,
    virtual_connections: &'a [(i32, i32)],
) -> ParentContext<'a> {
    ParentContext {
        connections_of: connections,
        virtual_connections,
        blockages: &[],
        soft_blockages: &[],
        fence_of: &|_| None,
        guide_of: &|_| None,
        terminals: &[],
        root: Root { x: 0, y: 0, width: 4000, height: 4000 },
        die_margin: 16_000,
        available_regions: &[],
        constraint_region_of: &|_| None,
        weights: SoftWeights::placement_defaults(),
        dbu_per_micron: 2000,
        tiny_threshold: 0,
        min_ar: 0.33,
    }
}

fn macro_child(id: i32, name: &str, w: i64, h: i64) -> Cluster {
    let mut c = Cluster::new(id, name);
    c.cluster_type = ClusterType::HardMacro;
    c.metrics.num_macro = 1;
    c.metrics.macro_area = w * h;
    c.tilings = vec![Tiling { width: w, height: h }];
    c
}

/// ⛔ **The classification order is upstream's and is not interchangeable.** An IO cluster is asked
/// about FIRST, and a FIXED macro second — ahead of its type, which is `HardMacroCluster` and would
/// otherwise swallow it.
#[test]
fn the_classification_order_is_not_interchangeable() {
    let mut io = Cluster::new(1, "io");
    io.set_as_io_bundle((0, 0), 0, 100);
    io.cluster_type = ClusterType::HardMacro;
    io.is_fixed_macro = true;
    assert_eq!(area_kind_of(&io), AreaKind::IoCluster, "IO wins over both");

    let mut fixed = Cluster::new(2, "MACRO_1");
    fixed.set_as_fixed_macro((0, 0, 100, 100));
    fixed.cluster_type = ClusterType::HardMacro;
    assert_eq!(area_kind_of(&fixed), AreaKind::FixedMacro, "fixed wins over the type");

    let mut plain = Cluster::new(3, "MACRO_2");
    plain.cluster_type = ClusterType::HardMacro;
    assert_eq!(area_kind_of(&plain), AreaKind::HardMacroCluster);
}

/// ⛔ **A FIXED macro is CLIPPED to the outline and rebased**; its own clustering-time soft macro is
/// neither, and both survive.
#[test]
fn a_fixed_macro_is_clipped_and_rebased_into_the_problem() {
    let mut parent = Cluster::new(0, "root");
    let mut fixed = Cluster::new(1, "MACRO_1");
    // Straddles the outline's left edge: 900..1900 against an outline starting at 1000.
    fixed.set_as_fixed_macro((900, 1500, 1900, 2500));
    parent.children.push(fixed);

    let conn = |_: i32| Vec::new();
    let got = build_parent_problem(&parent, OUTLINE, &ctx(&conn, &[]));
    let m = got.macros[0];
    assert_eq!((m.x, m.y), (0, 500), "rebased onto the outline");
    assert_eq!((m.width, m.height), (900, 1000), "and clipped to it");
    assert!(m.fixed);

    // The cluster's own soft macro is untouched — absolute and unclipped.
    let own = parent.children[0].soft_macro.unwrap();
    assert_eq!((own.x, own.width), (900, 1000));
}

/// ⚠️ **An IO cluster is rebased but NOT clipped**, and its area stays ZERO — it is a region rather
/// than an occupant.
#[test]
fn an_io_cluster_is_rebased_but_not_clipped() {
    let mut parent = Cluster::new(0, "root");
    let mut io = Cluster::new(1, "L_0");
    io.set_as_io_bundle((0, 1500), 0, 4000);
    parent.children.push(io);

    let conn = |_: i32| Vec::new();
    let got = build_parent_problem(&parent, OUTLINE, &ctx(&conn, &[]));
    let m = got.macros[0];
    assert_eq!((m.x, m.y), (-1000, 500), "rebased, and outside the outline");
    assert_eq!(m.height, 4000, "not clipped to the outline's 2000");
    assert_eq!(m.area, 0);
}

/// ⚠️ An ordinary cluster starts from its FIRST tiling.
#[test]
fn an_ordinary_cluster_starts_from_its_first_tiling() {
    let mut parent = Cluster::new(0, "root");
    let mut m = macro_child(1, "MACRO_1", 200, 400);
    m.tilings.push(Tiling { width: 400, height: 200 });
    parent.children.push(m);

    let conn = |_: i32| Vec::new();
    let got = build_parent_problem(&parent, OUTLINE, &ctx(&conn, &[]));
    assert_eq!((got.macros[0].width, got.macros[0].height), (200, 400));
    assert!(got.macros[0].is_macro_cluster);
}

/// ⚠️ **Virtual connections come FIRST and carry weight ten**, then each child's own — and only
/// where the child's id is strictly greater, which halves the undirected pairs.
#[test]
fn virtual_connections_lead_and_the_pairs_are_halved() {
    let mut parent = Cluster::new(0, "root");
    parent.children.push(macro_child(1, "A", 100, 100));
    parent.children.push(macro_child(2, "B", 100, 100));

    // Both directions are offered; only 2 -> 1 survives.
    let conn = |id: i32| match id {
        1 => vec![(2, 4.0)],
        2 => vec![(1, 4.0)],
        _ => Vec::new(),
    };
    let got = build_parent_problem(&parent, OUTLINE, &ctx(&conn, &[(1, 2)]));

    assert_eq!(got.inputs.nets.len(), 2, "one virtual, one real");
    assert_eq!(got.inputs.nets[0].weight, VIRTUAL_CONNECTION_WEIGHT, "the virtual one leads");
    assert_eq!(got.inputs.nets[1].weight, 4.0);
    assert_eq!((got.inputs.nets[1].source, got.inputs.nets[1].target), (1, 0), "2 -> 1, not both");
}

/// ⚠️ The fixed macros' boxes reach the fixed-macro penalty, and nothing else does.
#[test]
fn only_fixed_macros_reach_the_fixed_penalty() {
    let mut parent = Cluster::new(0, "root");
    let mut fixed = Cluster::new(1, "MACRO_1");
    fixed.set_as_fixed_macro((1100, 1100, 1300, 1300));
    parent.children.push(fixed);
    parent.children.push(macro_child(2, "MACRO_2", 100, 100));

    let conn = |_: i32| Vec::new();
    let got = build_parent_problem(&parent, OUTLINE, &ctx(&conn, &[]));
    assert_eq!(got.fixed_bboxes, vec![(100, 100, 300, 300)], "rebased, and only the fixed one");
}

/// ⚠️ The attributes carry what the cost terms read, per macro id.
#[test]
fn the_attributes_follow_the_macro_ids() {
    let mut parent = Cluster::new(0, "root");
    let mut m = macro_child(1, "MACRO_1", 200, 200);
    m.metrics.num_macro = 3;
    parent.children.push(m);
    let mut cells = Cluster::new(2, "cells");
    cells.cluster_type = ClusterType::StdCell;
    cells.metrics.std_cell_area = 50_000;
    parent.children.push(cells);

    let conn = |_: i32| Vec::new();
    let got = build_parent_problem(&parent, OUTLINE, &ctx(&conn, &[]));
    assert_eq!(got.inputs.attributes[0].num_macro, 3);
    assert_eq!(got.inputs.attributes[0].kind, Some(AreaKind::HardMacroCluster));
    assert_eq!(got.inputs.attributes[1].num_macro, 0, "a cell cluster holds none");
    assert_eq!(got.inputs.attributes[1].cluster_area, 50_000);
}

/// ⚠️ The outline is passed as a SIZE, not as a rectangle — the origin goes to the inputs.
#[test]
fn the_outline_is_split_into_a_size_and_an_origin() {
    let parent = Cluster::new(0, "root");
    let conn = |_: i32| Vec::new();
    let got = build_parent_problem(&parent, OUTLINE, &ctx(&conn, &[]));
    assert_eq!(got.outline, (2000, 2000));
    assert_eq!(got.inputs.outline_origin, (1000, 1000));
}

/// ⛔ **A CONSTRAINED IO cluster measures wirelength against its OWN region.** Only an
/// *unconstrained* one uses the die-wide available regions, and the two paths are separate: with
/// the constraint region missing the distance falls to zero and the whole net scores nothing.
///
/// 🔑 The context keys this by CLUSTER id, like the fence and guide beside it, and the translation
/// to the assembled macro index happens inside — a caller cannot predict that index, because it
/// depends on how many blockages came first and on where the IO clusters were deferred to.
#[test]
fn a_constrained_io_clusters_region_reaches_the_problem_by_cluster_id() {
    let region = vyges_mpl::placement::Region {
        x0: 3000, y0: 1010, x1: 3000, y1: 1030,
        boundary: vyges_mpl::halo::Boundary::R,
    };

    let mut parent = Cluster::new(1, "parent");
    parent.cluster_type = ClusterType::Mixed;
    let mut io = Cluster::new(7, "ios_1");
    io.set_as_cluster_of_unplaced_io_pins((3000, 1010), 0, 20, false);
    parent.children.push(io);
    parent.children.push(macro_child(8, "MACRO_1", 200, 200));

    let no_conn: &dyn Fn(i32) -> Vec<(i32, f32)> = &|_| Vec::new();
    let mut c = ctx(no_conn, &[]);
    let by_id: &dyn Fn(i32) -> Option<vyges_mpl::placement::Region> =
        &|id| (id == 7).then_some(region);
    c.constraint_region_of = by_id;
    let problem = build_parent_problem(&parent, OUTLINE, &c);

    assert_eq!(problem.inputs.constraint_regions.len(), 1, "the region reached the problem");
    let (macro_id, got) = problem.inputs.constraint_regions[0];
    assert_eq!(got, region);
    // 🔑 **The index is the point.** The IO cluster is the parent's FIRST child but is deferred to
    // the END of the macro list, so its child position (0) and its macro id (1) differ — keying by
    // the child's position would put the region on the macro cluster instead.
    assert_eq!(macro_id, 1, "the ASSEMBLED index, not the child position");
    assert_eq!(
        (problem.macros[macro_id].width, problem.macros[macro_id].height),
        (0, 20),
        "and that index is the IO cluster's own macro, which is a LINE"
    );

    // ⚠️ The control: without the lookup the list is empty, so the assertion above is about the
    // cluster-id translation and not about the list being filled unconditionally.
    let empty = build_parent_problem(&parent, OUTLINE, &ctx(no_conn, &[]));
    assert!(empty.inputs.constraint_regions.is_empty());
}
