// SPDX-License-Identifier: Apache-2.0
//! Merging clusters that are too small to stand alone.
//!
//! Upstream: `mergeChildrenBelowThresholds`, `findSingleWellFormedConnectedCluster`,
//! `strongConnection`, `findNeighbors`, `sameConnectionSignature`, `mergeHonorsMaxThresholds`,
//! `Cluster::attemptMerge`.
//!
//! 🔑 **Three merge types, tried in order, repeated until a round changes nothing.** The order is
//! part of the algorithm: a cluster absorbed by type 1 is no longer available to type 2.

use crate::cluster::{Cluster, ClusterId};
use crate::netlist::Connections;

/// Upstream's `minimum_connection_ratio_`. A connection carrying less than this share of the
/// weight involved is not "strong".
pub const MINIMUM_CONNECTION_RATIO: f32 = 0.08;

/// A cluster is "dust" when it holds at most this many standard cells and no macros.
pub const DUST_CLUSTER_STD_CELL: i32 = 10;

/// Total weight on all of a cluster's connections.
pub fn all_connections_weight(conns: &Connections, cluster: ClusterId) -> f32 {
    conns.of(cluster).iter().map(|(_, w)| *w).sum()
}

/// Is the connection between `a` and `b` a strong one?
///
/// ⚠️ **The denominator subtracts the connection once.** It appears in *both* clusters' totals, so
/// `a_total + b_total` double-counts it; upstream removes one copy before dividing. Leaving it in
/// understates every ratio and merges less than it should.
pub fn strong_connection(conns: &Connections, a: ClusterId, b: ClusterId) -> bool {
    debug_assert_ne!(a, b, "upstream errors (MPL-61) on evaluating a cluster against itself");
    let weight = conns.weight(a, b);
    if weight == 0.0 {
        return false;
    }
    let total = all_connections_weight(conns, a) + all_connections_weight(conns, b) - weight;
    if total <= 0.0 {
        return false;
    }
    weight / total >= MINIMUM_CONNECTION_RATIO
}

/// The clusters `target` is strongly enough connected to, ignoring one.
///
/// 🔑 **A DIFFERENT denominator from [`strong_connection`]** — here it is only the *target's* own
/// total, with no subtraction. The asymmetry is upstream's and it is easy to miss: the two
/// functions answer different questions, and unifying them changes which clusters merge.
pub fn find_neighbors(
    conns: &Connections,
    target: ClusterId,
    ignored: ClusterId,
) -> Vec<ClusterId> {
    let total = all_connections_weight(conns, target);
    if total <= 0.0 {
        return Vec::new();
    }
    conns
        .of(target)
        .into_iter()
        .filter(|&(id, _)| id != ignored)
        .filter(|&(_, w)| w / total >= MINIMUM_CONNECTION_RATIO)
        .map(|(id, _)| id)
        .collect()
}

/// Do two clusters connect to the same set of neighbours, ignoring each other?
///
/// ⚠️ **An empty neighbour set is NOT a match.** Two clusters connected to nothing have the same
/// (empty) signature, and upstream refuses that deliberately — merging on "both isolated" would
/// pull unrelated logic together.
pub fn same_connection_signature(conns: &Connections, a: ClusterId, b: ClusterId) -> bool {
    let mut an = find_neighbors(conns, a, b);
    if an.is_empty() {
        return false;
    }
    let mut bn = find_neighbors(conns, b, a);
    if an.len() != bn.len() {
        return false;
    }
    an.sort_unstable();
    bn.sort_unstable();
    an == bn
}

/// Would merging these two stay within the maximums?
///
/// ⚠️ `<=`, so a merge landing exactly on a maximum is allowed.
pub fn merge_honors_max_thresholds(
    a: &Cluster,
    b: &Cluster,
    max_std_cell: i32,
    max_macro: i32,
) -> bool {
    (a.num_macro() + b.num_macro()) <= max_macro
        && (a.num_std_cell() + b.num_std_cell()) <= max_std_cell
}

/// The single well-formed cluster `target` is strongly connected to, if there is exactly one.
///
/// 🔑 **Exactly one, or nothing.** Two candidates is not "pick the strongest" — upstream declines,
/// because a cluster pulled equally by two neighbours has no obviously right home.
///
/// ⚠️ Candidates that are themselves small are skipped: a small cluster is not *well-formed*, and
/// merging into one would only move the problem. IO clusters are skipped too.
pub fn find_single_well_formed_connected_cluster(
    conns: &Connections,
    target: ClusterId,
    small_ids: &[ClusterId],
    is_io_cluster: &dyn Fn(ClusterId) -> bool,
) -> Option<ClusterId> {
    let mut found = None;
    let mut count = 0;
    for (candidate, _) in conns.of(target) {
        if candidate == target || is_io_cluster(candidate) {
            continue;
        }
        if !strong_connection(conns, target, candidate) {
            continue;
        }
        if small_ids.contains(&candidate) {
            continue;
        }
        count += 1;
        found = Some(candidate);
    }
    if count == 1 {
        found
    } else {
        None
    }
}

/// Merge `incomer` into `receiver`.
///
/// 🔑 **The receiver's name becomes `receiver||incomer`.** That is observable in upstream's own
/// hierarchy dump, so it is reproduced rather than tidied.
///
/// ⚠️ **If the receiver has children, the incomer becomes one of them instead of dissolving.**
/// A cluster with children cannot absorb another's leaves without losing the structure its own
/// children describe.
///
/// Returns whether the incomer was dissolved (rather than adopted).
pub fn merge_into(receiver: &mut Cluster, incomer: Cluster) -> bool {
    receiver.metrics.num_std_cell += incomer.metrics.num_std_cell;
    receiver.metrics.num_macro += incomer.metrics.num_macro;
    receiver.name = format!("{}||{}", receiver.name, incomer.name);

    if !receiver.children.is_empty() {
        receiver.children.push(incomer);
        return false;
    }

    receiver.leaf_macros.extend(incomer.leaf_macros);
    receiver.leaf_std_cells.extend(incomer.leaf_std_cells);
    receiver.db_modules.extend(incomer.db_modules);
    true
}

/// Is this cluster "dust"?
///
/// ⚠️ `<=` on the cell count, and **no macros at all** — a single macro disqualifies it however
/// few cells it has, because a macro is never negligible.
pub fn is_dust(cluster: &Cluster) -> bool {
    cluster.num_std_cell() <= DUST_CLUSTER_STD_CELL && cluster.num_macro() == 0
}
