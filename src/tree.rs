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
            m.std_cell_area += self.design.instances[i].bbox.area();
        }
        for &i in &cluster.leaf_macros {
            m.num_macro += 1;
            m.macro_area += self.design.instances[i].bbox.area();
        }
        for &module in &cluster.db_modules {
            m.num_std_cell += self.module_metrics[module].num_std_cell;
            m.num_macro += self.module_metrics[module].num_macro;
            m.std_cell_area += self.module_metrics[module].std_cell_area;
            m.macro_area += self.module_metrics[module].macro_area;
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
            c.metrics = crate::cluster::Metrics {
                num_std_cell: 0,
                num_macro: 1,
                std_cell_area: 0,
                macro_area: inst.bbox.area(),
            };
            out.push(c);
        }
        out
    }

    pub fn next_id(&self) -> ClusterId {
        self.next_id
    }

    /// Resume id allocation from `id`. ⚠️ Used after IO clusters have taken their ids, so macro
    /// clusters continue the sequence rather than colliding with them.
    pub fn with_next_id(mut self, id: ClusterId) -> Self {
        self.next_id = id;
        self
    }
}

fn to_cluster_metrics(m: ModuleMetrics) -> crate::cluster::Metrics {
    crate::cluster::Metrics {
        num_std_cell: m.num_std_cell,
        num_macro: m.num_macro,
        std_cell_area: m.std_cell_area,
        macro_area: m.macro_area,
    }
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

/// What one `multilevel_autocluster` descent produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutoclusterOutcome {
    /// Clusters that need TritonPart. ⛔ **Stage 1 refuses on a non-empty list.**
    pub needs_partitioning: Vec<ClusterId>,
    /// Merge candidates reported per parent, in visit order.
    pub merge_candidates: Vec<(ClusterId, Vec<ClusterId>)>,
    /// Cluster ids dissolved by the level collapse, so the id map can be pruned.
    pub dissolved: Vec<ClusterId>,
}

impl AutoclusterOutcome {
    fn absorb(&mut self, other: AutoclusterOutcome) {
        self.needs_partitioning.extend(other.needs_partitioning);
        self.merge_candidates.extend(other.merge_candidates);
        self.dissolved.extend(other.dissolved);
    }
}

impl TreeBuilder<'_> {
    /// Upstream `ClusteringEngine::multilevelAutocluster`, the descent that drives `breakCluster`.
    ///
    /// 🔑 **The `else` branch recurses on the SAME cluster with the level incremented — not on its
    /// children.** That is how the walk descends a level without splitting anything, and it
    /// terminates only because `level >= max_level` returns. Recursing on the children there — the
    /// obvious reading — would skip levels entirely and build a different tree.
    ///
    /// ⚠️ **`force_split_root` is computed only at level 0**, and against the LEAF maximum
    /// (`base_max_std_cell / ratio^(max_level - 1)`), not the current level's. A root already
    /// smaller than a leaf is split anyway, because leaving it whole hands the placer one cluster.
    #[allow(clippy::too_many_arguments)]
    pub fn multilevel_autocluster(
        &mut self,
        parent: &mut Cluster,
        is_root: bool,
        level: i32,
        base: crate::thresholds::Thresholds,
        max_level: i32,
        cluster_size_ratio: f32,
    ) -> AutoclusterOutcome {
        let mut outcome = AutoclusterOutcome::default();

        // Only at the top, and against the LEAF maximum rather than this level's.
        let force_split_root = if level == 0 {
            let leaf_max_std_cell =
                (base.max_std_cell as f64 / (cluster_size_ratio as f64).powi(max_level - 1)) as i32;
            parent.num_std_cell() < leaf_max_std_cell
        } else {
            false
        };

        if level >= max_level {
            return outcome;
        }

        // ⚠️ The level is incremented BEFORE the thresholds are computed, so the first descent
        // already uses level 1's thresholds rather than the base ones.
        let level = level + 1;
        let t = crate::thresholds::update_size_thresholds(base, level, cluster_size_ratio);

        if force_split_root || crate::cluster::should_break(parent, t.max_std_cell, t.max_macro) {
            let breaks = self.break_cluster(
                parent,
                is_root,
                t.max_std_cell,
                t.max_macro,
                t.min_std_cell,
                t.min_macro,
            );
            if !breaks.merge_candidates.is_empty() {
                outcome.merge_candidates.push((parent.id, breaks.merge_candidates));
            }

            let sub = crate::cluster::update_sub_tree(parent, t.max_std_cell, t.max_macro);
            outcome.dissolved.extend(sub.dissolved);
            outcome.needs_partitioning.extend(sub.needs_partitioning);

            for child in &mut parent.children {
                // ⚠️ Children are never the root, whatever the parent was.
                let child_outcome = self.multilevel_autocluster(
                    child,
                    false,
                    level,
                    base,
                    max_level,
                    cluster_size_ratio,
                );
                outcome.absorb(child_outcome);
            }
        } else {
            // 🔑 The same cluster, one level down. NOT its children.
            let same = self.multilevel_autocluster(
                parent,
                is_root,
                level,
                base,
                max_level,
                cluster_size_ratio,
            );
            outcome.absorb(same);
        }

        outcome
    }
}

