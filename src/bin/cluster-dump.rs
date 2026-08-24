// SPDX-License-Identifier: Apache-2.0
//! Print a design's physical hierarchy in upstream's own format, so the two can be diffed.
fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: cluster-dump <design.odb>");
        std::process::exit(2);
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

    let opts = vyges_mpl::engine::ClusterOptions::default();
    let r = vyges_mpl::engine::run_clustering(&design, &pins, &opts);
    if let Some(refusal) = &r.refusal {
        eprintln!("refused: {refusal:?}");
        std::process::exit(1);
    }
    print!("{}", vyges_mpl::dump::physical_hierarchy(&r.root));
}
