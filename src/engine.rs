// SPDX-License-Identifier: Apache-2.0
//! `ClusteringEngine::run` — the clustering stage, composed.
//!
//! 🔑 **This is the first thing in the crate that can be compared against upstream's own output.**
//! Everything below it was checked against rules transcribed from the source; this is checked
//! against what upstream actually prints, on real designs.

use crate::cluster::{Cluster, ClusterId, ClusterType};
use crate::design::{compute_module_metrics, is_ignored_inst, Design, Rect};
use crate::ioclusters::{create_io_clusters, IoClusters, Pin};
use crate::options::{Halo, MplError};
use crate::read::MacroGeometry;
use std::collections::HashMap;
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
    /// An upstream `logger_->error` with its MPL code — a design upstream itself refuses.
    /// 🔑 The code is the contract; the wording is only for people.
    Mpl(MplError),
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

/// Everything the stage reads from the database.
///
/// 🔑 Grouped rather than passed one by one because `init` needs all of it *before* it decides
/// whether to run at all — the feasibility checks come first, and each reads a different corner
/// of the design.
pub struct DesignInputs<'a> {
    pub design: &'a Design,
    pub pins: &'a [Pin],
    pub nets: &'a [crate::netlist::DbNet],
    /// Per instance, `Some` for macros only.
    pub geometry: &'a [Option<MacroGeometry>],
    /// Placement blockages, as read. ⚠️ Their **union** is what occupies area, not their sum.
    pub blockages: &'a [Rect],
    /// Upstream `getMinimumSpacing`, computed once over every macro's geometry.
    pub minimum_spacing: i64,
}

/// Everything the stage takes from the command line.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterOptions {
    pub thresholds: Thresholds,
    pub max_num_level: i32,
    pub cluster_size_ratio: f32,
    pub global_fence: Option<Rect>,
    pub large_net_threshold: usize,
    pub base_halo: Halo,
    pub use_full_halo: bool,
    /// `set_macro_halo` per instance. ⚠️ An explicit halo wins outright — before `use_full_halo`
    /// is consulted and before any reorientation.
    pub macro_halos: HashMap<usize, Halo>,
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
            base_halo: d.base_halo_from_flags.unwrap_or_default(),
            use_full_halo: d.use_full_halo,
            macro_halos: HashMap::new(),
        }
    }
}

