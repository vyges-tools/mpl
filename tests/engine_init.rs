// SPDX-License-Identifier: Apache-2.0
//! `init`'s ORDER — which test a design meets first decides whether it is refused, skipped or
//! placed. Rules from the reference's `ClusteringEngine::init`.
use vyges_mpl::design::{Design, Instance, MasterKind, Module, Rect};
use vyges_mpl::engine::{run_clustering, ClusterOptions, DesignInputs};
use vyges_mpl::messages::Message;
use vyges_mpl::status::Status;

fn r(x0: i64, y0: i64, x1: i64, y1: i64) -> Rect {
    Rect { x_min: x0, y_min: y0, x_max: x1, y_max: y1 }
}

fn inst(name: &str, is_block: bool, is_fixed: bool, bbox: Rect) -> Instance {
    Instance {
        master_id: 0,
        name: name.into(),
        is_block,
        is_fixed,
        bbox,
        master: MasterKind::default(),
        is_ignorable_macro: false,
    }
}

/// ⛔ **All macros fixed, plus a fixed standard cell inside the core: SKIPPED, not refused.**
/// The reference returns on "No unfixed macros" (MPL-17) before `computeModuleMetrics` can raise
/// MPL-50 on the cell — `no_unfixed_macros_and_fixed_cells` in its suite. With the check after the
/// metrics, as it used to be, this design is refused.
#[test]
fn a_fixed_cell_does_not_refuse_a_design_with_nothing_to_place() {
    let d = Design {
        instances: vec![
            inst("U1", true, true, r(100, 100, 300, 300)),
            inst("fixed_buf", false, true, r(20, 20, 30, 30)),
        ],
        modules: vec![Module {
            name: "top".into(),
            hierarchical_name: "top".into(),
            insts: vec![0, 1],
            children: vec![],
        }],
        top: 0,
        core_area: r(0, 0, 1000, 1000),
        die_area: r(0, 0, 1000, 1000),
    };
    // No macro geometry: a fixed macro inside the area still has its halo resolved, but with
    // none the resolver skips it, which is all this test needs.
    let geometry = vec![None, None];
    let input = DesignInputs {
        design: &d,
        pins: &[],
        nets: &[],
        geometry: &geometry,
        blockages: &[],
        blocked_regions_for_pins: &[],
        minimum_spacing: 0,
        manufacturing_grid: 0,
        dbu_per_micron: 1000,
    };
    let c = run_clustering(&input, &ClusterOptions::default());
    assert_eq!(c.status, Status::Vacuous, "refusal: {:?}", c.refusal);
    assert!(c.refusal.is_none());
    // Rule (createHardMacros, init): each fixed block is reported as found, then MPL-17.
    assert_eq!(c.messages, [Message::Info(62, "Found fixed macro U1.".into()), Message::Info(17, "No unfixed macros.".into())]);
}

/// ⛔ **The shaping handoff counts EVERY block instance**, because the tiny-cluster threshold is
/// a thousandth of the reference's `block_->getInsts().size()`. It was a placeholder `0`: every
/// upstream suite design has under 1000 instances and scores the same either way, so only a
/// generated witness of 7609 showed it — a 5-cell and a 7-cell cluster the reference collapsed to
/// a unit square were shaped instead.
#[test]
fn the_shaping_handoff_carries_the_block_instance_count() {
    let mut instances = vec![inst("U1", true, false, r(0, 0, 200, 200))];
    for i in 0..1500 {
        instances.push(inst(&format!("c{i}"), false, false, r(0, 0, 10, 10)));
    }
    let n = instances.len();
    let d = Design {
        instances,
        modules: vec![Module {
            name: "top".into(),
            hierarchical_name: "top".into(),
            insts: (0..n).collect(),
            children: vec![],
        }],
        top: 0,
        core_area: r(0, 0, 10_000, 10_000),
        die_area: r(0, 0, 10_000, 10_000),
    };
    let mut geometry: Vec<Option<vyges_mpl::read::MacroGeometry>> = vec![None; n];
    geometry[0] = Some(vyges_mpl::read::MacroGeometry {
        master_width: 200,
        master_height: 200,
        pins: vec![],
        inst_halo: None,
        orient: vyges_mpl::halo::Orient::R0,
    });
    let input = DesignInputs {
        design: &d,
        pins: &[],
        nets: &[],
        geometry: &geometry,
        blockages: &[],
        blocked_regions_for_pins: &[],
        minimum_spacing: 0,
        manufacturing_grid: 0,
        dbu_per_micron: 1000,
    };
    let c = run_clustering(&input, &ClusterOptions::default());
    let h = c.shaping.unwrap_or_else(|| panic!("no handoff: {:?} {:?}", c.status, c.refusal));
    assert_eq!(h.block_instance_count, 1501, "macros count too, not just the std cells");
    // Rule (init, then createIOClusters): the design report, then MPL-26 for a design without
    // ports; no MPL-25, since it has standard cells.
    assert!(matches!(&c.messages[..], [Message::Report(t), Message::Warn(26, _)] if t.starts_with("Die Area: ")), "{:?}", c.messages);
    assert_eq!(vyges_mpl::placement::tiny_cluster_max_number_of_std_cells(h.block_instance_count), 1);
}
