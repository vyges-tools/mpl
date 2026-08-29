// SPDX-License-Identifier: Apache-2.0
//! Print a design's physical hierarchy — or its design-data report — in upstream's own format,
//! so the two can be diffed.
//!
//! ⛔ **The halo commands are COMMANDS, not database state.** Preparing the `.odb` captures
//! `set_io_pin_constraint`, which writes onto the ports, and captures nothing of
//! `set_macro_halo`, `set_macro_base_halo` or `-use_full_halo`. A case that sets one and is run
//! without the matching flag is being scored against a different input than upstream used.
//!
//! ⚠️ **Halo values are MICRONS**, as the Tcl takes them — `mpl.i` converts with
//! `block->micronsToDbu`, which multiplies and **rounds**. Passing database units instead makes
//! a 8 µm halo into 8 DBU, which on Nangate45 is 1/2000th of the intended keep-out and looks
//! almost exactly like no halo at all.
use std::collections::HashMap;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut path = None;
    let mut want_report = false;
    let mut want_shaping = false;
    let mut use_full_halo = false;
    let mut base_halo: Option<[f64; 4]> = None;
    let mut macro_halos: Vec<(String, [f64; 4])> = Vec::new();
    // ⛔ `set_macro_guidance_region` is ENGINE state, like the halo commands — a prepared `.odb`
    // cannot carry it, so the case's `.tcl` has to be translated onto this command line.
    let mut macro_guides: Vec<(String, [f64; 4])> = Vec::new();
    let mut w_boundary: Option<f32> = None;
    let mut w_notch: Option<f32> = None;
    let mut w_guidance: Option<f32> = None;
    // Upstream's `-target_util` default.
    let mut target_util: f32 = 0.25;
    // ⚠️ Zero means "not supplied" — the same sentinel `setBaseThresholds` reads.
    let mut sup = vyges_mpl::thresholds::Thresholds {
        max_macro: 0,
        min_macro: 0,
        max_std_cell: 0,
        min_std_cell: 0,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--report" => want_report = true,
            "--shaping" => want_shaping = true,
            "--place" => {}
            "--placement-trace" => {}
            "--push" => {}
            "--flip" => {}
            "--floorplan" => {}
            "--nets" => {}
            "--cost" => {}
            "--use-full-halo" => use_full_halo = true,
            "--base-halo" => {
                let Some(spec) = it.next() else { usage("--base-halo needs l,b,r,t in microns") };
                base_halo = Some(four(spec));
            }
            "--macro-halo" => {
                let Some(spec) = it.next() else { usage("--macro-halo needs NAME=l,b,r,t") };
                let Some((name, vals)) = spec.split_once('=') else {
                    usage("--macro-halo needs NAME=l,b,r,t")
                };
                macro_halos.push((name.to_string(), four(vals)));
            }
            // ⛔ `rtl_macro_placer`'s threshold options are ENGINE state like the halos: a
            // prepared `.odb` cannot carry them, and supplying them keeps `max_level` at 2 where
            // the derivation would otherwise drop it to 1 — which changes the soft-blockage weight.
            // ⛔ `rtl_macro_placer`'s WEIGHT and utilization options are engine state too. The
            // suite sets `-boundary_weight`, `-notch_weight`, `-guidance_weight` and
            // `-target_util`; an untranslated one leaves a term live that the case turned OFF, or
            // ramps the utilization from the wrong base.
            "--boundary-weight" => w_boundary = Some(next_f32(&mut it, "--boundary-weight")),
            "--notch-weight" => w_notch = Some(next_f32(&mut it, "--notch-weight")),
            "--guidance-weight" => w_guidance = Some(next_f32(&mut it, "--guidance-weight")),
            "--target-util" => target_util = next_f32(&mut it, "--target-util"),
            "--max-num-inst" => sup.max_std_cell = next_i32(&mut it, "--max-num-inst"),
            "--min-num-inst" => sup.min_std_cell = next_i32(&mut it, "--min-num-inst"),
            "--max-num-macro" => sup.max_macro = next_i32(&mut it, "--max-num-macro"),
            "--min-num-macro" => sup.min_macro = next_i32(&mut it, "--min-num-macro"),
            "--macro-guide" => {
                let Some(spec) = it.next() else { usage("--macro-guide needs NAME=lx,ly,ux,uy") };
                let Some((name, vals)) = spec.split_once('=') else {
                    usage("--macro-guide needs NAME=lx,ly,ux,uy")
                };
                macro_guides.push((name.to_string(), four(vals)));
            }
            other if !other.starts_with("--") => path = Some(other.to_string()),
            other => usage(&format!("unknown option {other}")),
        }
    }
    let Some(path) = path else {
        usage(
            "usage: cluster-dump [--report|--shaping] [--use-full-halo] \
             [--base-halo l,b,r,t] [--macro-halo NAME=l,b,r,t] \
             [--macro-guide NAME=lx,ly,ux,uy] [--push] <design.odb>   (all values in microns)",
        )
    };

    let place = std::env::args().any(|a| a == "--place");
    let summaries = std::env::args().any(|a| a == "--placement-trace")
        || std::env::args().any(|a| a == "--floorplan")
        || std::env::args().any(|a| a == "--nets")
        || std::env::args().any(|a| a == "--cost")
        || std::env::args().any(|a| a == "--push")
        || std::env::args().any(|a| a == "--flip");
    let db = match vyges_opendb::Db::open(&path) {
        Ok(d) => d,
        Err(e) => { eprintln!("cannot read {path}: {e}"); std::process::exit(2); }
    };
    let mut design = match vyges_mpl::read::read_design(&db) {
        Ok(d) => d,
        Err(e) => { eprintln!("cannot interpret {path}: {e}"); std::process::exit(2); }
    };
    let area = vyges_mpl::design::floorplan_shape(&design.core_area, None)
        .expect("a core to place into");
    vyges_mpl::read::mark_ignorable_macros(&mut design, &area);
    let pins = vyges_mpl::read::read_pins(&db);
    let nets = vyges_mpl::read::read_nets(&db, &design, &pins);
    let geometry = vyges_mpl::read::read_macro_geometry(&db, &design);
    let blockages = vyges_mpl::read::read_blockages(&db);
    let blocked_for_pins = vyges_mpl::read::read_blocked_regions_for_pins(&db);

    // ⚠️ Halos arrive by instance NAME on the command line and by index in the engine. A name
    // that matches nothing is an error, not a silent no-op — it would look like the halo applied.
    let by_name: HashMap<&str, usize> =
        design.instances.iter().enumerate().map(|(i, x)| (x.name.as_str(), i)).collect();
    let dbu = db.dbu_per_micron();
    let mut opts = vyges_mpl::engine::ClusterOptions::default();
    opts.use_full_halo = use_full_halo;
    opts.thresholds = sup;
    if let Some(h) = base_halo {
        opts.base_halo = to_dbu(h, dbu);
    }
    for (name, halo) in &macro_halos {
        match by_name.get(name.as_str()) {
            Some(&i) => { opts.macro_halos.insert(i, to_dbu(*halo, dbu)); }
            None => { eprintln!("no instance named {name}"); std::process::exit(2); }
        }
    }
    let mut guide_regions: Vec<(usize, (i32, i32, i32, i32))> = Vec::new();
    for (name, region) in &macro_guides {
        match by_name.get(name.as_str()) {
            Some(&i) => guide_regions.push((i, region_to_dbu(*region, dbu))),
            None => { eprintln!("no instance named {name}"); std::process::exit(2); }
        }
    }

    let input = vyges_mpl::engine::DesignInputs {
        design: &design,
        pins: &pins,
        nets: &nets,
        geometry: &geometry,
        blockages: &blockages,
        blocked_regions_for_pins: &blocked_for_pins,
        minimum_spacing: vyges_mpl::read::minimum_spacing(&db, &design),
        manufacturing_grid: db.manufacturing_grid().unwrap_or(None).unwrap_or(0),
    };

    let r = vyges_mpl::engine::run_clustering(&input, &opts);
    if let Some(refusal) = &r.refusal {
        eprintln!("refused: {refusal:?}");
        std::process::exit(1);
    }
    // ⛔ A vacuous run must not print a tree. It produces an empty root, and printing that gives
    // `root (0) Type: Mixed Leaf` — a line that reads like a result and would be DIFFED against
    // upstream as though the engine had clustered something. Vacuous gets its own exit code.
    if r.status == vyges_mpl::status::Status::Vacuous {
        eprintln!("vacuous: nothing to place");
        std::process::exit(3);
    }
    if want_report {
        match &r.report {
            Some(rep) => print!("{}", rep.render(dbu)),
            None => { eprintln!("no report: the run produced none"); std::process::exit(3); }
        }
        return;
    }
    if want_shaping {
        let overrides = Overrides {
            boundary: w_boundary,
            notch: w_notch,
            guidance: w_guidance,
            target_util,
        };
        let (term_boxes, master_of) = vyges_mpl::read::read_term_boxes(&db, &design);
        let inst_placement = vyges_mpl::read::read_instance_placements(&db, &design);
        let flip = FlipInputs {
            pins: &pins,
            term_boxes: &term_boxes,
            master_of: &master_of,
            inst_placement: &inst_placement,
        };
        print_coarse_shaping_trace(
            r, &blockages, dbu, place, summaries, &design, &nets, &guide_regions, overrides,
            &geometry, &flip,
        );
        return;
    }
    print!("{}", vyges_mpl::dump::physical_hierarchy(&r.root));
}

