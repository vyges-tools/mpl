// SPDX-License-Identifier: Apache-2.0
//! Every `rtl_macro_placer` case in upstream's own suite, re-run at our pin under
//! `set_debug_level MPL multilevel_autoclustering 1`, with the thresholds it reported.
//!
//! ⚠️ **This table is a weak oracle, and saying so is the point.** 34 of the 35 cases produce the
//! IDENTICAL outcome — every suite design has fewer than 150 macros and a small cell count, so all
//! of them floor to the same four numbers. Only `keep_clustering_data2` differs, because it is the
//! one case that supplies thresholds on the command line.
//!
//! 🔑 So the suite confirms the common path across 35 designs and exercises the derivation
//! arithmetic **barely at all**. The real coverage of the level scaling, the degenerate floors and
//! the truncation lives in `tests/thresholds.rs`, which is hand-written. A green run here is not
//! evidence that the derivation is right.
use vyges_mpl::thresholds::{set_base_thresholds, DesignMetrics, Thresholds as T};

struct C {
    name: &'static str,
    std: i32,
    mac: i32,
    fixed: bool,
    sup: T,
    ratio: f32,
    levels: i32,
    want_level: i32,
    want: T,
}

const CASES: &[C] = &[
    C { name: "boundary_push1", std: 0, mac: 4, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "centralization1", std: 0, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "clocked_macro", std: 0, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "fixed_covers", std: 150, mac: 2, fixed: true, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "fixed_ios1", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "fixed_macros1", std: 150, mac: 2, fixed: true, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "fixed_macros2", std: 400, mac: 2, fixed: true, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "guides1", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "guides2", std: 0, mac: 10, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "halos1", std: 150, mac: 2, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "halos2", std: 150, mac: 2, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "halos3", std: 0, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "halos4", std: 0, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "halos5", std: 150, mac: 2, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints10", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints1", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints2", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints3", std: 200, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints4", std: 400, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints5", std: 400, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints6", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints7", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints8", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_constraints9", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "io_pads1", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "keep_clustering_data1", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "keep_clustering_data2", std: 8, mac: 2, fixed: false, sup: T { max_macro: 2, min_macro: 1, max_std_cell: 6, min_std_cell: 2 }, ratio: 10.0, levels: 2, want_level: 2, want: T { max_macro: 20, min_macro: 10, max_std_cell: 60, min_std_cell: 20 } },
    C { name: "macro_only", std: 0, mac: 10, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "macros_without_pins1", std: 400, mac: 2, fixed: true, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "mixed_ios1", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "orientation_improve1", std: 0, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "orientation_improve2", std: 0, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "orientation_improve3", std: 0, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "placement_blockages1", std: 150, mac: 1, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },
    C { name: "probe", std: 150, mac: 2, fixed: false, sup: T { max_macro: 0, min_macro: 0, max_std_cell: 0, min_std_cell: 0 }, ratio: 10.0, levels: 2, want_level: 1, want: T { max_macro: 5, min_macro: 1, max_std_cell: 5000, min_std_cell: 1000 } },];

#[test]
fn every_upstream_case_reproduces_its_reported_thresholds() {
    let mut distinct = std::collections::BTreeSet::new();
    for c in CASES {
        let got = set_base_thresholds(
            c.sup,
            DesignMetrics { num_macro: c.mac, num_std_cell: c.std },
            c.ratio,
            c.levels,
            c.fixed,
        );
        assert_eq!(got.max_level, c.want_level, "{}: level", c.name);
        assert_eq!(got.thresholds.max_macro, c.want.max_macro, "{}: max_macro", c.name);
        assert_eq!(got.thresholds.min_macro, c.want.min_macro, "{}: min_macro", c.name);
        assert_eq!(got.thresholds.max_std_cell, c.want.max_std_cell, "{}: max_inst", c.name);
        assert_eq!(got.thresholds.min_std_cell, c.want.min_std_cell, "{}: min_inst", c.name);
        distinct.insert((
            got.max_level,
            got.thresholds.max_macro,
            got.thresholds.min_macro,
            got.thresholds.max_std_cell,
            got.thresholds.min_std_cell,
        ));
    }
    assert_eq!(CASES.len(), 35, "every rtl_macro_placer case that reported thresholds");

    // ⚠️ Pinned deliberately. If a future pin makes the suite more varied this fails, and that is
    // GOOD news to be told about -- it means the oracle got stronger. If it ever drops to 1, the
    // one case carrying variation has stopped arriving.
    assert_eq!(
        distinct.len(),
        2,
        "the suite exercises only two distinct threshold outcomes: {distinct:?}"
    );
}
