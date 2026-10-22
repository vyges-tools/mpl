// SPDX-License-Identifier: Apache-2.0
//! Splitting a large flat cluster in two with a hypergraph partitioner.
//!
//! Upstream: `ClusteringEngine::breakLargeFlatCluster` and `partitionerSolutionIsFullyUnbalanced`.
//!
//! 🔑 **The partitioner is a parameter, not a dependency.** This crate builds the hypergraph and
//! applies the answer; whoever links a partitioner supplies [`FlatPartitioner`]. With none, the
//! cluster is REFUSED by name — never approximated.

use crate::cluster::{Cluster, ClusterId};
use crate::design::{is_ignored_inst, Design};
use crate::netlist::DbNet;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One partitioner call: its inputs, and its answer once there is one.
///
/// ℹ️ The field names are the partitioner entry point's own, so a recorded call reads the same on
/// both sides of a comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionCall {
    pub num_parts: usize,
    pub balance: f32,
    pub seed: i32,
    pub hyperedges: Vec<Vec<usize>>,
    pub vertex_weights: Vec<f32>,
    pub hyperedge_weights: Vec<f32>,
    pub solution: Option<Vec<usize>>,
}

impl PartitionCall {
    /// One JSON object per call, keys in the order above.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "num_parts": self.num_parts,
            "balance": self.balance,
            "seed": self.seed,
            "hyperedges": self.hyperedges,
            "vertex_weights": self.vertex_weights,
            "hyperedge_weights": self.hyperedge_weights,
            "solution": self.solution,
        })
    }
}

/// A k-way partitioner in its simple mode: no timing, no placement, no fences.
pub trait FlatPartitioner {
    /// The block of every vertex, or `Err` naming why there is no answer.
    fn partition_k_way_simple(&mut self, call: &PartitionCall) -> Result<Vec<usize>, String>;
}

/// No partitioner linked: every call is unanswered, so the cluster is refused — after its call
/// is recorded.
pub struct NoPartitioner;

impl FlatPartitioner for NoPartitioner {
    fn partition_k_way_simple(&mut self, _call: &PartitionCall) -> Result<Vec<usize>, String> {
        Err("no partitioner linked".into())
    }
}

/// The hypergraph `breakLargeFlatCluster` hands the partitioner.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatHypergraph {
    pub hyperedges: Vec<Vec<usize>>,
    pub vertex_weights: Vec<f32>,
    /// The first vertices stand for every live cluster; the parent's own instances follow.
    pub num_other_cluster_vertices: usize,
    /// The parent's instances in vertex order: macros, then standard cells.
    pub insts: Vec<usize>,
}

/// What the hypergraph is built from, besides the parent.
pub struct FlatInputs<'a> {
    pub design: &'a Design,
    pub nets: &'a [DbNet],
    /// Upstream `id_to_cluster`'s keys: every cluster alive at the call, in id order.
    pub live: &'a BTreeSet<ClusterId>,
    /// Upstream `inst_to_cluster_id` as it stands at the call.
    pub inst_to_cluster: &'a [Option<ClusterId>],
    pub bterm_to_cluster: &'a [Option<ClusterId>],
    pub large_net_threshold: usize,
    pub dbu_per_micron: i32,
}

