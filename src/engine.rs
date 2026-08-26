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
    /// Upstream `reportDesignData`, which `init` emits only once it knows there is work to do.
    /// ⚠️ `None` for a refused or vacuous run — upstream returns before reporting in both cases.
    pub report: Option<crate::report::DesignReport>,
    /// What the next stage needs and this one already computed.
    /// ⚠️ `None` exactly when `report` is — a run that never clustered has nothing to shape.
    pub shaping: Option<ShapingHandoff>,
}

/// Everything `runCoarseShaping` reads that the clustering stage has already worked out.
///
/// 🔑 **Upstream does not need this type at all**: both stages are methods on `HierRTLMP` and
/// share one `tree_`, so shaping simply reads what clustering left behind. Here the values are
/// handed over explicitly, because the alternative is a mutable tree two stages can both reach
/// into — and the point of the split is that the second stage cannot change the first's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapingHandoff {
    pub die: Rect,
    /// `setFloorplanShape`'s result — the core, or the global fence clipped to it.
    pub floorplan: Rect,
    /// ⚠️ **Needs BOTH conditions.** Upstream sets it inside the no-standard-cells branch and
    /// only when the design also has no IO clusters (`clusterEngine.cpp` ~719). A design of pure
    /// macros WITH pins is not this case, and `boundary_push1` is exactly that design.
    pub has_only_macros: bool,
    pub has_io_pads: bool,
    /// The TOP module's standard-cell area. Zero means every blockage would have zero depth, and
    /// upstream returns rather than make them.
    pub top_std_cell_area: i64,
    /// `(width, height)` WITH halo, by instance index. ⚠️ `(0, 0)` for anything that is not a
    /// macro; the shaping stage only ever asks about macros.
    pub macro_dims: Vec<(i64, i64)>,
    /// The halo-inclusive bounding box, by instance index. Needed for the FIXED macros, whose
    /// position is part of what the tiling search has to pack around.
    pub macro_bboxes: Vec<Rect>,
    /// ⚠️ Whether the design has any standard cells at all — it selects the search's action
    /// probabilities, because the reference zeroes the resize share when it does not.
    pub has_std_cells: bool,
    /// Summed over the UNFIXED macros, as `init` computed it for the MPL-16 test.
    pub macro_with_halo_area: i64,
    pub io_bundles: Vec<crate::regions::IoRegion>,
    pub constrained_regions: Vec<crate::regions::IoRegion>,
    /// Block ports whose first pin is fixed — the bundle builder's denominator.
    pub fixed_ios: i64,
    /// Block ports whose first pin is NOT fixed — the constraint builder's denominator.
    pub unfixed_ios: i64,
    pub has_unconstrained_ios: bool,
    pub blocked_regions_for_pins: Vec<Rect>,
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
    /// `dbBlock::getBlockedRegionsForPins` — where pins may NOT sit.
    ///
    /// ⚠️ **Not the same thing as `blockages`.** These are die-edge lines consulted only by the
    /// coarse-shaping stage; placement blockages are areas inside the core.
    pub blocked_regions_for_pins: &'a [Rect],
    /// Upstream `getMinimumSpacing`, computed once over every macro's geometry.
    pub minimum_spacing: i64,
    /// Reported verbatim; the engine does not use it.
    pub manufacturing_grid: i32,
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
    // (width, height) WITH halo, per instance — what the shaping stage measures a macro by.
    let macro_dims: Vec<(i64, i64)> = (0..design.instances.len())
        .map(|i| match input.geometry[i].as_ref() {
            Some(g) => {
                let h = halos[i];
                (g.master_width + h.left + h.right, g.master_height + h.bottom + h.top)
            }
            None => (0, 0),
        })
        .collect();
    // The same geometry as a box, for the fixed macros the search must pack around.
    //
    // ⛔ **The halo is subtracted from the MINIMUM corner, not added to the maximum one.** The
    // reference anchors a fixed macro at `bbox.xMin() - halo.left, bbox.yMin() - halo.bottom` and
    // then sizes it from the MASTER's dimensions plus both halos. Growing right and top instead
    // gives a box of the right size in the wrong place, which survives everywhere except where it
    // is clipped to the outline — and there it silently changes the clipped extent.
    let macro_bboxes: Vec<Rect> = (0..design.instances.len())
        .map(|i| match input.geometry[i].as_ref() {
            Some(_) => {
                let (w, h) = macro_dims[i];
                let b = design.instances[i].bbox;
                let halo = halos[i];
                let x_min = b.x_min - halo.left;
                let y_min = b.y_min - halo.bottom;
                Rect { x_min, y_min, x_max: x_min + w, y_max: y_min + h }
            }
            None => Rect { x_min: 0, y_min: 0, x_max: 0, y_max: 0 },
        })
        .collect();

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
            report: None,
            shaping: None,
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

    // ---- reportDesignData, which upstream emits at the END of init — after the checks and only
    // when there is work. It is the ONLY place a resolved halo is observable before commit.
    let report = Some(crate::report::design_report(
        design,
        &placement_area,
        &design_metrics,
        opts.base_halo,
        macro_with_halo_area,
        unfixed.len(),
        input.manufacturing_grid,
    ));

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
        return Clustering {
            root,
            status: Status::Applied,
            refusal: None,
            report,
            shaping: Some(shaping_handoff(
                design,
                input.pins,
                input.blocked_regions_for_pins,
                &placement_area,
                design_metrics.std_cell_area,
                macro_with_halo_area,
                macro_dims,
                macro_bboxes,
                &io,
                !pads.is_empty(),
                design_metrics.num_std_cell,
            )),
        };
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
    Clustering {
        root,
        status: Status::Applied,
        refusal: None,
        report,
        shaping: Some(shaping_handoff(
            design,
            input.pins,
            input.blocked_regions_for_pins,
            &placement_area,
            design_metrics.std_cell_area,
            macro_with_halo_area,
            macro_dims,
            macro_bboxes,
            &io,
            !pads.is_empty(),
            design_metrics.num_std_cell,
        )),
    }
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

