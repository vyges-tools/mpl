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

/// A merge that upstream treats as **impossible** rather than unlikely.
///
/// ⛔ Upstream calls `logger_->critical` on these, which **aborts**. They can only happen if two
/// clusters selected as siblings turn out not to share a parent. We record them instead of
/// aborting, so the caller can refuse with a reason — but a non-empty list means a real invariant
/// broke, not a design the engine merely cannot handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImpossibleMerge {
    /// MPL-23: two siblings with the same connection signature failed to merge.
    SameSignature { receiver: ClusterId, incomer: ClusterId },
    /// MPL-24: two dust siblings failed to merge.
    Dust { receiver: ClusterId, incomer: ClusterId },
}

/// What the merge loop did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergeReport {
    pub rounds: usize,
    /// `(receiver, incomer)` in the order the merges happened.
    pub merged: Vec<(ClusterId, ClusterId)>,
    pub impossible: Vec<ImpossibleMerge>,
}

fn index_of(parent: &Cluster, id: ClusterId) -> Option<usize> {
    parent.children.iter().position(|c| c.id == id)
}

/// Fold `incomer_id` into `receiver_id`, both children of `parent`.
///
/// ⚠️ Uses `remove`, not `swap_remove`: sibling ORDER is observable downstream, and swapping
/// would silently reorder the tree.
fn merge_siblings(parent: &mut Cluster, receiver_id: ClusterId, incomer_id: ClusterId) -> bool {
    let (Some(ri), Some(ii)) = (index_of(parent, receiver_id), index_of(parent, incomer_id)) else {
        // Not siblings — upstream's `attemptMerge` returns false on differing parents.
        return false;
    };
    if ri == ii {
        return false;
    }
    let incomer = parent.children.remove(ii);
    let ri = index_of(parent, receiver_id).expect("receiver survives removing another child");
    merge_into(&mut parent.children[ri], incomer);
    true
}

/// Upstream `mergeChildrenBelowThresholds`: absorb children too small to stand alone.
///
/// `rebuild_connections` is called **once per round**, because cluster ids change as clusters
/// merge and a stale map would connect clusters that no longer exist.
///
/// 🔑 **Three merge types, in order, and the order is the algorithm** — a cluster absorbed by
/// type 1 is no longer available to type 2:
///
/// 1. **A single well-formed connected neighbour.** ⚠️ Its receiver may not be a sibling, in which
///    case the merge simply does not happen — upstream does not treat that as an error.
/// 2. **Siblings with the same connection signature.** ⛔ Failure here is `critical` upstream.
/// 3. **Dust into dust.** ⛔ Failure here is `critical` too.
///
/// The loop ends when a round merges nothing, or when no small children remain.
#[allow(clippy::too_many_arguments)]
pub fn merge_children_below_thresholds(
    parent: &mut Cluster,
    mut small: Vec<ClusterId>,
    rebuild_connections: &mut dyn FnMut(&Cluster) -> Connections,
    is_io_cluster: &dyn Fn(ClusterId) -> bool,
    min_std_cell: i32,
    min_macro: i32,
    max_std_cell: i32,
    max_macro: i32,
) -> MergeReport {
    let mut report = MergeReport::default();
    if small.is_empty() {
        return report;
    }

    loop {
        report.rounds += 1;
        let conns = rebuild_connections(parent);

        let count_at_round_start = small.len();
        // `None` = still unmerged. Upstream's `cluster_class`.
        let mut absorbed: Vec<bool> = vec![false; small.len()];

        // ---- Type 1: a single well-formed connected neighbour.
        for i in 0..small.len() {
            let Some(close) =
                find_single_well_formed_connected_cluster(&conns, small[i], &small, is_io_cluster)
            else {
                continue;
            };
            let (Some(ci), Some(si)) = (index_of(parent, close), index_of(parent, small[i])) else {
                continue;
            };
            if !merge_honors_max_thresholds(
                &parent.children[ci],
                &parent.children[si],
                max_std_cell,
                max_macro,
            ) {
                continue;
            }
            // ⚠️ Upstream calls `attemptMerge` here and lets it return false when the neighbour
            // is not a sibling, WITHOUT treating that as an error — unlike types 2 and 3. We
            // reach the same outcome one step earlier: the index lookup above already skipped a
            // non-sibling, so this call only ever sees siblings. Behaviourally identical; noted
            // because a mutation adding an error report here cannot fail, the guard having
            // short-circuited first.
            if merge_siblings(parent, close, small[i]) {
                absorbed[i] = true;
                report.merged.push((close, small[i]));
            }
        }

        // ---- Type 2: siblings with the same connection signature.
        for i in 0..small.len() {
            if absorbed[i] {
                continue;
            }
            for j in (i + 1)..small.len() {
                if absorbed[j] {
                    continue;
                }
                let (Some(ii), Some(ji)) = (index_of(parent, small[i]), index_of(parent, small[j]))
                else {
                    continue;
                };
                if !merge_honors_max_thresholds(
                    &parent.children[ii],
                    &parent.children[ji],
                    max_std_cell,
                    max_macro,
                ) || !same_connection_signature(&conns, small[i], small[j])
                {
                    continue;
                }
                if merge_siblings(parent, small[i], small[j]) {
                    absorbed[j] = true;
                    report.merged.push((small[i], small[j]));
                } else {
                    report.impossible.push(ImpossibleMerge::SameSignature {
                        receiver: small[i],
                        incomer: small[j],
                    });
                }
            }
        }

        // ---- Type 3: dust absorbs dust.
        let mut survivors = Vec::new();
        for i in 0..small.len() {
            if absorbed[i] {
                continue;
            }
            survivors.push(small[i]);
            let Some(ii) = index_of(parent, small[i]) else { continue };
            if !is_dust(&parent.children[ii]) {
                continue;
            }
            for j in (i + 1)..small.len() {
                if absorbed[j] {
                    continue;
                }
                let Some(ji) = index_of(parent, small[j]) else { continue };
                if !is_dust(&parent.children[ji]) {
                    continue;
                }
                // ⚠️ No threshold check here — dust merges regardless of the maximums.
                if merge_siblings(parent, small[i], small[j]) {
                    absorbed[j] = true;
                    report.merged.push((small[i], small[j]));
                } else {
                    report.impossible.push(ImpossibleMerge::Dust {
                        receiver: small[i],
                        incomer: small[j],
                    });
                }
            }
        }

        // Some survivors have grown past the minimums and are no longer "small".
        small = survivors
            .iter()
            .copied()
            .filter(|&id| {
                index_of(parent, id).is_some_and(|k| {
                    crate::cluster::is_merge_candidate(&parent.children[k], min_std_cell, min_macro)
                })
            })
            .collect();

        // Exit when nothing was absorbed this round.
        //
        // ℹ️ Upstream compares against the SURVIVORS rather than the filtered list, and that is
        // kept for faithfulness — but mutation testing showed the two are **equivalent here**:
        // a cluster can only leave the small list by growing, and it can only grow by absorbing
        // another, so "nothing absorbed" implies the filtered list is unchanged too. Documented
        // rather than asserted, because a test pinning the distinction could not fail.
        if count_at_round_start == survivors.len() {
            break;
        }
        if small.is_empty() {
            break;
        }
    }

    report
}