/// Run the coarse-shaping stage on a clustered design and print upstream's `coarse_shaping`
/// trace, so `openroad -no_init` output and ours can be diffed line for line.
///
/// ⛔ **A refusal gets its own exit code (4).** Eight of the regression designs anneal at the
/// root, and this engine refuses those by name rather than approximating them. Printing a partial
/// trace for one would put a SHORTER but otherwise matching block in front of the harness, which
/// reads as a pass on everything it did emit.
#[allow(clippy::too_many_arguments)]
/// What the flip WIRELENGTH needs and nothing else does.
///
/// ⚠️ Bundled rather than threaded as four more parameters: the chain from `main` to the flip pass
/// is three functions deep and already long, and these four travel together or not at all.
pub struct FlipInputs<'a> {
    pub pins: &'a [vyges_mpl::ioclusters::Pin],
    /// Per master TERMINAL geometry, untransformed — see `read::read_term_boxes`.
    pub term_boxes: &'a std::collections::HashMap<(String, String), Vec<(i32, i32, i32, i32)>>,
    /// The master name per INSTANCE; `Instance` carries only an interned id.
    pub master_of: &'a [String],
    /// Each instance's DATABASE transform — see `read::read_instance_placements`.
    ///
    /// ⛔ **Consulted for NON-MACRO instances only.** A macro's transform during the flip pass is
    /// the placer's, rebuilt from the halo on every flip; the database still holds the value the
    /// design was read with.
    pub inst_placement: &'a [(vyges_mpl::placement::DbOrient, (i32, i32))],
}

fn print_coarse_shaping_trace(
    mut r: vyges_mpl::engine::Clustering,
    blockages: &[vyges_mpl::design::Rect],
    dbu: i32,
    place: bool,
    summaries: bool,
    design: &vyges_mpl::design::Design,
    nets: &[vyges_mpl::netlist::DbNet],
    guide_regions: &[(usize, (i32, i32, i32, i32))],
    overrides: Overrides,
    geometry: &[Option<vyges_mpl::read::MacroGeometry>],
    flip: &FlipInputs,
) {
    use vyges_mpl::cluster::ClusterType;
    // Taken before `r` is dismantled below: a net reaches an IO cluster only through this.
    let pin_cluster = std::mem::take(&mut r.pin_cluster);
    let pad_assoc = std::mem::take(&mut r.pad_assoc);
    let Some(h) = r.shaping else {
        eprintln!("no shaping inputs: the run produced none");
        std::process::exit(3);
    };

    // Upstream `computePinAccessBaseDepth`'s two passes over the ROOT's children: the standard-
    // cell clusters, and only if those come to nothing, the mixed ones.
    let area_of = |t: ClusterType| -> i64 {
        r.root
            .children
            .iter()
            .filter(|c| c.cluster_type == t)
            .map(|c| c.std_cell_area() + c.macro_area())
            .sum()
    };
    let std_cell_children = area_of(ClusterType::StdCell);
    let mixed_children = area_of(ClusterType::Mixed);
    let root_area = h.floorplan.area();
    let base_depth = |io_span: i64| -> i64 {
        // ⚠️ Upstream errors (MPL-67) on a zero-area root. It cannot arise here: a design that
        // got this far has a floorplan, and `setFloorplanShape` refuses an empty one (MPL-68).
        vyges_mpl::shaping::pin_access_base_depth(
            std_cell_children,
            mixed_children,
            h.macro_with_halo_area,
            root_area,
            io_span,
        )
        .unwrap_or(0)
    };

    let input = vyges_mpl::shaping::CoarseInput {
        die: h.die,
        floorplan: h.floorplan,
        has_only_macros: h.has_only_macros,
        has_io_pads: h.has_io_pads,
        top_std_cell_area: h.top_std_cell_area,
        blockages,
        macro_dims: &|i| h.macro_dims[i],
        macro_bbox: &|i| h.macro_bboxes[i],
        has_std_cells: h.has_std_cells,
        search: vyges_mpl::anneal::TilingSearch::default(),
        io_bundles: &h.io_bundles,
        fixed_ios: h.fixed_ios,
        constrained_regions: &h.constrained_regions,
        unfixed_ios: h.unfixed_ios,
        blocked_regions_for_pins: &h.blocked_regions_for_pins,
        has_unconstrained_ios: h.has_unconstrained_ios,
        base_depth: &base_depth,
    };

    let mut root = r.root;
    let mut trace = vyges_mpl::trace::CoarseTrace::recording();
    let shaping = match vyges_mpl::shaping::run_coarse_shaping_traced(&mut root, &input, dbu, &mut trace) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("shaping refused: {e:?}");
            std::process::exit(4);
        }
    };
    if !place && !summaries {
        print!("{}", trace.finish());
        return;
    }
    if summaries {
        print_placement_summaries(
            &root,
            &h,
            &shaping,
            dbu,
            design,
            nets,
            &|i| pin_cluster.get(i).copied().flatten(),
            h.has_io_pads,
            guide_regions,
            &pad_assoc,
            overrides,
            geometry,
            flip,
        );
    } else {
        report_placement(&root, &h, &shaping, dbu, overrides);
    }
}

/// ⚠️ **A smoke path, not an oracle.** It runs the composed placement stage on a real design and
/// reports what each parent did, so the wiring is exercised rather than assumed. Scoring against
/// the reference's per-term penalty tables is a separate harness and is NOT this.
fn report_placement(
    root: &vyges_mpl::cluster::Cluster,
    h: &vyges_mpl::engine::ShapingHandoff,
    shaping: &vyges_mpl::shaping::CoarseShaping,
    dbu: i32,
    overrides: Overrides,
) {
    use vyges_mpl::placement as p;

    let floorplan = h.floorplan;
    let outline = (
        floorplan.x_min as i32,
        floorplan.y_min as i32,
        floorplan.x_max as i32,
        floorplan.y_max as i32,
    );
    let die = h.die;
    let margin = 2 * ((die.x_max - die.x_min) + (die.y_max - die.y_min));

    // ⚠️ `has_std_cells` decides whether the reset fires, so it is not a detail the harness may
    // default — it changes four weights and every action share.
    // ⛔ The tree's own level cap, not a hardcoded 1. `adjustSoftBlockageWeight` fires only at
    // level 1, so a design that keeps `max_level = 2` — one supplying its own thresholds — must
    // keep the command default of 10 rather than being raised to 50.
    let (weights, tiny, probabilities) = p::placement_setup(
        h.max_level,
        overrides.weights(),
        0,
        h.has_std_cells,
    );
    let utilizations = p::utilization_list(overrides.target_util, 10);

    // ⛔ **Both lists are CLIPPED to the outline and rebased onto it.** `placeChildren` opens with
    // `findOffsetIntersections` on each, and everything inside a placement problem is
    // outline-relative. Passing them raw leaves a blockage in DIE coordinates: on `guides1` the
    // pin-access blockage runs to the die's 250000 while the outline ends at 249200, so every
    // overlap measured against it was slightly too large — invisible in the final penalty, which
    // is zero there, and visible only in the normalisation factor averaged over the sweep.
    let raw_hard: Vec<(i32, i32, i32, i32)> = shaping
        .placement_blockages
        .iter()
        .map(|b| (b.x_min as i32, b.y_min as i32, b.x_max as i32, b.y_max as i32))
        .collect();
    let raw_soft: Vec<(i32, i32, i32, i32)> = shaping
        .io_blockages
        .iter()
        .map(|b| (b.x_min as i32, b.y_min as i32, b.x_max as i32, b.y_max as i32))
        .collect();
    let blockages = p::find_offset_intersections(&raw_hard, outline);
    let soft = p::find_offset_intersections(&raw_soft, outline);

    let visits = p::run_hierarchical_macro_placement(
        root,
        &utilizations,
        10,
        &vyges_mpl::anneal::SaParameters::default(),
        probabilities,
        weights,
        0,
        &mut |parent| {
            let ctx = p::ParentContext {
                connections_of: &|_| Vec::new(),
                virtual_connections: &parent.virtual_connections,
                blockages: &blockages,
                soft_blockages: &soft,
                fence_of: &|_| None,
                // ℹ️ The `--place` smoke path never receives guides; `--placement-trace` is the
                // mode the penalty tables are read from and the one that wires them.
                guide_of: &|_| None,
                terminals: &[],
                root: p::Root {
                    x: outline.0,
                    y: outline.1,
                    width: outline.2 - outline.0,
                    height: outline.3 - outline.1,
                },
                die_margin: margin,
                available_regions: &[],
                constraint_region_of: &|_| None,
                weights,
                dbu_per_micron: dbu,
                tiny_threshold: tiny,
                min_ar: 0.33,
            };
            Some((p::build_parent_problem(parent, outline, &ctx), parent.id))
        },
        // ℹ️ The `--place` smoke path neither writes back nor assembles macro-cluster inputs;
        // `--placement-trace` is the mode the macro summaries are read from.
        &mut |_, _| {},
        &mut |_| None,
        10,
    );

    for v in &visits {
        let what = match &v.outcome {
            p::ParentOutcome::Placed { run, macros } => {
                format!("placed, run {} of {}, {} macros", run.index, utilizations.len(), macros.len())
            }
            p::ParentOutcome::MacroCluster { macros } => {
                format!("macro cluster, {} macros placed", macros.len())
            }
            p::ParentOutcome::FixedMacroCluster => "fixed macro cluster".to_string(),
            p::ParentOutcome::Leaf => "leaf".to_string(),
            p::ParentOutcome::NoValidSolution(e) => format!("NO VALID SOLUTION (MPL-{})", e.code),
        };
        println!("cluster {:>3}  {what}", v.cluster);
    }
}

