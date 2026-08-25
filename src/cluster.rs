// SPDX-License-Identifier: Apache-2.0
//! The physical hierarchy — the cluster tree `multilevelAutocluster` builds.
//!
//! Upstream: `mpl/src/object.h` (`Cluster`) and `mpl/src/clusterEngine.cpp`.
//!
//! ⚠️ **Ownership mirrors upstream's.** A cluster owns its children (`unique_ptr` there, `Vec`
//! here), so releasing and re-adopting them is how the tree is restructured — and the ORDER that
//! produces is observable all the way down to macro positions.

use std::collections::VecDeque;

pub type ClusterId = i32;
pub type InstId = usize;
pub type ModuleId = usize;

/// Upstream `enum ClusterType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterType {
    StdCell,
    HardMacro,
    Mixed,
}

/// What a module or cluster contains. Upstream's `Metrics`, reduced to the two counts the tree
/// logic consults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metrics {
    pub num_std_cell: i32,
    pub num_macro: i32,
    /// ⚠️ Carried because the hierarchy dump prints it, and the dump is our oracle. Counts alone
    /// would compare equal against a run whose areas had drifted.
    pub std_cell_area: i64,
    pub macro_area: i64,
}

/// One node of the physical hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    pub id: ClusterId,
    pub name: String,
    pub cluster_type: ClusterType,
    pub db_modules: Vec<ModuleId>,
    pub leaf_std_cells: Vec<InstId>,
    pub leaf_macros: Vec<InstId>,
    pub children: Vec<Cluster>,
    pub metrics: Metrics,
    /// The three flags upstream ORs together for `isIOCluster`.
    pub is_cluster_of_unplaced_io_pins: bool,
    pub is_io_pad_cluster: bool,
    pub is_io_bundle: bool,
    /// ⚠️ NOT part of `is_io_cluster` — it only affects the printed type string.
    pub is_cluster_of_unconstrained_io_pins: bool,
    pub is_fixed_macro: bool,
    /// Only meaningful for a pin-carrying cluster; the dump prints it instead of the counts.
    pub num_io_pins: usize,
    /// The region an unplaced-IO cluster is restricted to. ⚠️ Two pins share a cluster only when
    /// their regions are IDENTICAL — this is matched by equality, not by overlap.
    pub constraint_region: Option<crate::design::Rect>,
    /// The outlines this cluster may take, filled by the shaping stage.
    /// ⚠️ Empty means *not shaped*, which is a different thing from *no legal shape* — the latter
    /// is an MPL-4 error and never reaches here.
    pub tilings: Vec<crate::shaping::Tiling>,
}

impl Cluster {
    pub fn new(id: ClusterId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            // Upstream's Cluster defaults to MixedCluster; the type is narrowed later.
            cluster_type: ClusterType::Mixed,
            db_modules: Vec::new(),
            leaf_std_cells: Vec::new(),
            leaf_macros: Vec::new(),
            children: Vec::new(),
            metrics: Metrics::default(),
            is_cluster_of_unplaced_io_pins: false,
            is_io_pad_cluster: false,
            is_io_bundle: false,
            is_cluster_of_unconstrained_io_pins: false,
            is_fixed_macro: false,
            num_io_pins: 0,
            constraint_region: None,
            tilings: Vec::new(),
        }
    }

    /// 🔑 **Type-masked.** A `HardMacroCluster` reports 0 standard cells whatever its metrics say,
    /// and a `StdCellCluster` reports 0 macros. Reading `metrics` directly instead of going
    /// through these would silently change every threshold comparison.
    pub fn num_std_cell(&self) -> i32 {
        if self.cluster_type == ClusterType::HardMacro {
            return 0;
        }
        self.metrics.num_std_cell
    }

    pub fn num_macro(&self) -> i32 {
        if self.cluster_type == ClusterType::StdCell {
            return 0;
        }
        self.metrics.num_macro
    }

    /// ⚠️ Areas are **not** type-masked upstream — only the counts are. A `HardMacroCluster`
    /// reports 0 standard cells and still reports their area, which is exactly why the dump
    /// prints a field when the count **or** the area is non-zero.
    pub fn std_cell_area(&self) -> i64 {
        self.metrics.std_cell_area
    }

    pub fn macro_area(&self) -> i64 {
        self.metrics.macro_area
    }

    /// Upstream `getClusterTypeString`. ⚠️ The IO and fixed-macro cases are checked **before**
    /// the ordinary type, and in this order.
    pub fn type_string(&self) -> &'static str {
        if self.is_io_bundle {
            return "IO Bundle";
        }
        if self.is_cluster_of_unconstrained_io_pins {
            return "Unconstrained IOs";
        }
        if self.is_cluster_of_unplaced_io_pins {
            return "Unplaced IOs";
        }
        if self.is_io_pad_cluster {
            return "IO Pad";
        }
        if self.is_fixed_macro {
            return "Fixed Macro";
        }
        match self.cluster_type {
            ClusterType::StdCell => "StdCell",
            ClusterType::Mixed => "Mixed",
            ClusterType::HardMacro => "Macro",
        }
    }

    /// Upstream `getIsLeafString`: `"Leaf"` for a childless non-IO cluster, otherwise empty.
    pub fn is_leaf_string(&self) -> &'static str {
        if !self.is_io_cluster() && self.children.is_empty() {
            "Leaf"
        } else {
            ""
        }
    }

    /// Nothing in it at all — no leaves and no modules.
    pub fn is_empty(&self) -> bool {
        self.leaf_std_cells.is_empty() && self.leaf_macros.is_empty() && self.db_modules.is_empty()
    }

    /// ⚠️ **Exactly one module and no loose leaves.** A cluster with one module *and* some glue
    /// instances does NOT correspond to a logical module, and takes the merged-cluster branch of
    /// `breakCluster` instead.
    pub fn corresponds_to_logical_module(&self) -> bool {
        self.leaf_std_cells.is_empty() && self.leaf_macros.is_empty() && self.db_modules.len() == 1
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    pub fn is_io_cluster(&self) -> bool {
        self.is_cluster_of_unplaced_io_pins || self.is_io_pad_cluster || self.is_io_bundle
    }

    pub fn release_children(&mut self) -> Vec<Cluster> {
        std::mem::take(&mut self.children)
    }

    pub fn add_children(&mut self, children: Vec<Cluster>) {
        self.children.extend(children);
    }
}

