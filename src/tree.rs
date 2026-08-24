// SPDX-License-Identifier: Apache-2.0
//! Building the physical hierarchy from the logical one.
//!
//! Upstream: `ClusteringEngine::createRoot`, `createCluster` (both overloads), `createFlatCluster`,
//! `addModuleLeafInstsToCluster`, `setClusterMetrics`, `incorporateNewCluster`.
//!
//! 🔑 **A cluster is not a module.** A module that contributes nothing gets no cluster at all, and
//! the instances a module owns directly become a separate *glue logic* cluster rather than staying
//! with the module's children. Both rules change the tree's shape, and therefore the placement.

use crate::cluster::{Cluster, ClusterId, ClusterType};
use crate::design::{is_ignored_inst, Design, ModuleMetrics};

/// Builds the tree, handing out ids in creation order.
///
/// ⚠️ **Ids are assigned on incorporation, in creation order**, and that order is observable: it
/// decides tie-breaks and the sequence the annealer later sees. Creating clusters in a different
/// order is a different algorithm.
pub struct TreeBuilder<'a> {
    design: &'a Design,
    /// Metrics per module index, computed once.
    module_metrics: Vec<ModuleMetrics>,
    next_id: ClusterId,
}

impl<'a> TreeBuilder<'a> {
    pub fn new(design: &'a Design, module_metrics: Vec<ModuleMetrics>) -> Self {
        Self { design, module_metrics, next_id: 0 }
    }

    fn take_id(&mut self) -> ClusterId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// The root: owns the top module and nothing else.
    ///
    /// Upstream also maps every instance in the block to the root's id here. That mapping is
    /// rebuilt by `updateInstancesAssociation` at every step, so it is not carried in the tree.
    pub fn create_root(&mut self) -> Cluster {
        let id = self.take_id();
        let mut root = Cluster::new(id, "root");
        root.db_modules.push(self.design.top);
        root.metrics = to_cluster_metrics(self.module_metrics[self.design.top]);
        root
    }

    /// One cluster for a child module.
    ///
    /// ⚠️ **A module whose metrics are empty gets NO cluster.** "Empty" counts instances only —
    /// a module holding nothing but ignored cells (pads, covers, end-caps) has zero counts and is
    /// skipped, even though its area is not zero.
    pub fn create_cluster_for_module(&mut self, module: usize) -> Option<Cluster> {
        if self.module_metrics[module].num_macro == 0
            && self.module_metrics[module].num_std_cell == 0
        {
            return None;
        }
        let id = self.take_id();
        // 🔑 Named by the module's HIERARCHICAL name, so two modules with the same leaf name in
        // different branches do not collide.
        let mut c = Cluster::new(id, self.design.modules[module].hierarchical_name.clone());
        c.db_modules.push(module);
        c.metrics = to_cluster_metrics(self.module_metrics[module]);
        Some(c)
    }

    /// The *glue logic* cluster: a module's own instances, separated from its child modules.
    ///
    /// 🔑 Named `(<parent name>)_glue_logic` — the parentheses are upstream's and they nest, so a
    /// deep hierarchy produces names like `((root)_glue_logic_0_1)_glue_logic_1_0`.
    ///
    /// ⚠️ **Returns `None` when the module owns no unignored instances.** Upstream builds the
    /// cluster, finds it empty and drops it. A cluster that exists with no leaves would take an
    /// id, occupy a slot in the tree and contribute nothing.
    pub fn create_flat_cluster(&mut self, module: usize, parent_name: &str) -> Option<Cluster> {
        let mut c = Cluster::new(0, format!("({parent_name})_glue_logic"));
        self.add_module_leaf_insts(&mut c, module);
        if c.leaf_std_cells.is_empty() && c.leaf_macros.is_empty() {
            return None;
        }
        c.id = self.take_id();
        self.set_cluster_metrics(&mut c);
        Some(c)
    }

