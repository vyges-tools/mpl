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