/// Does this cluster need breaking? Upstream `multilevelAutocluster` and `breakCluster`.
///
/// ⚠️ **`||`, not `&&`** — either count exceeding its maximum is enough. Compare
/// [`is_merge_candidate`], which needs BOTH to be small. Getting the two the same way round is a
/// single-character mistake that changes the whole tree shape.
pub fn should_break(cluster: &Cluster, max_std_cell: i32, max_macro: i32) -> bool {
    cluster.num_std_cell() > max_std_cell || cluster.num_macro() > max_macro
}

/// Is this a small child eligible for merging? Upstream `breakCluster`'s `small_children` sweep.
///
/// ⚠️ **`&&`, and IO clusters are excluded** — an IO cluster is never merged away however small.
pub fn is_merge_candidate(cluster: &Cluster, min_std_cell: i32, min_macro: i32) -> bool {
    !cluster.is_io_cluster()
        && cluster.num_std_cell() < min_std_cell
        && cluster.num_macro() < min_macro
}

/// Upstream `isLargeFlatCluster`: the gate on TritonPart partitioning.
///
/// 🔑 **This reads the LEAF VECTOR LENGTHS, not `num_std_cell()`/`num_macro()`.** Those are
/// metrics-based and type-masked; these are the actual instances hanging directly off the cluster.
/// A cluster typed `HardMacro` still reports its real standard-cell leaves here.
///
/// ⚠️ "Flat" means **no `dbModule` children** — the hierarchy has nothing left to split on, which
/// is exactly why upstream falls back to a hypergraph partitioner.
pub fn is_large_flat_cluster(cluster: &Cluster, max_std_cell: i32, max_macro: i32) -> bool {
    cluster.db_modules.is_empty()
        && (cluster.leaf_std_cells.len() as i64 > max_std_cell as i64
            || cluster.leaf_macros.len() as i64 > max_macro as i64)
}

/// What `update_sub_tree` did, and whether the caller must refuse.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubTreeUpdate {
    /// Ids of the intermediate clusters that were dissolved. Upstream erases these from
    /// `id_to_cluster`; the objects themselves are destroyed with their `unique_ptr`s.
    pub dissolved: Vec<ClusterId>,
    /// Ids of the resulting children that need TritonPart. ⛔ **Stage 1 has no partitioner, so a
    /// non-empty list here is a refusal, never an approximation.**
    pub needs_partitioning: Vec<ClusterId>,
}

/// Upstream `updateSubTree`: collapse the parent's whole subtree into a flat list of its leaves.
///
/// 🔑 **This is a LEVEL COLLAPSE, not a walk.** Every descendant that has children is dissolved
/// and its children promoted, breadth-first, until only leaves remain — and those all become
/// direct children of `parent`. The intermediate clusters cease to exist.
///
/// ⚠️ **The queue is FIFO** (upstream uses `std::queue`), so the resulting child order is
/// breadth-first. That order survives into the annealer's sequence pair and therefore into macro
/// positions, so a LIFO stack here would be a different algorithm producing a different layout.
pub fn update_sub_tree(parent: &mut Cluster, max_std_cell: i32, max_macro: i32) -> SubTreeUpdate {
    let mut leaves: Vec<Cluster> = Vec::new();
    let mut dissolved: Vec<ClusterId> = Vec::new();
    let mut wavefront: VecDeque<Cluster> = parent.release_children().into();

    while let Some(mut cluster) = wavefront.pop_front() {
        if cluster.children.is_empty() {
            leaves.push(cluster);
        } else {
            for child in cluster.release_children() {
                wavefront.push_back(child);
            }
            dissolved.push(cluster.id);
        }
    }

    parent.add_children(leaves);

    // Upstream then re-parents each new child and partitions any that is a large flat cluster.
    // We collect them instead: stage 1 refuses rather than approximating.
    let needs_partitioning = parent
        .children
        .iter()
        .filter(|c| is_large_flat_cluster(c, max_std_cell, max_macro))
        .map(|c| c.id)
        .collect();

    SubTreeUpdate { dissolved, needs_partitioning }
}