/// Emit the reference's `Cluster Placement Summary` for every parent that was placed.
///
/// ⛔ **Only a placed parent produces a table.** A macro cluster gets a `Macro Placement Summary`
/// from a different core, and a leaf gets nothing — so a harness must attribute a missing block,
/// never treat it as a match.
#[allow(clippy::too_many_arguments)]
fn print_placement_summaries(
    root: &vyges_mpl::cluster::Cluster,
    h: &vyges_mpl::engine::ShapingHandoff,
    shaping: &vyges_mpl::shaping::CoarseShaping,
    dbu: i32,
    design: &vyges_mpl::design::Design,
    nets: &[vyges_mpl::netlist::DbNet],
    pin_cluster_of: &dyn Fn(usize) -> Option<i32>,
    has_io_pads: bool,
    guide_regions: &[(usize, (i32, i32, i32, i32))],
    pad_assoc: &[(usize, vyges_mpl::cluster::ClusterId)],
    overrides: Overrides,
    geometry: &[Option<vyges_mpl::read::MacroGeometry>],
    flip: &FlipInputs,
) {
    use vyges_mpl::placement as p;

    // 🔑 **Rebuilt from the association as it stands**, which is what `rebuildConnections` does
    // inside `placeChildren` — not a map built once for the whole design.
    let mut assoc = vyges_mpl::tree::associate_instances(root, design);
    // ⛔ Re-applied after the walk, exactly as `split_mixed_leaves` does: an IO pad's cluster holds
    // no leaf instances, so the walk leaves the pad unassociated and every net reaching it is
    // dropped for want of a second endpoint.
    for &(inst, id) in pad_assoc {
        if let Some(slot) = assoc.get_mut(inst) {
            *slot = Some(id);
        }
    }

    // 🔑 **A cluster inherits the UNION of its macros' guides.** Upstream merges over
    // `cluster->getHardMacros()`; the association is the same set, read the other way round.
    // ⚠️ The merge happens BEFORE the outline clip and the `area() > 0` test, both of which
    // `merged_region` applies downstream — merging after clipping would drop a guide that reaches
    // the outline only through another macro's half of the union.
    let mut guide_of_cluster: std::collections::BTreeMap<i32, (i32, i32, i32, i32)> =
        std::collections::BTreeMap::new();
    for &(inst, r) in guide_regions {
        let Some(cluster) = assoc.get(inst).copied().flatten() else { continue };
        guide_of_cluster
            .entry(cluster)
            .and_modify(|m| {
                *m = (m.0.min(r.0), m.1.min(r.1), m.2.max(r.2), m.3.max(r.3));
            })
            .or_insert(r);
    }
    let connections = vyges_mpl::netlist::build_connections(
        nets,
        design,
        &|i| assoc.get(i).copied().flatten(),
        pin_cluster_of,
        has_io_pads,
        50,
    );

    let floorplan = h.floorplan;
    let outline = (
        floorplan.x_min as i32,
        floorplan.y_min as i32,
        floorplan.x_max as i32,
        floorplan.y_max as i32,
    );
    let die = h.die;
    let margin = 2 * ((die.x_max - die.x_min) + (die.y_max - die.y_min));
    // ⚠️ `has_std_cells` decides whether the reset fires, so it is not a detail the harness may
    // default — it changes four weights and every action share.
    // ⛔ The tree's own level cap, not a hardcoded 1. `adjustSoftBlockageWeight` fires only at
    // level 1, so a design that keeps `max_level = 2` — one supplying its own thresholds — must
    // keep the command default of 10 rather than being raised to 50.
    let (weights, tiny, probabilities) = p::placement_setup(
        h.max_level,
        overrides.weights(),
        0,
        h.has_std_cells,
    );
    let utilizations = p::utilization_list(overrides.target_util, 10);
    // ⛔ **Both lists are CLIPPED to the outline and rebased onto it.** `placeChildren` opens with
    // `findOffsetIntersections` on each, and everything inside a placement problem is
    // outline-relative. Passing them raw leaves a blockage in DIE coordinates: on `guides1` the
    // pin-access blockage runs to the die's 250000 while the outline ends at 249200, so every
    // overlap measured against it was slightly too large — invisible in the final penalty, which
    // is zero there, and visible only in the normalisation factor averaged over the sweep.
    let raw_hard: Vec<(i32, i32, i32, i32)> = shaping
        .placement_blockages
        .iter()
        .map(|b| (b.x_min as i32, b.y_min as i32, b.x_max as i32, b.y_max as i32))
        .collect();
    let raw_soft: Vec<(i32, i32, i32, i32)> = shaping
        .io_blockages
        .iter()
        .map(|b| (b.x_min as i32, b.y_min as i32, b.x_max as i32, b.y_max as i32))
        .collect();
    let blockages = p::find_offset_intersections(&raw_hard, outline);
    let soft = p::find_offset_intersections(&raw_soft, outline);

    // The die-edge stretches an unconstrained IO pin may land on.
    //
    // ⛔ **REBASED onto the outline, and the constraint regions are NOT.** That asymmetry is
    // upstream's: `setAvailableRegionsForUnconstrainedPins` subtracts the outline's corner from
    // every region it is handed, while `io_cluster_to_constraint_` is assigned straight from the
    // tree and keeps DIE coordinates. Both are then compared against an outline-relative pin, so
    // the constrained branch measures across two coordinate systems on purpose.
    //
    // ⚠️ Leaving these absolute makes every unconstrained-IO distance wrong by the outline's
    // corner — on `halos3` that is the whole difference between our wirelength and the
    // reference's, on identical geometry.
    // Absolute, as coarse shaping found them. Each CORE rebases them onto its OWN outline.
    let available_abs: Vec<p::Region> = shaping
        .available_regions
        .iter()
        .map(|r| p::Region {
            x0: r.line.x_min as i32,
            y0: r.line.y_min as i32,
            x1: r.line.x_max as i32,
            y1: r.line.y_max as i32,
            boundary: r.boundary,
        })
        .collect();
    let rebase = |regions: &[p::Region], (ox, oy): (i32, i32)| -> Vec<p::Region> {
        regions
            .iter()
            .map(|r| p::Region { x0: r.x0 - ox, y0: r.y0 - oy, x1: r.x1 - ox, y1: r.y1 - oy, ..*r })
            .collect()
    };
    let available: Vec<p::Region> = rebase(&available_abs, (outline.0, outline.1));

    // 🔑 **A CONSTRAINED IO cluster measures against its OWN region, not the available ones.**
    // Both paths were stubbed, and the constrained one is the commoner of the two: every
    // `set_io_pin_constraint` case reaches it, and without it those nets score a distance of zero.
    let die = h.die;
    let mut region_of_cluster: std::collections::BTreeMap<i32, p::Region> =
        std::collections::BTreeMap::new();
    let mut collect_regions = |c: &vyges_mpl::cluster::Cluster| {
        if let Some(r) = c.constraint_region {
            region_of_cluster.insert(
                c.id,
                p::Region {
                    x0: r.x_min as i32,
                    y0: r.y_min as i32,
                    x1: r.x_max as i32,
                    y1: r.y_max as i32,
                    boundary: vyges_mpl::regions::boundary_of(&die, &r),
                },
            );
        }
    };
    for c in &root.children {
        collect_regions(c);
        for g in &c.children {
            collect_regions(g);
        }
    }

    let floorplan_mode = std::env::args().any(|a| a == "--floorplan");
    let nets_mode = std::env::args().any(|a| a == "--nets");
    let cost_mode = std::env::args().any(|a| a == "--cost");
    // ⛔ **The push runs AFTER the whole placement walk and prints its own channel**, so every
    // summary this file would otherwise emit is suppressed — the `boundary_push` log carries no
    // penalty table, and a table printed alongside it would be diffed as a difference.
    let push_mode = std::env::args().any(|a| a == "--push");
    // ⛔ **The flip runs AFTER the push and reads the positions the push left behind** — upstream
    // orders `pushMacrosToCoreBoundaries` then `updateMacrosOnDb` then the orientation pass, so a
    // macro the push moved is flipped at its new coordinate. Scoring the flip against pre-push
    // positions differs on exactly the designs the push moved, which is the ones that matter.
    let flip_mode = std::env::args().any(|a| a == "--flip");
    // Returns the winning run's macros and their names, so the caller can do the write-back that
    // `updateChildrenShapesAndLocations` does on the tree.
    let mut emit = |parent: &vyges_mpl::cluster::Cluster|
     -> Option<(Vec<String>, Vec<vyges_mpl::anneal::SoftMacro>)> {
        let ctx = p::ParentContext {
            connections_of: &|id| connections.of(id),
            virtual_connections: &parent.virtual_connections,
            blockages: &blockages,
            soft_blockages: &soft,
            fence_of: &|_| None,
            guide_of: &|id| guide_of_cluster.get(&id).copied(),
            terminals: &[],
            root: p::Root {
                x: outline.0,
                y: outline.1,
                width: outline.2 - outline.0,
                height: outline.3 - outline.1,
            },
            die_margin: margin,
            available_regions: &available,
            constraint_region_of: &|id| region_of_cluster.get(&id).copied(),
            weights,
            dbu_per_micron: dbu,
            tiny_threshold: tiny,
            min_ar: 0.33,
        };
        let problem = p::build_parent_problem(parent, outline, &ctx);
        // ⚠️ On stderr, so it never reaches a harness diffing stdout. This line is how the first
        // divergence was attributed — the table alone says "different", this says WHERE.
        eprintln!(
            "[diag] cluster {} macros={} seq_pair={} nets={} attrs_with_macros={}",
            parent.id,
            problem.macros.len(),
            problem.number_of_sequence_pair_macros,
            problem.inputs.nets.len(),
            problem.inputs.attributes.iter().filter(|a| a.num_macro > 0).count()
        );
        if nets_mode {
            // `writeNetFile`: source, target, weight — written BEFORE the anneal, so it is the
            // earliest artifact this stage produces and the one that explains a wirelength
            // difference without the placement in the way.
            let name = |i: usize| problem.names.get(i).map(String::as_str).unwrap_or("");
            for n in &problem.inputs.nets {
                println!("{}   {}   {}", name(n.source), name(n.target), n.weight);
            }
            return None;
        }

        // ⚠️ The same two steps `run_hierarchical_macro_placement` applies around its own call:
        // the CLUSTER perturbation rule, and the dead-space fill on the winning solution. Without
        // them this path anneals a different walk and reports geometry the reference has already
        // grown. They coincide on a small design, which is why the tables agreed anyway.
        let mut sa = vyges_mpl::anneal::SaParameters::default();
        sa.num_perturb_per_step = p::cluster_perturbations_per_step(
            sa.num_perturb_per_step,
            problem.macros.len() as i32,
        );
        for (index, util) in utilizations.iter().enumerate() {
            let Some(mut search) = p::anneal_one_run(
                &problem,
                *util,
                0,
                &sa,
                probabilities,
                weights,
            ) else {
                continue;
            };
            let _ = index;
            if cost_mode {
                // `writeCostFile`: temperature and cost, once per step.
                for (t, c) in &search.cost_history {
                    println!("{t}  {c}");
                }
                return None;
            }
            let valid = search.is_valid(!search.fixed_bboxes.is_empty());
            let kinds: Vec<Option<p::AreaKind>> =
                problem.reshape.iter().map(|r| r.kind).collect();
            p::fill_dead_space_on_solution(
                &mut search.macros,
                &kinds,
                (search.outline_width, search.outline_height),
                valid,
            );
            if floorplan_mode {
                // `writeFloorplanFile`: name, x, y, width, height — three spaces between.
                for (i, m) in search.macros.iter().enumerate() {
                    println!(
                        "{}   {}   {}   {}   {}",
                        problem.names.get(i).map(String::as_str).unwrap_or(""),
                        m.x,
                        m.y,
                        m.width,
                        m.height
                    );
                }
            }
            if !floorplan_mode && !push_mode && !flip_mode {
                println!("Cluster Placement Summary");
                print!(
                    "{}",
                    p::cluster_placement_summary(
                        parent.id,
                        outline,
                        &search.penalties,
                        &search.weights,
                        &search.normalization,
                        search.area_penalty(),
                        search.norm_cost(),
                        dbu,
                    )
                );
            }
            return Some((problem.names.clone(), search.macros.clone()));
        }
        None
    };

    // ⛔ **A macro cluster's outline is the box CLUSTER placement just gave it**, so the walk has
    // to write back before it descends — the order `placeChildren` uses. Keyed by cluster id and
    // held beside the tree rather than on it; the values and the order are upstream's.
    let mut placed: std::collections::HashMap<i32, (i32, i32, i32, i32)> =
        std::collections::HashMap::new();
    // ⛔ **THE ROOT'S OWN BOX IS THE FLOORPLAN, and it has to be seeded here.** `setRootShapes`
    // gives the root soft macro the floorplan shape before any placement runs, so
    // `cluster->getBBox()` is defined for the root even though nothing ever *placed* it.
    // ⚠️ It matters only for an ALL-MACRO design, where `placeChildren` types the root
    // `HardMacroCluster` and hands it straight to `placeMacros` — without this the root is absent
    // from the map, `placeMacros` is skipped, and every macro keeps its DATABASE position. That is
    // invisible in the placement gates, which report those designs as `macro-path`, and shows up
    // first in the flip trace as a coordinate of zero where the reference has a placed one.
    placed.insert(root.id, outline);
    // ⛔ **The soft macro's AREA, kept beside its box.** `designHasSingleCentralizedMacroArray`
    // reads it rather than the cluster's own area, and upstream says why: only the abstraction
    // records that fine shaping shrank a standard-cell cluster away. It is carried on the macro,
    // not `width * height` — the resize paths set it from an interval corner.
    let mut placed_area: std::collections::HashMap<i32, i64> = std::collections::HashMap::new();

    // ⚠️ Only clusters the driver would have PLACED — the same guards, so the set matches.
    // Every hard macro's ABSOLUTE haloed corner. ⚠️ Seeded from the database, which is right for
    // the macros macro placement never moves — a FIXED macro cluster is refused at `placeMacros`'
    // first line and keeps the position it was read with.
    let mut macro_location: std::collections::HashMap<usize, (i32, i32)> = design
        .instances
        .iter()
        .enumerate()
        .filter(|(i, _)| h.macro_bboxes.get(*i).is_some())
        .map(|(i, _)| {
            let b = h.macro_bboxes[i];
            (i, (b.x_min as i32, b.y_min as i32))
        })
        .collect();

    let mut stack = vec![root];
    while let Some(c) = stack.pop() {
        let action = p::placement_action(p::area_kind_of(c), c.is_fixed_macro, c.children.is_empty());
        match action {
            p::PlacementAction::PlaceChildren => {
                if let Some((names, macros)) = emit(c) {
                    // `updateChildrenShapesAndLocations`, then `updateChildrenRealLocation`.
                    let children: Vec<(String, p::AreaKind)> = c
                        .children
                        .iter()
                        .map(|k| (k.name.clone(), p::area_kind_of(k)))
                        .collect();
                    let assembly = p::Assembly {
                        id_of: names.iter().cloned().enumerate().map(|(i, n)| (n, i)).collect(),
                        ..Default::default()
                    };
                    if let Ok(shaped) =
                        p::update_children_shapes_and_locations(&children, &macros, &assembly)
                    {
                        // The parent's own corner: the floorplan for the root, its placed box below.
                        let (ox, oy) = placed
                            .get(&c.id)
                            .map(|b| (b.0, b.1))
                            .unwrap_or((outline.0, outline.1));
                        let mut points: Vec<(i32, i32)> =
                            shaped.iter().map(|(_, m)| (m.x, m.y)).collect();
                        p::to_real_locations(&mut points, (ox, oy));
                        for ((name, m), (x, y)) in shaped.iter().zip(points) {
                            if let Some(child) = c.children.iter().find(|k| &k.name == name) {
                                placed.insert(
                                    child.id,
                                    (x, y, x + m.width, y + m.height),
                                );
                                placed_area.insert(child.id, m.area);
                            }
                        }
                    }
                }
                for child in c.children.iter().rev() {
                    stack.push(child);
                }
            }
            p::PlacementAction::PlaceMacros => {
                let located = emit_macro_summary(
                    // ⛔ **The MACRO core rebases onto ITS OWN outline**, not the root's. Passing
                    // the cluster path's already-rebased list would offset every unconstrained-IO
                    // distance by the difference between the two corners.
                    c, &placed, design, h, dbu, weights, probabilities, root, geometry,
                    &available_abs, nets, &assoc,
                    pin_cluster_of, has_io_pads, push_mode || flip_mode, guide_regions,
                );
                for (inst, location) in located {
                    macro_location.insert(inst, location);
                }
            }
            _ => {}
        }
    }

    if push_mode || flip_mode {
        print_boundary_push(
            root, &placed, &placed_area, &macro_location, design, h, &raw_soft, flip_mode,
            nets, flip, &available_abs,
        );
    }
}