/// One group of sibling mixed leaves, as `fetchMixedLeaves` collects them.
pub type MixedLeafGroups = Vec<Vec<ClusterId>>;

/// Upstream `fetchMixedLeaves`, with the retyping it performs on the way.
///
/// 🔑 **It MUTATES as it walks**: any child holding no macros is retyped `StdCellCluster` before
/// the leaf test, so the retyping decides which leaves are collected. Reading it as a pure search
/// would gather clusters upstream has already reclassified out of the way.
///
/// ⚠️ **Groups are pushed even when EMPTY**, and one group corresponds to one parent's children —
/// which is what makes the later merge sweep per-parent rather than global.
pub fn fetch_mixed_leaves(parent: &mut Cluster) -> MixedLeafGroups {
    let mut groups = Vec::new();
    let mut sisters = Vec::new();

    for child in &mut parent.children {
        if child.num_macro() == 0 {
            child.cluster_type = ClusterType::StdCell;
        }
        if child.children.is_empty() {
            if child.cluster_type != ClusterType::StdCell {
                sisters.push(child.id);
            }
        } else {
            groups.extend(fetch_mixed_leaves(child));
        }
    }

    groups.push(sisters);
    groups
}

/// Every macro a cluster owns: its own leaf macros, plus those inside any module it holds.
///
/// ⚠️ Upstream's `mapMacroInCluster2HardMacro` descends the module tree, so a cluster that holds
/// a module holds that module's macros too — counting only `leaf_macros` misses them entirely.
pub fn macros_of(cluster: &Cluster, design: &crate::design::Design) -> Vec<usize> {
    let mut out = cluster.leaf_macros.clone();
    for &m in &cluster.db_modules {
        hard_macros_of_module(m, design, &mut out);
    }
    out
}

/// Upstream `getHardMacros`: a module's OWN block instances first, in declaration order, then each
/// child module in order — depth first.
///
/// ⚠️ The order is not incidental: `createOneClusterForEachMacro` hands out cluster ids in exactly
/// this sequence, so any other traversal renumbers the whole tree.
fn hard_macros_of_module(
    module: usize,
    design: &crate::design::Design,
    out: &mut Vec<usize>,
) {
    for &i in &design.modules[module].insts {
        let inst = &design.instances[i];
        if crate::design::is_ignored_inst(inst) {
            continue;
        }
        if inst.is_block {
            out.push(i);
        }
    }
    for &c in &design.modules[module].children {
        hard_macros_of_module(c, design, out);
    }
}

/// Apply `breakMixedLeaf` to every mixed leaf under `root`.
///
/// 🔑 **Movable macro clusters go to the mixed leaf's PARENT; fixed ones go to the ROOT.** That is
/// the structural rule, and it is why this cannot be a simple in-place edit of one subtree.
///
/// Returns the virtual connections the split created.
pub fn split_mixed_leaves(
    root: &mut Cluster,
    ctx: &mut SplitCtx,
    next_id: &mut ClusterId,
) -> Vec<(ClusterId, ClusterId)> {
    // The association the descent left behind is the starting point; each split updates it.
    ctx.assoc = associate_instances(root, ctx.design);
    // ⚠️ An IO pad's cluster holds no leaf instances, so the tree walk cannot find it. Upstream
    // writes the pad into the map directly when it creates the cluster; without this the pad's
    // nets would look unconnected and the macros beside them would classify differently.
    for &(inst, id) in &ctx.seed_assoc {
        ctx.assoc[inst] = Some(id);
    }
    let mut to_root: Vec<Cluster> = Vec::new();
    let mut virtual_connections = Vec::new();
    split_recursive(root, true, ctx, next_id, &mut to_root, &mut virtual_connections);
    virtual_connections
}

