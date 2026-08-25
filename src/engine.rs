// SPDX-License-Identifier: Apache-2.0
//! `ClusteringEngine::run` — the clustering stage, composed.
//!
//! 🔑 **This is the first thing in the crate that can be compared against upstream's own output.**
//! Everything below it was checked against rules transcribed from the source; this is checked
//! against what upstream actually prints, on real designs.

use crate::cluster::{Cluster, ClusterId, ClusterType};
use crate::design::{compute_module_metrics, is_ignored_inst, Design, Rect};
use crate::ioclusters::{create_io_clusters, IoClusters, Pin};
use crate::status::Status;
use crate::thresholds::{set_base_thresholds, DesignMetrics, Thresholds};
use crate::tree::TreeBuilder;

/// Why clustering declined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// ⛔ Stage 1 has no partitioner. Names the clusters that needed one.
    NeedsPartitioner(Vec<ClusterId>),
    /// A design the engine cannot process at all.
    Error(String),
    /// ⏸️ A path that exists upstream and is not built here yet. **Refused, never approximated** —
    /// silently producing a different tree is the failure this whole programme is guarding against.
    NotImplemented(&'static str),
}

/// The outcome of clustering.
#[derive(Debug, Clone, PartialEq)]
pub struct Clustering {
    pub root: Cluster,
    pub status: Status,
    pub refusal: Option<Refusal>,
}

/// Everything the stage takes from the command line.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterOptions {
    pub thresholds: Thresholds,
    pub max_num_level: i32,
    pub cluster_size_ratio: f32,
    pub global_fence: Option<Rect>,
    pub large_net_threshold: usize,
}

impl Default for ClusterOptions {
    fn default() -> Self {
        let d = crate::options::PlacerOptions::default();
        Self {
            thresholds: Thresholds {
                max_macro: d.max_num_macro as i32,
                min_macro: d.min_num_macro as i32,
                max_std_cell: d.max_num_inst as i32,
                min_std_cell: d.min_num_inst as i32,
            },
            max_num_level: d.max_num_level as i32,
            cluster_size_ratio: d.coarsening_ratio as f32,
            global_fence: None,
            large_net_threshold: d.large_net_threshold as usize,
        }
    }
}

/// Upstream `ClusteringEngine::run`.
///
/// The order is the algorithm, and one step of it is visible in upstream's own output:
/// **IO clusters are created BEFORE the macro clusters**, so they take the lower ids —
/// `ios_1` precedes `MACRO_4` in a real dump.
pub fn run_clustering(
    design: &Design,
    pins: &[Pin],
    nets: &[crate::netlist::DbNet],
    opts: &ClusterOptions,
) -> Clustering {
    // ---- init
    let Some(placement_area) =
        crate::design::floorplan_shape(&design.core_area, opts.global_fence.as_ref())
    else {
        return refuse(Refusal::Error("the global fence is outside the core area".into()));
    };

    let mut errors = Vec::new();
    let design_metrics = compute_module_metrics(design, design.top, &placement_area, &mut errors);
    if let Some(e) = errors.first() {
        return refuse(Refusal::Error(format!(
            "fixed non-macro instance {} inside the macro placement area",
            e.name
        )));
    }

    // ⚠️ No unfixed macros means the run does nothing. That is `vacuous`, never `applied`.
    if crate::design::unfixed_macros(design).is_empty() {
        return Clustering {
            root: Cluster::new(0, "root"),
            status: Status::Vacuous,
            refusal: None,
        };
    }

    // ---- createRoot, then the thresholds it needs
    let module_metrics: Vec<_> = (0..design.modules.len())
        .map(|m| compute_module_metrics(design, m, &placement_area, &mut Vec::new()))
        .collect();
    let mut builder = TreeBuilder::new(design, module_metrics);
    let mut root = builder.create_root();

    let has_fixed_macros = design
        .instances
        .iter()
        .any(|i| i.is_block && i.is_fixed && !i.is_ignorable_macro);

    let base = set_base_thresholds(
        opts.thresholds,
        DesignMetrics {
            num_macro: design_metrics.num_macro,
            num_std_cell: design_metrics.num_std_cell,
        },
        opts.cluster_size_ratio,
        opts.max_num_level,
        has_fixed_macros,
    );

    // ---- IO clusters, BEFORE any macro cluster, so they take the lower ids
    let pads = crate::design::io_pads(design);
    let io_first_id = builder.next_id();
    let io = if pads.is_empty() {
        create_io_clusters(pins, &design.die_area, builder.next_id())
    } else {
        crate::ioclusters::create_io_pad_clusters(&pads, design, builder.next_id())
    };
    let mut builder = builder.with_next_id(io.next_id);
    for c in io.bundles.iter().chain(io.pin_clusters.iter()) {
        root.children.push(c.clone());
    }

    // ---- the split that decides everything after it
    if design_metrics.num_std_cell == 0 {
        // Upstream warns MPL-25 and gives every macro its own cluster.
        for c in builder.one_cluster_per_macro() {
            root.children.push(c);
        }
        return Clustering { root, status: Status::Applied, refusal: None };
    }

    // ---- the mixed path: descend the levels, then split the mixed leaves
    let out = builder.multilevel_autocluster(
        &mut root,
        true,
        0,
        base.thresholds,
        base.max_level,
        opts.cluster_size_ratio,
    );

    // ⛔ A flat cluster needing TritonPart. Refused, never approximated.
    if !out.needs_partitioning.is_empty() {
        return refuse(Refusal::NeedsPartitioner(out.needs_partitioning));
    }

    // `fetchMixedLeaves` retypes as it walks, so it must run before anything reads the types.
    // Its groups are not needed here: `split_mixed_leaves` walks the same post-order itself, and
    // one traversal that both collects and breaks cannot disagree with itself about the order.
    let _groups = crate::tree::fetch_mixed_leaves(&mut root);

    // Block ports reach the netlist through the IO cluster they were assigned to.
    let mut bterm_to_cluster = vec![None; pins.len()];
    for &(pin, cluster) in &io.assignment {
        bterm_to_cluster[pin] = Some(cluster);
    }

    let mut ctx = crate::tree::SplitCtx {
        design,
        nets,
        bterm_to_cluster,
        // ⚠️ With pads present the pads carry the connectivity and block ports are ignored.
        design_has_io_pads: !pads.is_empty(),
        large_net_threshold: opts.large_net_threshold,
        seed_assoc: pads.iter().copied().zip(io_first_id..).collect(),
        assoc: Vec::new(),
    };
    let mut next_id = builder.next_id();
    let _virtual = crate::tree::split_mixed_leaves(&mut root, &mut ctx, &mut next_id);

    let _ = (&base, is_ignored_inst, ClusterType::Mixed, IoClusters::default);
    Clustering { root, status: Status::Applied, refusal: None }
}

fn refuse(r: Refusal) -> Clustering {
    Clustering { root: Cluster::new(0, "root"), status: Status::Refused, refusal: Some(r) }
}
