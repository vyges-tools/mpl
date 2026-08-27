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
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--report" => want_report = true,
            "--shaping" => want_shaping = true,
            "--place" => {}
            "--placement-trace" => {}
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
             [--macro-guide NAME=lx,ly,ux,uy] <design.odb>   (all values in microns)",
        )
    };

    let place = std::env::args().any(|a| a == "--place");
    let summaries = std::env::args().any(|a| a == "--placement-trace");
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
        print_coarse_shaping_trace(
            r, &blockages, dbu, place, summaries, &design, &nets, &guide_regions,
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
fn print_coarse_shaping_trace(
    mut r: vyges_mpl::engine::Clustering,
    blockages: &[vyges_mpl::design::Rect],
    dbu: i32,
    place: bool,
    summaries: bool,
    design: &vyges_mpl::design::Design,
    nets: &[vyges_mpl::netlist::DbNet],
    guide_regions: &[(usize, (i32, i32, i32, i32))],
) {
    use vyges_mpl::cluster::ClusterType;
    // Taken before `r` is dismantled below: a net reaches an IO cluster only through this.
    let pin_cluster = std::mem::take(&mut r.pin_cluster);
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
        );
    } else {
        report_placement(&root, &h, &shaping, dbu);
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
    let (weights, tiny, probabilities) = p::placement_setup(
        1,
        vyges_mpl::anneal::SoftWeights::placement_defaults(),
        0,
        h.has_std_cells,
    );
    let utilizations = p::utilization_list(0.25, 10);

    let blockages: Vec<(i32, i32, i32, i32)> = shaping
        .placement_blockages
        .iter()
        .map(|b| (b.x_min as i32, b.y_min as i32, b.x_max as i32, b.y_max as i32))
        .collect();
    let soft: Vec<(i32, i32, i32, i32)> = shaping
        .io_blockages
        .iter()
        .map(|b| (b.x_min as i32, b.y_min as i32, b.x_max as i32, b.y_max as i32))
        .collect();

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
    );

    for v in &visits {
        let what = match &v.outcome {
            p::ParentOutcome::Placed { run, macros } => {
                format!("placed, run {} of {}, {} macros", run.index, utilizations.len(), macros.len())
            }
            p::ParentOutcome::MacroCluster => "macro cluster".to_string(),
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
) {
    use vyges_mpl::placement as p;

    // 🔑 **Rebuilt from the association as it stands**, which is what `rebuildConnections` does
    // inside `placeChildren` — not a map built once for the whole design.
    let assoc = vyges_mpl::tree::associate_instances(root, design);

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
    let (weights, tiny, probabilities) = p::placement_setup(
        1,
        vyges_mpl::anneal::SoftWeights::placement_defaults(),
        0,
        h.has_std_cells,
    );
    let utilizations = p::utilization_list(0.25, 10);
    let blockages: Vec<(i32, i32, i32, i32)> = shaping
        .placement_blockages
        .iter()
        .map(|b| (b.x_min as i32, b.y_min as i32, b.x_max as i32, b.y_max as i32))
        .collect();
    let soft: Vec<(i32, i32, i32, i32)> = shaping
        .io_blockages
        .iter()
        .map(|b| (b.x_min as i32, b.y_min as i32, b.x_max as i32, b.y_max as i32))
        .collect();

    // The die-edge stretches an unconstrained IO pin may land on, as the placer wants them.
    let available: Vec<p::Region> = shaping
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

    let mut emit = |parent: &vyges_mpl::cluster::Cluster| {
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
        for (index, util) in utilizations.iter().enumerate() {
            let Some(search) = p::anneal_one_run(
                &problem,
                *util,
                0,
                &vyges_mpl::anneal::SaParameters::default(),
                probabilities,
                weights,
            ) else {
                continue;
            };
            let _ = index;
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
            return;
        }
    };

    // ⚠️ Only clusters the driver would have PLACED — the same guards, so the set matches.
    let mut stack = vec![root];
    while let Some(c) = stack.pop() {
        let action = p::placement_action(p::area_kind_of(c), c.is_fixed_macro, c.children.is_empty());
        if action == p::PlacementAction::PlaceChildren {
            emit(c);
            for child in c.children.iter().rev() {
                stack.push(child);
            }
        }
    }
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
