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
            other if !other.starts_with("--") => path = Some(other.to_string()),
            other => usage(&format!("unknown option {other}")),
        }
    }
    let Some(path) = path else {
        usage(
            "usage: cluster-dump [--report|--shaping] [--use-full-halo] \
             [--base-halo l,b,r,t] [--macro-halo NAME=l,b,r,t] <design.odb>   (halos in microns)",
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
        print_coarse_shaping_trace(r, &blockages, dbu, place, summaries, &design, &nets);
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
    r: vyges_mpl::engine::Clustering,
    blockages: &[vyges_mpl::design::Rect],
    dbu: i32,
    place: bool,
    summaries: bool,
    design: &vyges_mpl::design::Design,
    nets: &[vyges_mpl::netlist::DbNet],
) {
    use vyges_mpl::cluster::ClusterType;
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
            &|_| None,
            h.has_io_pads,
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

    let (weights, tiny) =
        p::placement_setup(1, vyges_mpl::anneal::SoftWeights::placement_defaults(), 0);
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
        vyges_mpl::anneal::ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
        weights,
        0,
        &mut |parent| {
            let ctx = p::ParentContext {
                connections_of: &|_| Vec::new(),
                virtual_connections: &[],
                blockages: &blockages,
                soft_blockages: &soft,
                fence_of: &|_| None,
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
) {
    use vyges_mpl::placement as p;

    // 🔑 **Rebuilt from the association as it stands**, which is what `rebuildConnections` does
    // inside `placeChildren` — not a map built once for the whole design.
    let assoc = vyges_mpl::tree::associate_instances(root, design);
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
    let (weights, tiny) =
        p::placement_setup(1, vyges_mpl::anneal::SoftWeights::placement_defaults(), 0);
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

    let mut emit = |parent: &vyges_mpl::cluster::Cluster| {
        let ctx = p::ParentContext {
            connections_of: &|id| connections.of(id),
            virtual_connections: &[],
            blockages: &blockages,
            soft_blockages: &soft,
            fence_of: &|_| None,
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
                vyges_mpl::anneal::ActionProbabilities::normalized(0.2, 0.2, 0.2, 0.2, 0.2),
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