/// Upstream `run()`'s post-placement push, driven from the placed tree.
///
/// ⛔ **The macro clusters come from `fetchMacroClusters`, not from the placement walk.** It
/// descends only through MIXED clusters, so a macro cluster reachable any other way is not pushed —
/// and the flat obstacle list it builds is in that same traversal order.
///
/// ⚠️ **The IO blockages are the ABSOLUTE ones**, as `createPinAccessBlockages` left them. The
/// outline-relative copies the placement problems carry would put every blockage a corner away
/// from the macros being tested against it.
fn print_boundary_push(
    root: &vyges_mpl::cluster::Cluster,
    placed: &std::collections::HashMap<i32, (i32, i32, i32, i32)>,
    placed_area: &std::collections::HashMap<i32, i64>,
    macro_location: &std::collections::HashMap<usize, (i32, i32)>,
    design: &vyges_mpl::design::Design,
    h: &vyges_mpl::engine::ShapingHandoff,
    io_blockages: &[(i32, i32, i32, i32)],
    flip_mode: bool,
    nets: &[vyges_mpl::netlist::DbNet],
    flip: &FlipInputs,
    available: &[vyges_mpl::placement::Region],
) {
    use vyges_mpl::placement as p;

    let by_id = |id: usize| -> Option<&vyges_mpl::cluster::Cluster> { sibling_of(root, id as i32) };
    // ⛔ **Upstream's `ClusterType`, not our `AreaKind`.** An IO cluster's type is `Mixed` — the
    // three `setAs*` IO setters never touch it — and a FIXED macro cluster's is `HardMacro`, so
    // both are gathered here while `AreaKind` would treat each as a kind of its own and drop them.
    let ids = p::fetch_macro_clusters(
        root.id as usize,
        &|id| by_id(id).map_or(vyges_mpl::cluster::ClusterType::StdCell, |c| c.cluster_type),
        &|id| by_id(id).map_or(Vec::new(), |c| c.children.iter().map(|k| k.id as usize).collect()),
    );

    // The flat obstacle list, in `fetchMacroClusters` order — and the clusters' indices into it.
    let mut macros: Vec<p::PushMacro> = Vec::new();
    let mut clusters: Vec<p::PushCluster> = Vec::new();
    // ⚠️ The instance behind each flat entry, so the orientation pass can read the POST-push
    // positions back out. `PushMacro` carries a name and a cluster id, neither of which indexes
    // the design.
    let mut macro_inst: Vec<usize> = Vec::new();
    for id in ids {
        let Some(c) = by_id(id) else { continue };
        let Some(&bbox) = placed.get(&c.id) else { continue };
        let mut indices = Vec::new();
        for &inst in &c.leaf_macros {
            let (width, height) = h.macro_dims[inst];
            indices.push(macros.len());
            macro_inst.push(inst);
            macros.push(p::PushMacro {
                name: design.instances[inst].name.clone(),
                cluster_id: c.id,
                location: macro_location.get(&inst).copied().unwrap_or((0, 0)),
                width: width as i32,
                height: height as i32,
            });
        }
        clusters.push(p::PushCluster {
            id: c.id,
            name: c.name.clone(),
            is_fixed_macro: c.is_fixed_macro,
            bbox,
            macros: indices,
        });
    }

    // ⛔ **The SOFT MACRO's area as CLUSTER PLACEMENT left it**, not the cluster's own and not the
    // one on the tree — `Cluster::soft_macro` is populated here only for IO clusters and fixed
    // macros, and neither is a kind this guard reads an area for. A standard-cell cluster fine
    // shaping shrank away reports zero through the placement result and nothing at all through the
    // tree, and reading the tree declined the push on every design with standard cells.
    let root_children: Vec<(vyges_mpl::cluster::ClusterType, i64)> = root
        .children
        .iter()
        .map(|c| {
            (c.cluster_type, p::soft_macro_area(placed_area.get(&c.id).copied().unwrap_or(0)))
        })
        .collect();

    let core = (
        design.core_area.x_min as i32,
        design.core_area.y_min as i32,
        design.core_area.x_max as i32,
        design.core_area.y_max as i32,
    );
    // ⚠️ On stderr, so it never reaches a harness diffing stdout. An empty trace has three possible
    // causes — either guard, or an empty cluster list — and they are indistinguishable in the
    // output itself.
    eprintln!(
        "[diag] push root={:?} children={:?} clusters={} decision={:?}",
        root.cluster_type,
        root_children,
        clusters.len(),
        p::push_decision(root.cluster_type, &root_children)
    );
    let push_trace = p::run_boundary_push(
        root.cluster_type,
        &root_children,
        &clusters,
        &mut macros,
        core,
        io_blockages,
    );
    if !flip_mode {
        for line in push_trace {
            println!("{line}");
        }
        return;
    }

    // ---------------------------------------------------------------- orientation improvement
    //
    // ⛔ **The cluster set here is NOT the pusher's.** `correctMacroOrientationByCluster` walks
    // `id_to_cluster` — every cluster in the tree, in ASCENDING ID — and takes each that is a
    // `HardMacroCluster` and not fixed. That INCLUDES THE ROOT, which in an all-macro design is
    // typed `HardMacroCluster` by MPL-27 and holds every macro. `macro_only` has ten macros and
    // emits fourteen lines per pass because of it.
    let mut by_id: Vec<&vyges_mpl::cluster::Cluster> = Vec::new();
    fn collect<'a>(c: &'a vyges_mpl::cluster::Cluster, out: &mut Vec<&'a vyges_mpl::cluster::Cluster>) {
        out.push(c);
        for k in &c.children {
            collect(k, out);
        }
    }
    collect(root, &mut by_id);
    by_id.sort_by_key(|c| c.id);

    // The flat macro list the flip groups index into. ⚠️ Built from the POST-PUSH positions.
    // 🔑 **The positions the PUSH left**, read back through the instance map. `run_boundary_push`
    // mutated `macros` in place, so these are post-push and pre-push only where nothing moved.
    let push_of: std::collections::HashMap<usize, (i32, i32)> =
        macro_inst.iter().zip(macros.iter()).map(|(&inst, m)| (inst, m.location)).collect();

    // ⛔ **Which cluster a macro POINTS AT, by `mapMacroInCluster2HardMacro`'s own call order:**
    // `id_to_cluster` ascending, ending in `setCluster`, so the HIGHEST-ID cluster holding a macro
    // owns it. ⚠️ Std-cell clusters claim nothing — that function returns before either source.
    let mut owner: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    for c in &by_id {
        for inst in hard_macros_of(c, design) {
            owner.insert(inst, c.name.clone());
        }
    }

    let mut flip_macros: Vec<p::FlipMacro> = Vec::new();
    let mut index_of: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut flip_clusters: Vec<p::FlipCluster> = Vec::new();
    for c in &by_id {
        if c.cluster_type != vyges_mpl::cluster::ClusterType::HardMacro || c.is_fixed_macro {
            continue;
        }
        let mut indices = Vec::new();
        for inst in hard_macros_of(c, design) {
            let idx = *index_of.entry(inst).or_insert_with(|| {
                let halo = h.macro_halos[inst];
                let (hx, hy) = push_of
                    .get(&inst)
                    .copied()
                    .unwrap_or_else(|| macro_location.get(&inst).copied().unwrap_or((0, 0)));
                flip_macros.push(p::FlipMacro {
                    name: design.instances[inst].name.clone(),
                    cluster_name: owner
                        .get(&inst)
                        .cloned()
                        .unwrap_or_else(|| design.instances[inst].name.clone()),
                    location: (hx, hy),
                    // ⛔ `getRealX` = `x_ + halo.left` at R0 — the halo comes OFF, and the trace
                    // still reports the haloed corner above.
                    real_location: (hx + halo.left as i32, hy + halo.bottom as i32),
                });
                flip_macros.len() - 1
            });
            indices.push(idx);
        }
        if indices.is_empty() {
            continue;
        }
        flip_clusters.push(p::FlipCluster { id: c.id, name: c.name.clone(), macros: indices });
    }

    // ⚠️ **Wirelength is NOT modelled yet.** Reporting zero is correct for the 80 of 135 reference
    // lines whose macros carry no signal net, and wrong for the other 55 — the gate attributes
    // those by design rather than hiding them.
    // ---------------------------------------------------------------- the flip wirelength
    //
    // ⛔ **Macros start at `R0`, whatever the DEF says.** `HardMacro`'s constructor never reads the
    // instance's orientation — it keeps the member default — and `updateMacroOnDb` then OVERWRITES
    // the database with it. So the flip pass begins from `R0` on every macro, and no orientation
    // needs reading back for them. ⚠️ Other instances (pads, cells) DO keep their DEF orientation,
    // and their terminals still count towards a macro's nets.
    let mut orient: Vec<p::DbOrient> = vec![p::DbOrient::R0; flip_macros.len()];
    let mut halo_of: Vec<(i32, i32, i32, i32)> = vec![(0, 0, 0, 0); flip_macros.len()];
    // ⚠️ The MASTER's own size, not the haloed one — the transform acts on master geometry.
    let mut master_dims: Vec<(i32, i32)> = vec![(0, 0); flip_macros.len()];
    for (&inst, &idx) in &index_of {
        let h2 = h.macro_halos[inst];
        halo_of[idx] = (h2.left as i32, h2.bottom as i32, h2.right as i32, h2.top as i32);
        let (w, ht) = h.macro_dims[inst];
        master_dims[idx] = (
            (w - h2.left - h2.right) as i32,
            (ht - h2.bottom - h2.top) as i32,
        );
    }
    let inst_of: Vec<usize> = {
        let mut v = vec![usize::MAX; flip_macros.len()];
        for (&inst, &idx) in &index_of {
            v[idx] = inst;
        }
        v
    };


    // Each macro's SIGNAL terminals that carry a net, and the net they are on.
    let mut macro_pins: Vec<Vec<(String, usize)>> = vec![Vec::new(); flip_macros.len()];
    for (net_index, net) in nets.iter().enumerate() {
        for t in &net.iterms {
            if let Some(&idx) = index_of.get(&t.inst) {
                macro_pins[idx].push((t.term.clone(), net_index));
            }
        }
    }

    let boxes_of = |inst: usize, term: &str| -> Vec<(i32, i32, i32, i32)> {
        flip.master_of
            .get(inst)
            .and_then(|m| flip.term_boxes.get(&(m.clone(), term.to_string())))
            .cloned()
            .unwrap_or_default()
    };
    let net_terms = |n: usize| -> Vec<p::FlipNetTerm> {
        let mut out = Vec::new();
        for t in &nets[n].iterms {
            out.push(p::FlipNetTerm::Instance { inst: t.inst, term: t.term.clone() });
        }
        for b in &nets[n].bterms {
            out.push(p::FlipNetTerm::Port(b.bterm));
        }
        out
    };
    let port = |i: usize| -> Option<p::FlipPort> {
        flip.pins.get(i).map(|q| p::FlipPort {
            is_fixed: q.is_fixed,
            bbox: (q.bbox.x_min as i32, q.bbox.y_min as i32, q.bbox.x_max as i32, q.bbox.y_max as i32),
        })
    };
    let die = h.die;
    let port_region = |i: usize| -> Option<p::Region> {
        flip.pins.get(i).and_then(|q| q.constraint).map(|r| p::Region {
            x0: r.x_min as i32,
            y0: r.y_min as i32,
            x1: r.x_max as i32,
            y1: r.y_max as i32,
            boundary: vyges_mpl::regions::boundary_of(&die, &r),
        })
    };

    let use_full_halo = std::env::args().any(|a| a == "--use-full-halo");
    match p::orientation_strategy(use_full_halo) {
        p::OrientationStrategy::Single => {
            // ⛔ **A DIFFERENT trace line, and only ONE design in the suite reaches it** —
            // `halos5`, the only case setting `-use_full_halo`.
            //
            // ⚠️ **The order is a PROXY.** Upstream walks `tree_->maps.inst_to_hard`, an
            // `odb::PtrMap` keyed by `dbInst*`, so its iteration order is POINTER order — stable
            // within a run and formally unspecified. Database order is the closest thing we can
            // reproduce, and it agrees on `halos5`. A design where the two disagree would differ
            // here and nothing in the suite would say so.
            let mut unfixed: Vec<usize> = index_of.keys().copied().collect();
            unfixed.sort_unstable();
            let unfixed: Vec<usize> = unfixed
                .into_iter()
                .filter(|&inst| !design.instances[inst].is_fixed)
                .map(|inst| index_of[&inst])
                .collect();
            let mut zero = |_m: usize, _v: bool| (0.0f32, 0.0f32);
            for line in p::run_orientation_single(&flip_macros, &unfixed, &mut zero) {
                println!("{line}");
            }
        }
        p::OrientationStrategy::ByCluster => {
            // ⛔ **`adjustRealMacroOrientation` is a GROUP operation, not a per-macro one.** It sums
            // the group's wirelength, flips EVERY member, sums again, and reverts the whole group
            // if it got worse. Flipping members one at a time would judge each against a board the
            // others have not moved on.
            //
            // ⚠️ **A flip moves the instance ORIGIN**, because `getRealX`/`getRealY` take the halo
            // off a different side once mirrored — so the origin is recomputed on every flip, not
            // carried.
            let measure = |orient: &[p::DbOrient], group: &[usize]| -> f32 {
                group
                    .iter()
                    .map(|&m| {
                        let inst = inst_of[m];
                        p::flip_macro_wirelength(
                            &macro_pins[m],
                            &net_terms,
                            &|i| {
                                index_of.get(&i).map_or_else(
                                    // ⛔ A non-macro instance keeps its DATABASE placement, read
                                    // back in full. Nothing in this stage moves it, but that is
                                    // not the same as it sitting at `R0` on the origin: `io_pads1`
                                    // fixes `PAD_1` at `W`, and assuming `R0` put its terminal on
                                    // the wrong side of the die and inverted the flip decision.
                                    || flip.inst_placement.get(i).copied(),
                                    |&k| {
                                        // ⛔ TWO steps, and conflating them is wrong on every
                                        // flip: `real_origin` says WHERE THE BOX GOES (the halo
                                        // comes off a different side once mirrored), and
                                        // `instance_offset` turns that into the transform's
                                        // offset, which for a mirrored master differs by its
                                        // width.
                                        let at = p::real_origin(
                                            flip_macros[k].location,
                                            halo_of[k],
                                            orient[k],
                                        );
                                        Some((
                                            orient[k],
                                            p::instance_offset(at, master_dims[k], orient[k]),
                                        ))
                                    },
                                )
                            },
                            &boxes_of,
                            &port,
                            &port_region,
                            available,
                            inst,
                        )
                    })
                    .sum()
            };
            let mut wirelength_of = |group: &[usize], is_vertical: bool| -> (f32, f32) {
                let original = measure(&orient, group);
                for &m in group {
                    orient[m] = p::flip_db_orientation(orient[m], is_vertical);
                }
                let new = measure(&orient, group);
                // ⚠️ `>` strictly — a TIE KEEPS THE FLIP, so only a strict worsening reverts.
                if !p::keep_flip(original, new) {
                    for &m in group {
                        orient[m] = p::flip_db_orientation(orient[m], is_vertical);
                    }
                }
                (original, new)
            };
            for line in
                p::run_orientation_by_cluster(&flip_clusters, &flip_macros, &mut wirelength_of)
            {
                println!("{line}");
            }
        }
    }
}

