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
    let mut use_full_halo = false;
    let mut base_halo: Option<[f64; 4]> = None;
    let mut macro_halos: Vec<(String, [f64; 4])> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--report" => want_report = true,
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
            "usage: cluster-dump [--report] [--use-full-halo] [--base-halo l,b,r,t] \
             [--macro-halo NAME=l,b,r,t] <design.odb>   (halos in microns)",
        )
    };

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
    print!("{}", vyges_mpl::dump::physical_hierarchy(&r.root));
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