/// Upstream `ClusteringEngine::run`.
///
/// The order is the algorithm, and one step of it is visible in upstream's own output:
/// **IO clusters are created BEFORE the macro clusters**, so they take the lower ids —
/// `ios_1` precedes `MACRO_4` in a real dump.
pub fn run_clustering(input: &DesignInputs, opts: &ClusterOptions) -> Clustering {
    let design = input.design;

    // ---- setFloorplanShape
    let Some(placement_area) =
        crate::design::floorplan_shape(&design.core_area, opts.global_fence.as_ref())
    else {
        return refuse(Refusal::Mpl(MplError::new(
            68,
            "The global fence set is completely outside the core area.",
        )));
    };

    // ---- createHardMacros: resolve every macro's halo, and refuse one that cannot fit the core
    //
    // ⚠️ **Before** the movable-cell test, because that test measures macros WITH their halos.
    let halos = match resolve_halos(input, opts, &placement_area) {
        Ok(h) => h,
        Err(e) => return refuse(Refusal::Mpl(e)),
    };

    // ---- movableCellsFitInMacroPlacementArea (MPL-65)
    //
    // 🔑 This runs BEFORE the module metrics and before the vacuous test, so a design that fails
    // it never produces a tree at all.
    let macro_area = |i: usize| -> i64 {
        let Some(g) = input.geometry[i].as_ref() else { return 0 };
        let h = halos[i];
        (g.master_width + h.left + h.right) * (g.master_height + h.bottom + h.top)
    };
    if !crate::feasibility::movable_cells_fit(
        design,
        &placement_area,
        &macro_area,
        input.blockages,
    ) {
        return refuse(Refusal::Mpl(MplError::new(
            65,
            "The movable cells do not fit in the macro placement area.",
        )));
    }

    let mut errors = Vec::new();
    let design_metrics = compute_module_metrics(design, design.top, &placement_area, &mut errors);
    if let Some(e) = errors.first() {
        return refuse(Refusal::Error(format!(
            "fixed non-macro instance {} inside the macro placement area",
            e.name
        )));
    }

    // ⚠️ No unfixed macros means the run does nothing. That is `vacuous`, never `applied`.
    let unfixed = crate::design::unfixed_macros(design);
    if unfixed.is_empty() {
        return Clustering {
            root: Cluster::new(0, "root"),
            status: Status::Vacuous,
            refusal: None,
        };
    }

    // ---- the halo-area test (MPL-16), which upstream runs only once it knows there is work
    let macro_with_halo_area: i64 = unfixed.iter().map(|&i| macro_area(i)).sum();
    if !crate::feasibility::instance_area_with_halos_fits(
        macro_with_halo_area,
        design_metrics.std_cell_area,
        placement_area.area(),
    ) {
        return refuse(Refusal::Mpl(MplError::new(
            16,
            "The instance area considering the macros' halos exceeds the floorplan area.",
        )));
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
        create_io_clusters(input.pins, &design.die_area, io_first_id)
    } else {
        crate::ioclusters::create_io_pad_clusters(&pads, design, io_first_id)
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
    let mut bterm_to_cluster = vec![None; input.pins.len()];
    for &(pin, cluster) in &io.assignment {
        bterm_to_cluster[pin] = Some(cluster);
    }

    let mut ctx = crate::tree::SplitCtx {
        design,
        nets: input.nets,
        bterm_to_cluster,
        // ⚠️ With pads present the pads carry the connectivity and block ports are ignored.
        design_has_io_pads: !pads.is_empty(),
        large_net_threshold: opts.large_net_threshold,
        seed_assoc: pads.iter().copied().zip(io_first_id..).collect(),
        assoc: Vec::new(),
    };
    let mut next_id = builder.next_id();
    let _virtual = crate::tree::split_mixed_leaves(&mut root, &mut ctx, &mut next_id);

    let _ = (is_ignorable_marker(), ClusterType::Mixed, IoClusters::default);
    Clustering { root, status: Status::Applied, refusal: None }
}

fn is_ignorable_marker() -> fn(&crate::design::Instance) -> bool {
    is_ignored_inst
}

/// Upstream `createHardMacros`: one resolved halo per macro, and MPL-6 for any that cannot fit.
///
/// ⚠️ **The MPL-6 comparison is against the CORE area, not the floorplan shape** a global fence
/// may have narrowed — and it uses the macro's size *with halo*.
///
/// ℹ️ An ignorable macro (fixed, and not overlapping the placement area) is skipped entirely, so
/// it never gets a halo and never trips MPL-6.
fn resolve_halos(
    input: &DesignInputs,
    opts: &ClusterOptions,
    placement_area: &Rect,
) -> Result<Vec<Halo>, MplError> {
    let design = input.design;
    let mut halos = vec![Halo::default(); design.instances.len()];
    for (i, inst) in design.instances.iter().enumerate() {
        let Some(g) = input.geometry[i].as_ref() else { continue };
        if inst.is_fixed && !overlaps(&inst.bbox, placement_area) {
            continue;
        }
        let halo = crate::halo::build_macro_halo(
            opts.macro_halos.get(&i).copied(),
            g.inst_halo,
            opts.base_halo,
            opts.use_full_halo,
            &g.pins,
            g.master_width,
            g.master_height,
            input.minimum_spacing,
            inst.is_fixed,
            g.orient,
        );
        if !crate::feasibility::macro_fits_in_core(
            g.master_width + halo.left + halo.right,
            g.master_height + halo.bottom + halo.top,
            &design.core_area,
        ) {
            return Err(MplError::new(
                6,
                &format!("Found macro that does not fit in the core.\nName: {}", inst.name),
            ));
        }
        halos[i] = halo;
    }
    Ok(halos)
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x_min < b.x_max && b.x_min < a.x_max && a.y_min < b.y_max && b.y_min < a.y_max
}

fn refuse(r: Refusal) -> Clustering {
    Clustering { root: Cluster::new(0, "root"), status: Status::Refused, refusal: Some(r) }
}