/// Everything `breakMixedLeaf` needs to rebuild the cluster-level connections after each split.
///
/// ⚠️ Upstream rebuilds them **per mixed leaf** (`clearConnections(); buildNetListConnections()`),
/// not once for the whole pass — the classification of one leaf's macros sees the clusters the
/// previous leaf created. Building them once would classify against a stale netlist.
pub struct SplitCtx<'a> {
    pub design: &'a crate::design::Design,
    pub nets: &'a [crate::netlist::DbNet],
    pub bterm_to_cluster: Vec<Option<ClusterId>>,
    pub design_has_io_pads: bool,
    pub large_net_threshold: usize,
    /// `(instance, cluster)` pairs no tree walk can recover — today, the IO pads.
    pub seed_assoc: Vec<(usize, ClusterId)>,
    pub assoc: Vec<Option<ClusterId>>,
}

impl SplitCtx<'_> {
    fn connections(&self) -> crate::netlist::Connections {
        let assoc = &self.assoc;
        let bterm = &self.bterm_to_cluster;
        crate::netlist::build_connections(
            self.nets,
            self.design,
            &|i| assoc.get(i).copied().flatten(),
            &|b| bterm.get(b).copied().flatten(),
            self.design_has_io_pads,
            self.large_net_threshold,
        )
    }
}

fn split_recursive(
    parent: &mut Cluster,
    is_root: bool,
    ctx: &mut SplitCtx,
    next_id: &mut ClusterId,
    to_root: &mut Vec<Cluster>,
    virtual_connections: &mut Vec<(ClusterId, ClusterId)>,
) {
    // Post-order, matching `fetchMixedLeaves`: a parent's own leaves are broken only after every
    // deeper parent's have been, so the ids come out in upstream's sequence.
    for child in &mut parent.children {
        if !child.children.is_empty() {
            split_recursive(child, false, ctx, next_id, to_root, virtual_connections);
        }
    }

    let mut new_siblings: Vec<Cluster> = Vec::new();
    let mut new_virtual: Vec<(ClusterId, ClusterId)> = Vec::new();
    for child in &mut parent.children {
        if !child.children.is_empty() || child.cluster_type == ClusterType::StdCell {
            continue;
        }
        let macro_insts = macros_of(child, ctx.design);
        if macro_insts.is_empty() {
            continue;
        }

        // `createOneClusterForEachMacro`: one cluster per macro, named after the instance, ids
        // handed out in `mapMacroInCluster2HardMacro` order.
        let mut macro_clusters = Vec::new();
        let mut descriptors = Vec::new();
        for &i in &macro_insts {
            let inst = &ctx.design.instances[i];
            let mut c = Cluster::new(*next_id, inst.name.clone());
            c.leaf_macros.push(i);
            c.cluster_type = ClusterType::HardMacro;
            c.metrics = crate::cluster::Metrics {
                num_std_cell: 0,
                num_macro: 1,
                std_cell_area: 0,
                macro_area: inst.bbox.area(),
            };
            c.is_fixed_macro = inst.is_fixed;
            descriptors.push(crate::macroclass::MacroCluster {
                id: *next_id,
                name: inst.name.clone(),
                inst: i,
                is_fixed: inst.is_fixed,
                size: crate::macroclass::MacroSize {
                    width: inst.bbox.x_max - inst.bbox.x_min,
                    height: inst.bbox.y_max - inst.bbox.y_min,
                },
            });
            macro_clusters.push(c);
            *next_id += 1;
        }

        // `incorporateNewCluster` associates each macro with its own new cluster BEFORE the
        // netlist connections are rebuilt — that is what makes the signature classification see
        // per-macro clusters rather than the mixed leaf.
        for d in &descriptors {
            ctx.assoc[d.inst] = Some(d.id);
        }
        let conns = ctx.connections();
        let plan = crate::macroclass::break_mixed_leaf(child.id, &descriptors, &conns);
        virtual_connections.extend(plan.virtual_connections.iter().copied());
        // ⚠️ `parent` HERE is upstream's `mixed_leaf->getParent()` — the leaf being broken is
        // `child`. Storing them on `child` would put them under the std-cell cluster, where
        // `buildBundledNets` never looks.
        new_virtual.extend(plan.virtual_connections.iter().copied());

        // `replaceByStdCellCluster`: the leaf keeps its standard cells and becomes one.
        child.leaf_macros.clear();
        child.cluster_type = ClusterType::StdCell;
        child.metrics.num_macro = 0;
        child.metrics.macro_area = 0;

        // ⚠️ Only the leaders survive. `attemptMerge` DESTROYS the cluster it absorbed, so a
        // merged macro cluster is not in the tree at all — keeping it would print an extra line.
        let survivors: Vec<ClusterId> = plan
            .arrays
            .iter()
            .map(|a| a.id)
            .chain(plan.fixed_clusters.iter().copied())
            .collect();
        // A merged macro follows its leader: `attemptMerge` moved its instance there.
        for a in &plan.arrays {
            for &m in &a.members {
                if let Some(d) = descriptors.iter().find(|d| d.id == m) {
                    ctx.assoc[d.inst] = Some(a.id);
                }
            }
        }
        for mut c in macro_clusters {
            if !survivors.contains(&c.id) {
                continue;
            }
            // ⚠️ `attemptMerge` moves the absorbed cluster's macros into the leader, and
            // `setClusterMetrics` then recounts. A leader that kept only its own macro would
            // print `Macros: 1` for a group of four.
            if let Some(a) = plan.arrays.iter().find(|a| a.id == c.id) {
                c.leaf_macros.clear();
                c.metrics = crate::cluster::Metrics::default();
                // 🔑 `Cluster::attemptMerge` does `name_ += "||" + incomer->name_`, so the
                // leader's printed name carries every macro it absorbed, in merge order.
                c.name = a
                    .members
                    .iter()
                    .filter_map(|m| descriptors.iter().find(|d| d.id == *m))
                    .map(|d| d.name.clone())
                    .collect::<Vec<_>>()
                    .join("||");
                for &m in &a.members {
                    let Some(d) = descriptors.iter().find(|d| d.id == m) else { continue };
                    c.leaf_macros.push(d.inst);
                    c.metrics.num_macro += 1;
                    c.metrics.macro_area += ctx.design.instances[d.inst].bbox.area();
                }
            }
            if c.is_fixed_macro && !is_root {
                to_root.push(c);
            } else {
                new_siblings.push(c);
            }
        }
    }

    // At the root, the fixed macros collected from deeper levels were incorporated before the
    // root's own leaves were broken — so they go in first.
    if is_root {
        parent.children.append(to_root);
    }
    parent.children.extend(new_siblings);
    parent.virtual_connections.extend(new_virtual);
}

