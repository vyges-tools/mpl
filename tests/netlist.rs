// SPDX-License-Identifier: Apache-2.0
//! Net connectivity. Rules from upstream `isValidNet` / `buildNet` / `connectClusters`.
use vyges_mpl::design::{Design, Instance, MasterKind, Module, Rect};
use vyges_mpl::netlist::{
    build_connections, build_net, connections_for, is_valid_net, Connections, DbNet, InstTerm,
    Net, PortTerm,
};

fn inst(name: &str) -> Instance {
    Instance {
        name: name.into(),
        is_block: false,
        is_fixed: false,
        bbox: Rect { x_min: 0, y_min: 0, x_max: 10, y_max: 10 },
        master: MasterKind::default(),
        is_ignorable_macro: false,
    }
}

fn design(instances: Vec<Instance>) -> Design {
    let n = instances.len();
    Design {
        instances,
        modules: vec![Module {
            name: "top".into(),
            hierarchical_name: "top".into(),
            insts: (0..n).collect(),
            children: vec![],
        }],
        top: 0,
        core_area: Rect { x_min: 0, y_min: 0, x_max: 1000, y_max: 1000 },
        die_area: Rect { x_min: 0, y_min: 0, x_max: 1000, y_max: 1000 },
    }
}

fn net(is_supply: bool, iterms: Vec<(usize, bool)>, bterms: Vec<(usize, bool)>) -> DbNet {
    DbNet {
        name: "n".into(),
        is_supply,
        iterms: iterms.into_iter().map(|(inst, is_output)| InstTerm { inst, is_output }).collect(),
        bterms: bterms.into_iter().map(|(bterm, is_input)| PortTerm { bterm, is_input }).collect(),
    }
}

/// Identity mapping: instance i lives in cluster i.
fn ident(i: usize) -> Option<i32> {
    Some(i as i32)
}

// ------------------------------------------------------------------ is_valid_net

#[test]
fn a_supply_net_is_never_valid() {
    let d = design(vec![inst("a"), inst("b")]);
    assert!(!is_valid_net(&net(true, vec![(0, true), (1, false)], vec![]), &d));
}

#[test]
fn a_net_touching_only_ignored_instances_is_not_valid() {
    // ⚠️ The condition that is easy to miss. A net between two tapcells is not connectivity the
    // placer can act on, and counting it would pull unrelated clusters together.
    let mut a = inst("tap1");
    a.master.is_end_cap = true;
    let mut b = inst("tap2");
    b.master.is_pad = true;
    let d = design(vec![a, b]);
    assert!(!is_valid_net(&net(false, vec![(0, true), (1, false)], vec![]), &d));
}

#[test]
fn one_unignored_instance_is_enough_to_make_a_net_valid() {
    let mut a = inst("tap");
    a.master.is_end_cap = true;
    let d = design(vec![a, inst("real")]);
    assert!(is_valid_net(&net(false, vec![(0, true), (1, false)], vec![]), &d));
}

#[test]
fn a_net_with_only_ports_is_not_valid() {
    // ℹ️ Upstream requires at least one un-ignored INSTANCE terminal; ports alone are not enough.
    let d = design(vec![inst("a")]);
    assert!(!is_valid_net(&net(false, vec![], vec![(0, true)]), &d));
}

// ------------------------------------------------------------------ build_net

#[test]
fn an_output_pin_is_the_driver_and_the_rest_are_loads() {
    let n = net(false, vec![(0, true), (1, false), (2, false)], vec![]);
    let b = build_net(&n, &ident, &ident, false);
    assert_eq!(b, Net { driver: Some(0), loads: vec![1, 2] });
}

#[test]
fn the_LAST_output_wins_on_a_multiply_driven_net() {
    // ⚠️ Upstream assigns on every output terminal it meets rather than erroring, so the last
    // one stands. Reproduced rather than "improved" -- a divergence here would be invisible.
    let n = net(false, vec![(0, true), (1, true), (2, false)], vec![]);
    assert_eq!(build_net(&n, &ident, &ident, false).driver, Some(1));
}

#[test]
fn a_net_with_no_output_pin_has_no_driver() {
    let n = net(false, vec![(0, false), (1, false)], vec![]);
    assert_eq!(build_net(&n, &ident, &ident, false).driver, None);
}

#[test]
fn a_block_INPUT_port_is_the_driver_the_inverse_of_the_instance_rule() {
    // 🔑 The trap. An input PORT drives signal into the design, so it is the driver -- the
    // opposite convention to an instance pin. Reversing it silently flips connectivity on every
    // port net and nothing about the resulting tree looks wrong.
    let n = net(false, vec![(1, false)], vec![(7, true)]);
    let b = build_net(&n, &ident, &|bt| Some(100 + bt as i32), false);
    assert_eq!(b.driver, Some(107), "the input port drives");
    assert_eq!(b.loads, vec![1]);
}

