// SPDX-License-Identifier: Apache-2.0
//! Classifying macros before they are grouped.
//!
//! Upstream: `classifyMacrosBySize`, `classifyMacrosByConnSignature`,
//! `classifyMacrosByInterconn`, `groupSingleMacroClusters`.
//!
//! 🔑 **Three classifications, then one grouping that reads all three.** Each assigns every macro
//! a *representative index* — the lowest-numbered macro it is equivalent to — and the grouping
//! merges two macros only when their representatives agree.

use crate::cluster::ClusterId;
use crate::merge::{same_connection_signature, strong_connection};
use crate::netlist::Connections;

/// A macro's dimensions. ⚠️ Upstream's `HardMacro::operator==` compares **width and height only**
/// — not area, not master. Two different masters with identical dimensions are "the same size".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacroSize {
    pub width: i64,
    pub height: i64,
}

/// `classifyMacrosBySize`.
///
/// ⚠️ The representative is assigned in a **second pass**: the first pass only marks followers, so
/// an unmatched macro keeps `-1` until the end and then becomes its own representative. Assigning
/// it in the first pass would change which index leads a group.
pub fn classify_by_size(sizes: &[MacroSize]) -> Vec<usize> {
    let mut class: Vec<i64> = vec![-1; sizes.len()];
    for i in 0..sizes.len() {
        if class[i] == -1 {
            for j in (i + 1)..sizes.len() {
                if class[j] == -1 && sizes[i] == sizes[j] {
                    class[j] = i as i64;
                }
            }
        }
    }
    // Second pass: whoever is still unassigned represents itself.
    class
        .iter()
        .enumerate()
        .map(|(i, &c)| if c == -1 { i } else { c as usize })
        .collect()
}

/// `classifyMacrosByConnSignature`.
///
/// ⚠️ Unlike the size pass, the representative is assigned **immediately** (`class[i] = i` before
/// the inner loop), because `same_connection_signature` must see a settled state.
pub fn classify_by_conn_signature(conns: &Connections, ids: &[ClusterId]) -> Vec<usize> {
    let mut class: Vec<i64> = vec![-1; ids.len()];
    for i in 0..ids.len() {
        if class[i] != -1 {
            continue;
        }
        class[i] = i as i64;
        for j in (i + 1)..ids.len() {
            if class[j] == -1 && same_connection_signature(conns, ids[i], ids[j]) {
                class[j] = i as i64;
            }
        }
    }
    class.iter().map(|&c| c as usize).collect()
}

/// `classifyMacrosByInterconn`: which macros are strongly wired to each other.
///
/// 🔑 **The inner loop runs over ALL macros, not just later ones**, and it **breaks** as soon as it
/// meets a neighbour that already has a class — adopting that class rather than leading its own.
/// So a macro can end up in a group led by a *higher* index, which the other two classifiers can
/// never do.
///
/// ⚠️ `-1` is meaningful downstream: it marks a macro that leads no interconnected array.
pub fn classify_by_interconn(conns: &Connections, ids: &[ClusterId]) -> Vec<i64> {
    let mut class: Vec<i64> = vec![-1; ids.len()];
    for i in 0..ids.len() {
        if class[i] != -1 {
            continue;
        }
        class[i] = i as i64;
        for j in 0..ids.len() {
            if i == j {
                continue;
            }
            if strong_connection(conns, ids[i], ids[j]) {
                if class[j] != -1 {
                    // Adopt the neighbour's group and stop looking.
                    class[i] = class[j];
                    break;
                }
                class[j] = i as i64;
            }
        }
    }
    class
}

/// A merge the grouping decided on, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMerge {
    /// Same size and same interconnection group — an **array of interconnected macros**.
    Interconnected { receiver: usize, incomer: usize },
    /// Same size and same connection signature.
    SameSignature { receiver: usize, incomer: usize },
}

/// What the grouping produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grouping {
    /// Representative per macro; `macro_class[i] == i` means it leads a group.
    pub macro_class: Vec<usize>,
    /// ⚠️ **Mutated by the grouping**, not just read: a macro that meets a same-size neighbour in a
    /// *different* interconnection group has its own class cleared to `-1`. That is how upstream
    /// distinguishes a real interconnected array from macros merely sharing a signature.
    pub interconn_class: Vec<i64>,
    pub merges: Vec<GroupMerge>,
}

/// `groupSingleMacroClusters`.
///
/// 🔑 **Same SIZE is required for every merge** — the two connection tests only decide *which kind*
/// of group it is, never whether to group at all. Two macros of different dimensions never merge
/// however strongly they are wired together.
pub fn group_single_macro_clusters(
    size_class: &[usize],
    signature_class: &[usize],
    interconn_class: &[i64],
) -> Grouping {
    let n = size_class.len();
    let mut macro_class: Vec<i64> = vec![-1; n];
    let mut interconn = interconn_class.to_vec();
    let mut merges = Vec::new();

    for i in 0..n {
        if macro_class[i] != -1 {
            continue;
        }
        macro_class[i] = i as i64;

        for j in (i + 1)..n {
            if macro_class[j] != -1 || size_class[i] != size_class[j] {
                continue;
            }
            if interconn[i] == interconn[j] {
                merges.push(GroupMerge::Interconnected { receiver: i, incomer: j });
                macro_class[j] = i as i64;
            } else {
                // ⚠️ Clearing i's class here is upstream's, and it affects every LATER j in this
                // same inner loop — so the order of comparisons changes the outcome.
                interconn[i] = -1;
                if signature_class[i] == signature_class[j] {
                    merges.push(GroupMerge::SameSignature { receiver: i, incomer: j });
                    macro_class[j] = i as i64;
                }
            }
        }
    }

    Grouping {
        macro_class: macro_class.iter().map(|&c| c as usize).collect(),
        interconn_class: interconn,
        merges,
    }
}