    /// The other glue overload: move a cluster's OWN leaves into a child of itself.
    ///
    /// ⚠️ Unlike [`create_flat_cluster`], this one is created unconditionally — upstream calls it
    /// only after checking the parent has leaves.
    pub fn create_glue_from_leaves(&mut self, parent: &Cluster) -> Cluster {
        let id = self.take_id();
        let mut c = Cluster::new(id, format!("({})_glue_logic", parent.name));
        c.leaf_std_cells = parent.leaf_std_cells.clone();
        c.leaf_macros = parent.leaf_macros.clone();
        self.set_cluster_metrics(&mut c);
        c
    }

    /// Add a module's directly-owned instances as leaves.
    ///
    /// 🔑 Ignored instances are skipped, and everything else is filed by `is_block`: a macro goes
    /// to `leaf_macros`, anything else to `leaf_std_cells`. ⚠️ **Only the module's OWN instances** —
    /// this does not descend into child modules, which get their own clusters.
    pub fn add_module_leaf_insts(&self, cluster: &mut Cluster, module: usize) {
        for &i in &self.design.modules[module].insts {
            let inst = &self.design.instances[i];
            if is_ignored_inst(inst) {
                continue;
            }
            if inst.is_block {
                cluster.leaf_macros.push(i);
            } else {
                cluster.leaf_std_cells.push(i);
            }
        }
    }

    /// A cluster's metrics: its own leaves, **plus** the metrics of every module it holds.
    ///
    /// ⚠️ Both parts, not either. A cluster can hold loose leaves and a module at once, and
    /// counting only one of them understates it.
    pub fn set_cluster_metrics(&self, cluster: &mut Cluster) {
        let mut m = crate::cluster::Metrics::default();
        for &i in &cluster.leaf_std_cells {
            m.num_std_cell += 1;
            let _ = self.design.instances[i].bbox.area();
        }
        m.num_macro += cluster.leaf_macros.len() as i32;
        for &module in &cluster.db_modules {
            m.num_std_cell += self.module_metrics[module].num_std_cell;
            m.num_macro += self.module_metrics[module].num_macro;
        }
        cluster.metrics = m;
    }

    /// Every macro becomes its own cluster. Upstream `treatEachMacroAsSingleCluster`, used when
    /// the design has no standard cells at all.
    ///
    /// ⚠️ Walks the **top module's** instances, and skips ignored ones.
    pub fn one_cluster_per_macro(&mut self) -> Vec<Cluster> {
        let mut out = Vec::new();
        for &i in &self.design.modules[self.design.top].insts {
            let inst = &self.design.instances[i];
            if is_ignored_inst(inst) || !inst.is_block {
                continue;
            }
            let id = self.take_id();
            let mut c = Cluster::new(id, inst.name.clone());
            c.leaf_macros.push(i);
            // 🔑 Typed HardMacro here, which MASKS its standard-cell count to zero downstream.
            c.cluster_type = ClusterType::HardMacro;
            c.metrics = crate::cluster::Metrics { num_std_cell: 0, num_macro: 1 };
            out.push(c);
        }
        out
    }

    pub fn next_id(&self) -> ClusterId {
        self.next_id
    }
}

fn to_cluster_metrics(m: ModuleMetrics) -> crate::cluster::Metrics {
    crate::cluster::Metrics { num_std_cell: m.num_std_cell, num_macro: m.num_macro }
}

/// Metrics for every module, indexed by module.
pub fn all_module_metrics(
    design: &Design,
    placement_area: &crate::design::Rect,
) -> (Vec<ModuleMetrics>, Vec<crate::design::FixedInstanceInArea>) {
    let mut errors = Vec::new();
    let metrics = (0..design.modules.len())
        .map(|m| crate::design::compute_module_metrics(design, m, placement_area, &mut Vec::new()))
        .collect();
    // Errors are collected once, from the top, so a shared submodule is not reported twice.
    crate::design::compute_module_metrics(design, design.top, placement_area, &mut errors);
    (metrics, errors)
}

/// What `break_cluster` left for the caller to deal with.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BreakOutcome {
    /// Children small enough to be merge candidates, by id, in the order upstream collects them.
    ///
    /// ⚠️ Identified but **not merged** — merging consults net connectivity, which is a separate
    /// stage. Reporting them keeps the decision visible rather than silently skipped.
    pub merge_candidates: Vec<ClusterId>,
}