/// The vertices and hyperedges of `breakLargeFlatCluster`, before the partitioner runs.
///
/// 🔑 **Every live cluster is a vertex of weight 0**, the parent included, so the partitioner sees
/// how the parent's instances connect to the rest of the tree. Only the parent's own instances
/// carry weight — their area in square microns.
///
/// 🔑 **One hyperedge per signal net**, its vertices a SORTED set: the driver (the LAST output
/// instance pin, then overridden by the LAST input port) plus every load. Kept only with a driver
/// and between 2 and `large_net_threshold - 1` distinct vertices.
///
/// ⚠️ A net touching an ignored instance is dropped whole — even when its other pins are inside
/// the parent.
///
/// ⚠️ **A lookup that misses yields vertex 0, not an error.** Upstream indexes both maps with
/// `operator[]`, so an instance whose cluster is no longer live, or that claims the parent without
/// being one of its leaves, lands on the first vertex.
pub fn build_flat_hypergraph(parent: &Cluster, inp: &FlatInputs) -> Result<FlatHypergraph, String> {
    let mut cluster_vertex: BTreeMap<ClusterId, usize> = BTreeMap::new();
    let mut vertex_weights: Vec<f32> = Vec::new();
    for &id in inp.live {
        cluster_vertex.insert(id, vertex_weights.len());
        vertex_weights.push(0.0);
    }
    let num_other_cluster_vertices = vertex_weights.len();

    let d = inp.dbu_per_micron as f64;
    let mut inst_vertex: HashMap<usize, usize> = HashMap::new();
    let mut insts = Vec::new();
    for &i in parent.leaf_macros.iter().chain(parent.leaf_std_cells.iter()) {
        inst_vertex.insert(i, vertex_weights.len());
        // `dbuAreaToMicrons` is double; the weight vector is float.
        vertex_weights.push((inp.design.instances[i].bbox.area() as f64 / (d * d)) as f32);
        insts.push(i);
    }

    let cluster_v = |id: ClusterId| cluster_vertex.get(&id).copied().unwrap_or(0) as i64;
    let mut hyperedges = Vec::new();
    for net in inp.nets {
        if net.is_supply {
            continue;
        }
        let mut driver: i64 = -1;
        let mut loads: BTreeSet<i64> = BTreeSet::new();
        let mut ignore = false;
        for t in &net.iterms {
            if is_ignored_inst(&inp.design.instances[t.inst]) {
                ignore = true;
                break;
            }
            let Some(cluster) = inp.inst_to_cluster[t.inst] else {
                return Err(format!("instance {} has no cluster", inp.design.instances[t.inst].name));
            };
            let v = if cluster != parent.id {
                cluster_v(cluster)
            } else {
                inst_vertex.get(&t.inst).copied().unwrap_or(0) as i64
            };
            if t.is_output {
                driver = v;
            } else {
                loads.insert(v);
            }
        }
        if ignore {
            continue;
        }
        for b in &net.bterms {
            let Some(cluster) = inp.bterm_to_cluster.get(b.bterm).copied().flatten() else {
                return Err(format!("port {} of net {} has no cluster", b.bterm, net.name));
            };
            if b.is_input {
                driver = cluster_v(cluster);
            } else {
                loads.insert(cluster_v(cluster));
            }
        }
        loads.insert(driver);
        if driver != -1 && loads.len() > 1 && loads.len() < inp.large_net_threshold {
            hyperedges.push(loads.into_iter().map(|v| v as usize).collect());
        }
    }

    Ok(FlatHypergraph { hyperedges, vertex_weights, num_other_cluster_vertices, insts })
}

/// Upstream `partitionerSolutionIsFullyUnbalanced`: every INSTANCE vertex in the same block.
/// ⚠️ The cluster vertices are not consulted — where they land does not matter.
pub fn is_fully_unbalanced(solution: &[usize], num_other_cluster_vertices: usize) -> bool {
    let first = solution[num_other_cluster_vertices];
    solution[num_other_cluster_vertices..].iter().all(|&b| b == first)
}