/// A single movable macro is never treated as an array of one.
///
/// ⚠️ Upstream special-cases this before any classification runs.
pub fn single_macro_grouping() -> Grouping {
    Grouping { macro_class: vec![0], interconn_class: vec![-1], merges: Vec::new() }
}

/// One macro that `breakMixedLeaf` turned into its own cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroCluster {
    pub id: ClusterId,
    /// Upstream names the cluster after the macro instance itself.
    pub name: String,
    pub inst: usize,
    pub is_fixed: bool,
    pub size: MacroSize,
}

/// A surviving group of movable macros.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroArray {
    pub id: ClusterId,
    /// ⚠️ Set only when the group's interconnection class survived the grouping. It is what
    /// distinguishes a genuinely interconnected array from macros merely sharing a signature.
    pub is_interconnected_array: bool,
    /// The macro clusters folded into this one, the leader first.
    pub members: Vec<ClusterId>,
}

/// What `breakMixedLeaf` decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixedLeafPlan {
    /// The mixed leaf itself, retyped as a standard-cell cluster with its macros removed.
    pub std_cell_cluster: ClusterId,
    pub arrays: Vec<MacroArray>,
    /// 🔑 **Parented to the ROOT, not to the mixed leaf's parent.** A fixed macro is not the
    /// placer's to move, so it is lifted out of the local hierarchy entirely.
    pub fixed_clusters: Vec<ClusterId>,
    /// Every pair among the std-cell cluster, the arrays and the fixed clusters.
    pub virtual_connections: Vec<(ClusterId, ClusterId)>,
}

/// Upstream `breakMixedLeaf`: split a cluster holding both macros and standard cells.
///
/// The order is upstream's, and two steps of it are structural rather than cosmetic:
///
/// 1. **Every macro becomes its own cluster**, named after the instance. ⚠️ A **fixed** macro's
///    cluster is parented to the **root**; a movable one to the mixed leaf's parent.
/// 2. The macros are then classified and grouped — but only the **movable** ones. A fixed macro
///    is never folded into an array, because it cannot move to join one.
/// 3. The mixed leaf keeps its standard cells and becomes a `StdCellCluster`.
/// 4. **Virtual connections join every pair** among the std-cell cluster, the surviving arrays and
///    the fixed clusters — so the annealer knows they came from one place.
pub fn break_mixed_leaf(
    mixed_leaf: ClusterId,
    macros: &[MacroCluster],
    conns: &Connections,
) -> MixedLeafPlan {
    let movable: Vec<&MacroCluster> = macros.iter().filter(|m| !m.is_fixed).collect();
    let fixed_clusters: Vec<ClusterId> =
        macros.iter().filter(|m| m.is_fixed).map(|m| m.id).collect();

    let grouping = if movable.len() == 1 {
        // ⚠️ Special-cased before any classification: one macro is not an array of one.
        single_macro_grouping()
    } else {
        let ids: Vec<ClusterId> = movable.iter().map(|m| m.id).collect();
        let sizes: Vec<MacroSize> = movable.iter().map(|m| m.size).collect();
        let size_class = classify_by_size(&sizes);
        let signature_class = classify_by_conn_signature(conns, &ids);
        let interconn_class = classify_by_interconn(conns, &ids);
        group_single_macro_clusters(&size_class, &signature_class, &interconn_class)
    };

    // Only the group LEADERS survive; the rest were folded in.
    let mut arrays = Vec::new();
    for (i, m) in movable.iter().enumerate() {
        if grouping.macro_class.get(i) != Some(&i) {
            continue;
        }
        let members = movable
            .iter()
            .enumerate()
            .filter(|(j, _)| grouping.macro_class.get(*j) == Some(&i))
            .map(|(_, mc)| mc.id)
            .collect();
        arrays.push(MacroArray {
            id: m.id,
            is_interconnected_array: grouping.interconn_class.get(i).copied().unwrap_or(-1) != -1,
            members,
        });
    }

    // 🔑 The std-cell cluster comes FIRST, then the arrays, then the fixed clusters — the order
    // upstream builds the list in, and the pairs below follow it.
    let mut virtual_members = vec![mixed_leaf];
    virtual_members.extend(arrays.iter().map(|a| a.id));
    virtual_members.extend(fixed_clusters.iter().copied());

    let mut virtual_connections = Vec::new();
    for i in 0..virtual_members.len() {
        for j in (i + 1)..virtual_members.len() {
            virtual_connections.push((virtual_members[i], virtual_members[j]));
        }
    }

    MixedLeafPlan { std_cell_cluster: mixed_leaf, arrays, fixed_clusters, virtual_connections }
}