impl TreeBuilder<'_> {
    /// Upstream `ClusteringEngine::breakCluster`: split one cluster into children.
    ///
    /// ⚠️ **`is_root` is a real behavioural switch, not bookkeeping.** A flat module at the root
    /// gets a glue-logic CHILD; the same flat module anywhere else is absorbed INTO its own
    /// cluster, which then stops being a module cluster at all. Same input, different tree.
    pub fn break_cluster(
        &mut self,
        parent: &mut Cluster,
        is_root: bool,
        max_std_cell: i32,
        max_macro: i32,
        min_std_cell: i32,
        min_macro: i32,
    ) -> BreakOutcome {
        // Nothing to split.
        //
        // ℹ️ **An optimisation, not a behaviour** — and mutation testing proved it. `is_empty`
        // means no leaves AND no modules, so the logical-module branch cannot match (it needs
        // exactly one module) and the merged branch has nothing to iterate and no leaves to turn
        // into glue. Removing this early return changes no output, here or upstream. It is kept
        // because upstream has it, and flagged so nobody writes a test that cannot fail.
        if parent.is_empty() {
            return BreakOutcome::default();
        }

        if parent.corresponds_to_logical_module() {
            let module = parent.db_modules[0];

            // A module with no child modules: the hierarchy has nothing left to split on.
            if self.design.modules[module].children.is_empty() {
                if is_root {
                    // 🔑 At the root the glue becomes a CHILD, so the root keeps its module and
                    // gains one cluster holding the instances.
                    let name = parent.name.clone();
                    if let Some(c) = self.create_flat_cluster(module, &name) {
                        parent.children.push(c);
                    }
                } else {
                    // 🔑 Anywhere else the instances are absorbed INTO this cluster and the
                    // module reference is dropped -- it becomes a leaf-holding cluster, which is
                    // what later makes it a candidate for flat partitioning.
                    self.add_module_leaf_insts(parent, module);
                    parent.db_modules.clear();
                }
                return BreakOutcome::default();
            }

            // Otherwise: one cluster per child module, THEN the parent's own instances as glue.
            // ⚠️ The order matters -- the glue cluster's id follows the child modules'.
            for i in 0..self.design.modules[module].children.len() {
                let child_module = self.design.modules[module].children[i];
                if let Some(c) = self.create_cluster_for_module(child_module) {
                    parent.children.push(c);
                }
            }
            let name = parent.name.clone();
            if let Some(c) = self.create_flat_cluster(module, &name) {
                parent.children.push(c);
            }
        } else {
            // A cluster built by merging: it may hold several modules and loose instances.
            for i in 0..parent.db_modules.len() {
                let module = parent.db_modules[i];
                if let Some(c) = self.create_cluster_for_module(module) {
                    parent.children.push(c);
                }
            }
            // ⚠️ The parent's leaves are COPIED into a glue child and deliberately not cleared
            // here; the instance-to-cluster mapping is rebuilt afterwards and settles ownership.
            if !parent.leaf_std_cells.is_empty() || !parent.leaf_macros.is_empty() {
                let glue = self.create_glue_from_leaves(parent);
                parent.children.push(glue);
            }
        }

        // Recurse into children that still hold a module AND are too big.
        //
        // ⛔ **The module check is what makes this recursion TERMINATE.** It reads like a mere
        // "nothing to split on", and it is not: a cluster with no module takes the merged branch
        // below, which copies its own leaves into a new glue child — a child with the same leaves,
        // no module, and the same size. Recursing into that regenerates it forever.
        //
        // Measured: removing this condition overflows the stack. A child with no module is left
        // for flat partitioning, which is also exactly where stage 1 refuses.
        // ⚠️ So it is skipped **however large it is** — size is not the deciding factor here.
        for child in &mut parent.children {
            if !child.db_modules.is_empty()
                && crate::cluster::should_break(child, max_std_cell, max_macro)
            {
                self.break_cluster(child, false, max_std_cell, max_macro, min_std_cell, min_macro);
            }
        }

        // Collect the small ones, in child order.
        BreakOutcome {
            merge_candidates: parent
                .children
                .iter()
                .filter(|c| crate::cluster::is_merge_candidate(c, min_std_cell, min_macro))
                .map(|c| c.id)
                .collect(),
        }
    }
}