#[test]
fn a_block_OUTPUT_port_is_a_load() {
    let n = net(false, vec![(1, true)], vec![(7, false)]);
    let b = build_net(&n, &ident, &|bt| Some(100 + bt as i32), false);
    assert_eq!(b.driver, Some(1));
    assert_eq!(b.loads, vec![107], "the output port is a load");
}

#[test]
fn ports_are_ignored_entirely_when_the_design_has_io_pads() {
    // ⚠️ With pads present the pads carry the connectivity; reading ports too double-counts it.
    let n = net(false, vec![(1, true)], vec![(7, false)]);
    let with_pads = build_net(&n, &ident, &|bt| Some(100 + bt as i32), true);
    assert_eq!(with_pads.loads, Vec::<i32>::new(), "the port contributed nothing");
    let without = build_net(&n, &ident, &|bt| Some(100 + bt as i32), false);
    assert_eq!(without.loads, vec![107], "and it does when there are no pads");
}

// ------------------------------------------------------------------ connections_for

#[test]
fn a_net_with_no_driver_or_no_loads_contributes_nothing() {
    assert!(connections_for(&Net { driver: None, loads: vec![1] }, 50).is_empty());
    assert!(connections_for(&Net { driver: Some(0), loads: vec![] }, 50).is_empty());
}

#[test]
fn a_large_net_is_dropped_at_the_threshold_not_past_it() {
    // 🔑 `>=`, not `>`. A net with exactly the threshold many loads is ALREADY too large.
    let three = Net { driver: Some(0), loads: vec![1, 2, 3] };
    assert!(connections_for(&three, 3).is_empty(), "3 loads with threshold 3 is dropped");
    assert_eq!(connections_for(&three, 4).len(), 3, "threshold 4 keeps it");
}

#[test]
fn a_load_in_the_drivers_own_cluster_is_skipped() {
    // A cluster is not connected to itself.
    let n = Net { driver: Some(5), loads: vec![5, 6, 5] };
    assert_eq!(connections_for(&n, 50), vec![(5, 6)]);
}

#[test]
fn duplicate_loads_are_NOT_deduplicated() {
    // ⚠️ Two pins of one cluster on a net weigh twice. Deduplicating would quietly halve the
    // connectivity between exactly the clusters most worth merging.
    let n = Net { driver: Some(0), loads: vec![1, 1, 1] };
    assert_eq!(connections_for(&n, 50).len(), 3);
}

// ------------------------------------------------------------------ Connections

#[test]
fn a_connection_is_recorded_on_both_clusters() {
    // 🔑 Symmetric. A one-sided map would make the merge stage's answer depend on which cluster
    // it happened to ask.
    let mut c = Connections::new();
    c.connect(1, 2, 1.0);
    assert_eq!(c.weight(1, 2), 1.0);
    assert_eq!(c.weight(2, 1), 1.0);
}

#[test]
fn weights_accumulate_across_nets() {
    let mut c = Connections::new();
    c.connect(1, 2, 1.0);
    c.connect(1, 2, 1.0);
    assert_eq!(c.weight(1, 2), 2.0, "a pair joined by two nets weighs two");
}

#[test]
fn an_unconnected_pair_weighs_nothing() {
    let c = Connections::new();
    assert_eq!(c.weight(1, 2), 0.0);
    assert!(c.of(1).is_empty());
}

#[test]
fn clearing_removes_everything() {
    // ⚠️ Rebuilt every merge round: ids change as clusters merge, so a stale map would connect
    // clusters that no longer exist.
    let mut c = Connections::new();
    c.connect(1, 2, 1.0);
    c.clear();
    assert!(c.is_empty());
}

// ------------------------------------------------------------------ end to end

#[test]
fn the_whole_map_skips_supply_and_ignored_nets() {
    let mut tap = inst("tap");
    tap.master.is_end_cap = true;
    let d = design(vec![inst("a"), inst("b"), tap]);
    let nets = vec![
        net(true, vec![(0, true), (1, false)], vec![]),   // supply -- skipped
        net(false, vec![(2, true), (2, false)], vec![]),  // all ignored -- skipped
        net(false, vec![(0, true), (1, false)], vec![]),  // the only real one
    ];
    let c = build_connections(&nets, &d, &ident, &ident, false, 50);
    assert_eq!(c.weight(0, 1), 1.0);
    assert_eq!(c.of(0).len(), 1, "exactly one connection was recorded");
}