/// A cluster's `getHardMacros()`, via [`p::cluster_hard_macros`].
///
/// ⚠️ Not `leaf_macros` — an all-macro ROOT has none of its own and every macro through its module.
fn hard_macros_of(
    c: &vyges_mpl::cluster::Cluster,
    design: &vyges_mpl::design::Design,
) -> Vec<usize> {
    vyges_mpl::placement::cluster_hard_macros(
        c.cluster_type,
        &c.leaf_macros,
        &c.db_modules,
        &|m| design.modules[m].insts.clone(),
        &|m| design.modules[m].children.clone(),
        &|i| design.instances[i].is_block,
    )
}

/// Depth-first lookup of any cluster by id.
fn sibling_of(
    root: &vyges_mpl::cluster::Cluster,
    id: i32,
) -> Option<&vyges_mpl::cluster::Cluster> {
    if root.id == id {
        return Some(root);
    }
    root.children.iter().find_map(|c| sibling_of(c, id))
}

/// Upstream `placeMacros` for ONE hard-macro cluster, and its five-row summary.
///
/// ⛔ **The outline is the box CLUSTER placement gave this cluster**, not its shaping box — which
/// is why the caller must have written back before descending.
#[allow(clippy::too_many_arguments)]
/// Runs `placeMacros` on one macro cluster.
///
/// 🔑 **Returns each macro's ABSOLUTE haloed corner** — `placeMacros`' closing loop shifts every
/// hard macro by the outline's corner, and those are the positions the boundary push then reads
/// and moves. Discarding them leaves the pusher measuring against the macros' pre-placement
/// coordinates, which is a different design.
///
/// ⚠️ **Empty on every early return.** A cluster that never reached the search keeps whatever the
/// database gave it, which is what upstream's `HardMacro` also holds in that case.
fn emit_macro_summary(
    cluster: &vyges_mpl::cluster::Cluster,
    placed: &std::collections::HashMap<i32, (i32, i32, i32, i32)>,
    design: &vyges_mpl::design::Design,
    h: &vyges_mpl::engine::ShapingHandoff,
    dbu: i32,
    weights: vyges_mpl::anneal::SoftWeights,
    probabilities: vyges_mpl::anneal::ActionProbabilities,
    root: &vyges_mpl::cluster::Cluster,
    geometry: &[Option<vyges_mpl::read::MacroGeometry>],
    available: &[vyges_mpl::placement::Region],
    nets_in: &[vyges_mpl::netlist::DbNet],
    assoc: &[Option<i32>],
    pin_cluster_of: &dyn Fn(usize) -> Option<i32>,
    has_io_pads: bool,
    push_mode: bool,
    guide_regions: &[(usize, (i32, i32, i32, i32))],
) -> Vec<(usize, (i32, i32))> {
    use vyges_mpl::placement as p;
    let Some(&(x0, y0, x1, y1)) = placed.get(&cluster.id) else { return Vec::new() };
    let outline = (x1 - x0, y1 - y0);
    if outline.0 <= 0 || outline.1 <= 0 {
        return Vec::new();
    }

    // The cluster's hard macros, in its OWN coordinates — `createTempMacroClusters`' view.
    //
    // ⛔ `getHardMacros()`, NOT `leaf_macros`. An all-macro ROOT owns no leaf macro and reaches
    // every macro through its module; reading the leaf list places nothing at all there.
    let hard = hard_macros_of(cluster, design);
    let mut macros = Vec::new();
    let mut masters: Vec<usize> = Vec::new();
    for &inst in &hard {
        let b = h.macro_bboxes[inst];
        macros.push(vyges_mpl::anneal::SoftMacro {
            x: b.x_min as i32 - x0,
            y: b.y_min as i32 - y0,
            width: (b.x_max - b.x_min) as i32,
            height: (b.y_max - b.y_min) as i32,
            fixed: design.instances[inst].is_fixed,
            area: (b.x_max - b.x_min) * (b.y_max - b.y_min),
            is_macro_cluster: true,
        });
        masters.push(design.instances[inst].master_id);
    }
    if macros.is_empty() {
        return Vec::new();
    }
    masters.sort_unstable();
    masters.dedup();

    let n = macros.len();

    // `computeArraySequencePair`: the grid the macros already sit in, from the CLUSTER's box and
    // the FIRST macro's haloed size. ⚠️ It also reports whether the grid has cells the macros do
    // not fill — which decides whether invalid states are allowed, or the actions collapse to
    // exchange only.
    let array = if cluster.is_macro_array {
        p::array_sequence_pair(n, outline.0, outline.1, macros[0].width, macros[0].height)
    } else {
        Default::default()
    };

    // ⛔ **`rebuildConnections` runs with the TEMP clusters in the association** — one per hard
    // macro — so a net between two macros of the same cluster becomes a net between two temp
    // clusters. Reusing the parent-level connections would collapse them all onto one id and
    // score no wirelength at all, which is what an unassembled problem looks like.
    let first_temp_id = 1_000_000;
    let mut temp_assoc: Vec<Option<i32>> = assoc.to_vec();
    for (k, &inst) in hard.iter().enumerate() {
        temp_assoc[inst] = Some(first_temp_id + k as i32);
    }
    let connections = vyges_mpl::netlist::build_connections(
        nets_in,
        design,
        &|i| temp_assoc.get(i).copied().flatten(),
        pin_cluster_of,
        has_io_pads,
        50,
    );

    // `createFixedTerminals`: every connected cluster that is not one of these macros becomes a
    // fixed terminal, appended AFTER the sequence pair.
    let mut connected: Vec<i32> = Vec::new();
    for k in 0..n {
        for (id, _) in connections.of(first_temp_id + k as i32) {
            connected.push(id);
        }
    }
    let terminal_ids = p::hard_terminal_cluster_ids(&connected, &|id| {
        (first_temp_id..first_temp_id + n as i32).contains(&id)
    });
    let mut macro_of: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();
    for k in 0..n {
        macro_of.insert(first_temp_id + k as i32, k);
    }
    let mut macros = macros;
    for id in &terminal_ids {
        // ⛔ **An IO cluster is NOT in the placed map** — `updateChildrenShapesAndLocations` skips
        // it on purpose, because its position is already absolute and must not be overwritten. So
        // a terminal's geometry comes from the cluster's OWN soft macro when it has one, and from
        // the placed box otherwise. Reading only the map drops every net to an IO cluster, which
        // scores wirelength zero on any design whose macros talk to the outside world.
        let sibling = sibling_of(root, *id);
        let (tx0, ty0, tx1, ty1, is_unplaced_ios) = match sibling.and_then(|c| {
            c.soft_macro.map(|m| {
                (m.x, m.y, m.x + m.width, m.y + m.height, c.is_cluster_of_unplaced_io_pins)
            })
        }) {
            Some(v) => v,
            None => match placed.get(id) {
                Some(&(a, b, c2, d)) => (a, b, c2, d, false),
                None => continue,
            },
        };
        macro_of.insert(*id, macros.len());
        macros.push(p::fixed_terminal(
            &p::TerminalCluster {
                center: ((tx0 + tx1) / 2, (ty0 + ty1) / 2),
                origin: (tx0, ty0),
                width: tx1 - tx0,
                height: ty1 - ty0,
                is_cluster_of_unplaced_io_pins: is_unplaced_ios,
            },
            (x0, y0),
        ));
    }

    let per_cluster: Vec<(i32, Vec<(i32, f32)>)> = (0..n)
        .map(|k| {
            let id = first_temp_id + k as i32;
            (id, connections.of(id))
        })
        .collect();
    let nets = p::build_bundled_nets_for_macros(&per_cluster, &|id| macro_of.get(&id).copied())
        .unwrap_or_default();

    let total = macros.len();

    // ⛔ **The terminals' IO flags and constraint regions are what the wirelength model branches
    // on.** A net whose TARGET is a cluster of unplaced IO pins takes the region path, not the
    // half-perimeter one, so a terminal left with default attributes scores zero however correct
    // the net is.
    let mut attributes = vec![p::MacroAttributes::default(); total];
    // ⛔ **`HardMacro::getPinX` is `x_ + pin_x_`, not the macro's centre.** `pin_x_` is the centre
    // of the master's SIGNAL pin bounding box plus HALF THE TOTAL halo — note it is
    // `(left + right) / 2`, not `left`, which is upstream's own expression and not an alignment
    // of the pin onto the haloed box. The two coincide only when the pins sit in the middle.
    for (k, &inst) in hard.iter().enumerate() {
        let Some(g) = geometry[inst].as_ref() else { continue };
        let mut lo = (i64::MAX, i64::MAX);
        let mut hi = (i64::MIN, i64::MIN);
        for pin in &g.pins {
            lo = (lo.0.min(pin.x_min), lo.1.min(pin.y_min));
            hi = (hi.0.max(pin.x_max), hi.1.max(pin.y_max));
        }
        if lo.0 > hi.0 {
            continue;
        }
        // `(left + right)` is the haloed width less the master's, which is what we can recover.
        let halo_x = h.macro_dims[inst].0 - g.master_width;
        let halo_y = h.macro_dims[inst].1 - g.master_height;
        attributes[k].pin_offset = Some((
            ((lo.0 + hi.0) / 2 + halo_x / 2) as i32,
            ((lo.1 + hi.1) / 2 + halo_y / 2) as i32,
        ));
    }
    let mut constraint_regions: Vec<(usize, p::Region)> = Vec::new();
    for id in &terminal_ids {
        let Some(&index) = macro_of.get(id) else { continue };
        let Some(sib) = sibling_of(root, *id) else { continue };
        attributes[index].is_cluster_of_unplaced_io_pins = sib.is_cluster_of_unplaced_io_pins;
        attributes[index].is_unconstrained_io_cluster = sib.is_cluster_of_unconstrained_io_pins;
        if let Some(r) = sib.constraint_region {
            // ⚠️ ABSOLUTE, like the cluster path — the asymmetry with the available regions is
            // upstream's and is documented on `ParentContext::constraint_region_of`.
            constraint_regions.push((
                index,
                p::Region {
                    x0: r.x_min as i32,
                    y0: r.y_min as i32,
                    x1: r.x_max as i32,
                    y1: r.y_max as i32,
                    boundary: vyges_mpl::regions::boundary_of(&design.die_area, &r),
                },
            ));
        }
    }
    // ⛔ **`computeFencesAndGuides`, the MACRO-path overload — and it was never called.**
    // `placeMacros` computes them at step 4, keyed by the macro's INDEX in `hard_macros`, from
    // `guides_[inst]` clipped to THIS cluster's outline and rebased onto it.
    //
    // ⚠️ **No area test here.** The cluster path drops a fence or guide whose clipped area is zero;
    // this overload keeps every entry it finds. A guide that misses the outline entirely becomes a
    // zero rect at the origin rather than being absent, which is what upstream's `intersection`
    // writes on a miss.
    //
    // 🔑 Without this the guidance penalty is identically zero on the macro path, and `guides2` —
    // whose whole point is a guidance weight of 30 outweighing wirelength — places its macros
    // somewhere the reference does not.
    let guide_for_macro: std::collections::HashMap<usize, (i32, i32, i32, i32)> =
        guide_regions.iter().copied().collect();
    let (_, guides) = p::macro_fences_and_guides(
        // ℹ️ `set_macro_fence` is not exercised by the suite; the fence half stays empty rather
        // than being invented.
        &|_| None,
        &|k| hard.get(k).and_then(|inst| guide_for_macro.get(inst).copied()),
        n,
        (x0, y0, x1, y1),
    );

    let problem = p::MacroProblem {
        macros,
        number_of_sequence_pair_macros: n,
        inputs: p::PlacementInputs {
            attributes,
            nets,
            guides,
            constraint_regions,
            available_regions: available
                .iter()
                .map(|r| p::Region {
                    x0: r.x0 - x0,
                    y0: r.y0 - y0,
                    x1: r.x1 - x0,
                    y1: r.y1 - y0,
                    ..*r
                })
                .collect(),
            die_margin: 2 * ((design.die_area.x_max - design.die_area.x_min)
                + (design.die_area.y_max - design.die_area.y_min)),
            root: p::Root { x: x0, y: y0, width: outline.0, height: outline.1 },
            weights,
            ..Default::default()
        },
        outline,
        dbu_per_micron: dbu,
        is_macro_array: cluster.is_macro_array,
        array_has_empty_space: array.has_empty_space,
        initial_sequence_pair: if cluster.is_macro_array {
            Some(vyges_mpl::anneal::SequencePair { pos: array.pos.clone(), neg: array.neg.clone() })
        } else {
            None
        },
        master_count: masters.len(),
    };

    // ℹ️ `placeMacros` derives its own perturbation count inside `place_macros`; the other SA
    // hyperparameters are the command defaults.
    let params = vyges_mpl::anneal::SaParameters::default();
    let Some(search) = p::place_macros(&problem, weights, probabilities, &params, 10, 0) else {
        return Vec::new();
    };
    if !push_mode {
        println!("Macro Placement Summary");
        print!(
            "{}",
            p::macro_placement_summary(
                cluster.id,
                (x0, y0, x1, y1),
                &search.penalties,
                &search.weights,
                &search.normalization,
                search.area_penalty(),
                search.norm_cost(),
                dbu,
            )
        );
    }

    // ⛔ **`placeMacros`' closing loop, and it runs whether or not a run was selected.** The shift
    // is by the CLUSTER's outline corner — the box cluster placement gave it — because everything
    // inside a macro problem is outline-relative.
    // ⚠️ Only the first `n` entries are the cluster's own macros; the rest are fixed terminals.
    hard.iter()
        .enumerate()
        .filter_map(|(k, &inst)| search.macros.get(k).map(|m| (inst, (m.x + x0, m.y + y0))))
        .collect()
}

