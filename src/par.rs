// SPDX-License-Identifier: Apache-2.0
//! The partitioner `run` links with the `par` feature: [`vyges_par`] answering each
//! [`PartitionCall`](crate::flatpart::PartitionCall) of a large flat cluster.
//!
//! Each call is the partitioner's simple k-way mode: a fresh partitioner, the call's own balance
//! and seed, every other parameter at its default. A call the partitioner refuses (a stage this
//! build cannot run) refuses the clustering by name, as an unlinked build does.

use crate::flatpart::{FlatPartitioner, PartitionCall};
use vyges_par::multilevel::multilevel_partition;
use vyges_par::{simple_hypergraph, Params};

pub struct Par;

impl FlatPartitioner for Par {
    fn partition_k_way_simple(&mut self, c: &PartitionCall) -> Result<Vec<usize>, String> {
        let g = simple_hypergraph(&c.hyperedges, &c.vertex_weights, &c.hyperedge_weights);
        let p = Params::simple(c.num_parts, c.balance, c.seed);
        multilevel_partition(&p, &g).0.map_err(|r| format!("{r:?}"))
    }
}