/// Upstream `updateInstancesAssociation`, applied to a whole tree.
///
/// 🔑 The type decides what a cluster claims, and the two halves are independent:
/// a **HardMacro or Mixed** cluster claims its leaf macros; a **StdCell or Mixed** cluster claims
/// its leaf standard cells. Its modules are walked only for StdCell (macros excluded) and Mixed
/// (macros included) — ℹ️ *macro clusters have no module*.
pub fn associate_instances(root: &Cluster, design: &crate::design::Design) -> Vec<Option<ClusterId>> {
    let mut assoc = vec![None; design.instances.len()];
    associate_cluster(root, design, &mut assoc);
    assoc
}

fn associate_cluster(
    cluster: &Cluster,
    design: &crate::design::Design,
    assoc: &mut [Option<ClusterId>],
) {
    let t = cluster.cluster_type;
    if t == ClusterType::HardMacro || t == ClusterType::Mixed {
        for &i in &cluster.leaf_macros {
            if !crate::design::is_ignored_inst(&design.instances[i]) {
                assoc[i] = Some(cluster.id);
            }
        }
    }
    if t == ClusterType::StdCell || t == ClusterType::Mixed {
        for &i in &cluster.leaf_std_cells {
            if !crate::design::is_ignored_inst(&design.instances[i]) {
                assoc[i] = Some(cluster.id);
            }
        }
    }
    if t == ClusterType::StdCell || t == ClusterType::Mixed {
        let include_macro = t == ClusterType::Mixed;
        for &m in &cluster.db_modules {
            associate_module(m, cluster.id, include_macro, design, assoc);
        }
    }
    for child in &cluster.children {
        associate_cluster(child, design, assoc);
    }
}

fn associate_module(
    module: usize,
    id: ClusterId,
    include_macro: bool,
    design: &crate::design::Design,
    assoc: &mut [Option<ClusterId>],
) {
    for &i in &design.modules[module].insts {
        let inst = &design.instances[i];
        if crate::design::is_ignored_inst(inst) {
            continue;
        }
        // ⚠️ `include_macro` false SKIPS block instances — it does not merely avoid overwriting
        // them. A std-cell cluster never claims a macro, even one sitting in its module.
        if !include_macro && inst.is_block {
            continue;
        }
        assoc[i] = Some(id);
    }
    for &c in &design.modules[module].children {
        associate_module(c, id, include_macro, design, assoc);
    }
}