/// `micronsToDbu`: multiply by the tech's units per micron and **round**, not truncate.
/// A guidance region in microns, as `add_guidance_region` converts it.
///
/// ⚠️ **The SWIG entry point takes `float`, not `double`** — the Tcl value is narrowed to single
/// precision before `micronsToDbu` ever sees it. Converting from `f64` would be more accurate than
/// the reference and is therefore a different function; the narrowing is spelled out here.
fn region_to_dbu(r: [f64; 4], dbu: i32) -> (i32, i32, i32, i32) {
    let d = |v: f64| ((v as f32) as f64 * dbu as f64).round() as i32;
    (d(r[0]), d(r[1]), d(r[2]), d(r[3]))
}

/// The `rtl_macro_placer` options that are ENGINE state and cannot ride on a prepared `.odb`.
#[derive(Debug, Clone, Copy)]
struct Overrides {
    boundary: Option<f32>,
    notch: Option<f32>,
    guidance: Option<f32>,
    target_util: f32,
}

impl Overrides {
    fn weights(&self) -> vyges_mpl::anneal::SoftWeights {
        let mut w = vyges_mpl::anneal::SoftWeights::placement_defaults();
        if let Some(v) = self.boundary {
            w.boundary = v;
        }
        if let Some(v) = self.notch {
            w.notch = v;
        }
        if let Some(v) = self.guidance {
            w.guidance = v;
        }
        w
    }
}

fn next_f32(it: &mut std::slice::Iter<'_, String>, flag: &str) -> f32 {
    match it.next().and_then(|v| v.parse().ok()) {
        Some(v) => v,
        None => usage(&format!("{flag} needs a number")),
    }
}

fn next_i32(it: &mut std::slice::Iter<'_, String>, flag: &str) -> i32 {
    match it.next().and_then(|v| v.parse().ok()) {
        Some(v) => v,
        None => usage(&format!("{flag} needs an integer")),
    }
}

fn to_dbu(h: [f64; 4], dbu: i32) -> vyges_mpl::options::Halo {
    let d = |v: f64| (v * dbu as f64).round() as i64;
    vyges_mpl::options::Halo { left: d(h[0]), bottom: d(h[1]), right: d(h[2]), top: d(h[3]) }
}

/// Four comma-separated microns, or two mirrored into four — `parse_halo`'s own rule.
fn four(spec: &str) -> [f64; 4] {
    let v: Vec<f64> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    match v.len() {
        2 => [v[0], v[1], v[0], v[1]],
        4 => [v[0], v[1], v[2], v[3]],
        _ => usage("a halo needs 2 or 4 values, in microns"),
    }
}

fn usage(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}