/// Upstream's starting balance, and how far each fully unbalanced answer relaxes it.
pub const DEFAULT_BALANCE_CONSTRAINT: f32 = 1.0;
pub const BALANCE_CONSTRAINT_RELAXATION_FACTOR: f32 = 10.0;
/// MPL-45 fires when the balance about to be tried has reached this.
pub const BALANCE_CONSTRAINT_LIMIT: f32 = 90.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::{Instance, MasterKind, Module, Rect};
    use crate::netlist::{InstTerm, PortTerm};

    fn inst(name: &str, area_side: i64) -> Instance {
        Instance {
            name: name.into(),
            is_block: false,
            is_fixed: false,
            bbox: Rect { x_min: 0, y_min: 0, x_max: area_side, y_max: area_side },
            master: MasterKind::default(),
            master_id: 0,
            is_ignorable_macro: false,
        }
    }

    fn design(insts: Vec<Instance>) -> Design {
        let n = insts.len();
        Design {
            instances: insts,
            modules: vec![Module {
                name: "top".into(),
                hierarchical_name: "top".into(),
                insts: (0..n).collect(),
                children: vec![],
            }],
            top: 0,
            core_area: Rect { x_min: 0, y_min: 0, x_max: 100, y_max: 100 },
            die_area: Rect { x_min: 0, y_min: 0, x_max: 100, y_max: 100 },
        }
    }

    fn it(inst: usize, is_output: bool) -> InstTerm {
        InstTerm { inst, is_output, term: "A".into() }
    }

    fn net(iterms: Vec<InstTerm>, bterms: Vec<PortTerm>) -> DbNet {
        DbNet { name: "n".into(), is_supply: false, iterms, bterms }
    }

    /// Cluster vertices first, then the parent's instances; the driver joins its loads in a
    /// sorted set; an input port overrides an instance driver, which then drops out of the net;
    /// a missing cluster id lands on vertex 0.
    #[test]
    fn hypergraph_follows_the_reference_rules() {
        let d = design(vec![inst("a", 2000), inst("b", 1000), inst("c", 1000), inst("pad", 1)]);
        let mut parent = Cluster::new(5, "p");
        parent.leaf_std_cells = vec![0, 1];
        let live: BTreeSet<ClusterId> = [0, 2, 5].into_iter().collect();
        // c sits in cluster 2; nothing claims id 9.
        let assoc = vec![Some(5), Some(5), Some(2), Some(9)];
        let nets = vec![
            // b drives a and c: {3, 4, 1}.
            net(vec![it(0, false), it(1, true), it(2, false)], vec![]),
            // a drives, but the input port in cluster 0 takes over — and a, never a load, is
            // LOST: {0, 4}.
            net(vec![it(0, true), it(1, false)], vec![PortTerm { bterm: 0, is_input: true }]),
            // No driver: dropped.
            net(vec![it(0, false), it(1, false)], vec![]),
            // A stale cluster id is vertex 0: {0, 4}.
            net(vec![it(1, true), it(3, false)], vec![]),
            // Two pins on one vertex: a single distinct vertex, dropped.
            net(vec![it(0, true), it(0, false)], vec![]),
        ];
        let inp = FlatInputs {
            design: &d,
            nets: &nets,
            live: &live,
            inst_to_cluster: &assoc,
            bterm_to_cluster: &[Some(0)],
            large_net_threshold: 50,
            dbu_per_micron: 1000,
        };
        let g = build_flat_hypergraph(&parent, &inp).unwrap();
        assert_eq!(g.num_other_cluster_vertices, 3);
        assert_eq!(g.vertex_weights, vec![0.0, 0.0, 0.0, 4.0, 1.0]);
        assert_eq!(g.insts, vec![0, 1]);
        assert_eq!(g.hyperedges, vec![vec![1, 3, 4], vec![0, 4], vec![0, 4]]);
    }

    /// A net touching an ignored instance is dropped whole, and so is a supply net; a net of
    /// `large_net_threshold` distinct vertices is dropped, one fewer is kept.
    #[test]
    fn ignored_pins_supply_nets_and_the_large_net_threshold_drop_nets() {
        let mut pad = inst("pad", 1);
        pad.master.is_pad = true;
        let d = design(vec![inst("a", 1), inst("b", 1), inst("c", 1), inst("d", 1), pad]);
        let mut parent = Cluster::new(1, "p");
        parent.leaf_std_cells = vec![0, 1, 2, 3];
        let live: BTreeSet<ClusterId> = [1].into_iter().collect();
        let assoc = vec![Some(1); 5];
        let mut supply = net(vec![it(0, true), it(1, false)], vec![]);
        supply.is_supply = true;
        let nets = vec![
            net(vec![it(0, true), it(4, false), it(1, false)], vec![]),
            supply,
            net(vec![it(0, true), it(1, false), it(2, false), it(3, false)], vec![]),
            net(vec![it(0, true), it(1, false), it(2, false)], vec![]),
        ];
        let inp = FlatInputs {
            design: &d,
            nets: &nets,
            live: &live,
            inst_to_cluster: &assoc,
            bterm_to_cluster: &[],
            large_net_threshold: 4,
            dbu_per_micron: 1,
        };
        let g = build_flat_hypergraph(&parent, &inp).unwrap();
        assert_eq!(g.hyperedges, vec![vec![1, 2, 3]]);
    }

    /// Two output pins on a net: the LAST one drives, and the earlier one — never a load — drops
    /// out. ℹ️ No tinyRocket net has two, so only this test sees the rule.
    #[test]
    fn the_last_output_pin_drives() {
        let d = design(vec![inst("a", 1), inst("b", 1), inst("c", 1)]);
        let mut parent = Cluster::new(1, "p");
        parent.leaf_std_cells = vec![0, 1, 2];
        let live: BTreeSet<ClusterId> = [1].into_iter().collect();
        let assoc = vec![Some(1); 3];
        let nets = vec![net(vec![it(0, true), it(1, true), it(2, false)], vec![])];
        let inp = FlatInputs {
            design: &d,
            nets: &nets,
            live: &live,
            inst_to_cluster: &assoc,
            bterm_to_cluster: &[],
            large_net_threshold: 50,
            dbu_per_micron: 1,
        };
        assert_eq!(build_flat_hypergraph(&parent, &inp).unwrap().hyperedges, vec![vec![2, 3]]);
    }

    #[test]
    fn only_instance_vertices_decide_unbalance() {
        assert!(is_fully_unbalanced(&[0, 1, 1, 1], 1));
        assert!(!is_fully_unbalanced(&[1, 1, 0, 1], 1));
    }
}