/// Gather what the shaping stage needs, at the point every value is already in hand.
///
/// ⚠️ Called at BOTH success returns. The no-standard-cells branch returns early and still goes
/// on to be shaped — `macro_only` and `boundary_push1` both take it.
#[allow(clippy::too_many_arguments)]
fn shaping_handoff(
    design: &Design,
    pins: &[Pin],
    blocked_regions_for_pins: &[Rect],
    floorplan: &Rect,
    top_std_cell_area: i64,
    macro_with_halo_area: i64,
    macro_dims: Vec<(i64, i64)>,
    macro_bboxes: Vec<Rect>,
    io: &IoClusters,
    has_io_pads: bool,
    num_std_cell: i32,
) -> ShapingHandoff {
    let die = design.die_area;
    // ⚠️ Upstream measures an IO cluster by its BBOX, which for every kind that reaches here is a
    // LINE on a die edge. A cluster without one contributes nothing rather than a stray point.
    let as_region = |c: &Cluster| -> Option<crate::regions::IoRegion> {
        let line = c.io_region?;
        Some(crate::regions::IoRegion {
            region: crate::regions::BoundaryRegion {
                boundary: crate::regions::boundary_of(&die, &line),
                line,
            },
            ios: c.num_io_pins as i64,
        })
    };
    ShapingHandoff {
        die,
        floorplan: *floorplan,
        has_only_macros: num_std_cell == 0 && !io.has_io_clusters,
        has_io_pads,
        top_std_cell_area,
        macro_dims,
        macro_bboxes,
        has_std_cells: num_std_cell > 0,
        macro_with_halo_area,
        io_bundles: io.bundles.iter().filter_map(as_region).collect(),
        // ⚠️ The CONSTRAINED ones only. An unconstrained cluster is served by the available-region
        // builder instead, and counting it here would give it a blockage from both.
        constrained_regions: io
            .pin_clusters
            .iter()
            .filter(|c| !c.is_cluster_of_unconstrained_io_pins)
            .filter_map(as_region)
            .collect(),
        fixed_ios: pins.iter().filter(|p| p.is_fixed).count() as i64,
        unfixed_ios: pins.iter().filter(|p| !p.is_fixed).count() as i64,
        has_unconstrained_ios: io
            .pin_clusters
            .iter()
            .any(|c| c.is_cluster_of_unconstrained_io_pins),
        blocked_regions_for_pins: blocked_regions_for_pins.to_vec(),
    }
}

fn refuse(r: Refusal) -> Clustering {
    Clustering {
        root: Cluster::new(0, "root"),
        status: Status::Refused,
        refusal: Some(r),
        report: None,
        shaping: None,
    }
}
