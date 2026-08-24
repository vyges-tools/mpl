// SPDX-License-Identifier: Apache-2.0
//! Net connectivity between clusters — what the merge stage decides on.
//!
//! Upstream: `ClusteringEngine::isValidNet`, `buildNet`, `connectClusters`, `connect`,
//! and `Cluster::addConnection`.
//!
//! 🔑 **A net becomes a WEIGHT between two clusters, not a wire.** Everything downstream — which
//! small clusters merge, and later the annealer's wirelength — reads that weight, so the rules
//! that decide which nets count are as load-bearing as the geometry.

use crate::cluster::ClusterId;
use crate::design::{is_ignored_inst, Design};
use std::collections::BTreeMap;

/// A pin on an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstTerm {
    pub inst: usize,
    /// ⚠️ `OUTPUT` makes this the DRIVER. Anything else is a load — upstream tests only for
    /// output, so an inout counts as a load.
    pub is_output: bool,
}

/// A pin on a block port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortTerm {
    pub bterm: usize,
    /// 🔑 **INPUT makes this the driver — the inverse of the instance rule.** A block input port
    /// drives signal *into* the design. Getting this backwards silently reverses connectivity on
    /// every port net, and nothing about the resulting tree looks wrong.
    pub is_input: bool,
}

/// A net as read from the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbNet {
    pub name: String,
    pub is_supply: bool,
    pub iterms: Vec<InstTerm>,
    pub bterms: Vec<PortTerm>,
}

/// A net reduced to cluster ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    /// `None` when nothing drove it.
    pub driver: Option<ClusterId>,
    pub loads: Vec<ClusterId>,
}

/// Is this net worth reading at all?
///
/// ⚠️ **Two conditions, and the second is easy to miss.** A supply net is skipped; so is a net
/// whose every instance pin sits on an **ignored** instance. A net between two tapcells is not
/// connectivity the placer can act on, and counting it would pull unrelated clusters together.
///
/// ℹ️ A net with only block ports and no instance pins is **not** valid — upstream requires at
/// least one un-ignored instance terminal.
pub fn is_valid_net(net: &DbNet, design: &Design) -> bool {
    if net.is_supply {
        return false;
    }
    net.iterms
        .iter()
        .any(|t| !is_ignored_inst(&design.instances[t.inst]))
}

/// Reduce a net to the clusters it touches.
///
/// ⚠️ **The last output wins.** Upstream assigns `driver_id` on every output terminal it meets, so
/// a multiply-driven net keeps the last one rather than erroring.
///
/// ⚠️ **Block ports are only consulted when the design has NO IO pads.** With pads present the
/// pads themselves carry the connectivity, and reading ports as well would double-count it.
pub fn build_net(
    net: &DbNet,
    inst_to_cluster: &dyn Fn(usize) -> Option<ClusterId>,
    bterm_to_cluster: &dyn Fn(usize) -> Option<ClusterId>,
    design_has_io_pads: bool,
) -> Net {
    let mut driver = None;
    let mut loads = Vec::new();

    for t in &net.iterms {
        let Some(id) = inst_to_cluster(t.inst) else { continue };
        if t.is_output {
            driver = Some(id);
        } else {
            loads.push(id);
        }
    }

    if !design_has_io_pads {
        for p in &net.bterms {
            let Some(id) = bterm_to_cluster(p.bterm) else { continue };
            if p.is_input {
                driver = Some(id);
            } else {
                loads.push(id);
            }
        }
    }

    Net { driver, loads }
}

/// The driver→load pairs this net contributes, or nothing.
///
/// ⚠️ **Three ways a net contributes nothing**, and the third is a threshold: no driver, no loads,
/// or **`loads.len() >= large_net_threshold`**. A clock or reset touching half the design would
/// otherwise tie every cluster to every other and flatten the connectivity it is meant to express.
///
/// 🔑 The comparison is `>=`, not `>` — a net with exactly `large_net_threshold` loads is already
/// too large.
///
/// ℹ️ A load in the **same** cluster as the driver is skipped: a cluster is not connected to itself.
/// Duplicate loads are **not** deduplicated — two pins of one cluster on a net weigh twice.
pub fn connections_for(net: &Net, large_net_threshold: usize) -> Vec<(ClusterId, ClusterId)> {
    let Some(driver) = net.driver else { return Vec::new() };
    if net.loads.is_empty() || net.loads.len() >= large_net_threshold {
        return Vec::new();
    }
    net.loads
        .iter()
        .filter(|&&load| load != driver)
        .map(|&load| (driver, load))
        .collect()
}

/// Accumulated connection weights, per cluster.
///
/// 🔑 **Symmetric**: connecting A to B records the weight on BOTH. A one-sided map would make the
/// merge stage's answer depend on which cluster it asked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Connections {
    per_cluster: BTreeMap<ClusterId, BTreeMap<ClusterId, f32>>,
}

/// The weight one net contributes to one driver/load pair. Upstream's `connection_weight`.
pub const CONNECTION_WEIGHT: f32 = 1.0;

impl Connections {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a connection, both ways. Weights **accumulate**, so a pair joined by three nets
    /// weighs three.
    pub fn connect(&mut self, a: ClusterId, b: ClusterId, weight: f32) {
        *self.per_cluster.entry(a).or_default().entry(b).or_insert(0.0) += weight;
        *self.per_cluster.entry(b).or_default().entry(a).or_insert(0.0) += weight;
    }

    pub fn weight(&self, a: ClusterId, b: ClusterId) -> f32 {
        self.per_cluster.get(&a).and_then(|m| m.get(&b)).copied().unwrap_or(0.0)
    }

    /// Everything `cluster` connects to, id-ordered.
    pub fn of(&self, cluster: ClusterId) -> Vec<(ClusterId, f32)> {
        self.per_cluster
            .get(&cluster)
            .map(|m| m.iter().map(|(&k, &v)| (k, v)).collect())
            .unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.per_cluster.is_empty()
    }

    /// ⚠️ Rebuilt from scratch every merge round — the cluster ids change as clusters merge, so a
    /// stale map would connect clusters that no longer exist.
    pub fn clear(&mut self) {
        self.per_cluster.clear();
    }
}

/// Build the whole connection map for one round.
pub fn build_connections(
    nets: &[DbNet],
    design: &Design,
    inst_to_cluster: &dyn Fn(usize) -> Option<ClusterId>,
    bterm_to_cluster: &dyn Fn(usize) -> Option<ClusterId>,
    design_has_io_pads: bool,
    large_net_threshold: usize,
) -> Connections {
    let mut c = Connections::new();
    for db_net in nets {
        if !is_valid_net(db_net, design) {
            continue;
        }
        let net = build_net(db_net, inst_to_cluster, bterm_to_cluster, design_has_io_pads);
        for (a, b) in connections_for(&net, large_net_threshold) {
            c.connect(a, b, CONNECTION_WEIGHT);
        }
    }
    c
}
