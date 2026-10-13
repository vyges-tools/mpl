// SPDX-License-Identifier: Apache-2.0
//! `init`'s ORDER — which test a design meets first decides whether it is refused, skipped or
//! placed. Rules from the reference's `ClusteringEngine::init`.
use vyges_mpl::design::{Design, Instance, MasterKind, Module, Rect};
use vyges_mpl::engine::{run_clustering, ClusterOptions, DesignInputs};
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
    };
    let c = run_clustering(&input, &ClusterOptions::default());
    assert_eq!(c.status, Status::Vacuous, "refusal: {:?}", c.refusal);
    assert!(c.refusal.is_none());
}
