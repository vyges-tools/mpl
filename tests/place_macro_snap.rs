// SPDX-License-Identifier: Apache-2.0
//! `place-macro --snap`: the reference's own `place_macro` (`MacroPlacer::placeMacro`), as
//! OpenROAD-flow-scripts runs it, rather than LibreLane's manual placement.
//!
//! Reference rule (pin 4155ac31, `mpl/src/rtl_mp.cpp`): micronsToDbu of the float location; the
//! orientation is set; the oriented box must lie inside the core (MPL-34, an error); the location
//! is set; a right-angle rotation is not snapped (MPL-36, a warning); otherwise, unless `-exact`,
//! `Snapper::snapMacro` moves the macro so its pins sit on their layers' tracks; the macro is
//! left LOCKED.
//!
//! Oracle: the reference on `tests/fixtures/place_macro_snap.odb` (upstream's
//! `place_macro_right_angle_rotation` macro, Nangate45 tech, re-floorplanned to a 1000 um die,
//! `make_tracks`), one fresh run per case.
//!
//! ⚠️ The fixture is not tracked (`tests/fixtures/*.odb`, as `counter.odb`); CI skips this target by
//! name. Regenerate it with the reference at the pin, from its `src/mpl/test` directory:
//! `read_lef Nangate45/Nangate45_tech.lef; read_lef testcases/place_macro_right_angle_rotation.lef;
//! read_def testcases/place_macro_right_angle_rotation.def; initialize_floorplan -die_area {0 0 1000
//! 1000} -core_area {10 10 990 990} -site FreePDK45_38x28_10R_NP_162NW_34O; make_tracks; write_db`.

use std::path::PathBuf;
use std::process::Command;

use vyges_opendb::Db;

const FIXTURE: &str = "tests/fixtures/place_macro_snap.odb";

fn out(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("vyges-mpl-pm-snap-{}-{tag}.odb", std::process::id()))
}

/// Run `place-macro` on the fixture; the exit code and, on success, (x, y, orient, status).
fn place(tag: &str, spec: &str, flags: &[&str]) -> (i32, Option<Placed>) {
    let o = out(tag);
    let mut args = vec!["place-macro", FIXTURE, "--macro", spec, "--out-odb", o.to_str().unwrap()];
    args.extend_from_slice(flags);
    let st = Command::new(env!("CARGO_BIN_EXE_vyges-mpl")).args(&args).output().expect("run vyges-mpl");
    let code = st.status.code().unwrap_or(-1);
    if code != 0 {
        return (code, None);
    }
    let db = Db::open(o.to_str().unwrap()).unwrap();
    let got = (
        db.inst_get_location_x("macro"),
        db.inst_get_location_y("macro"),
        db.inst_get_orient("macro"),
        db.inst_get_placement_status("macro"),
    );
    (code, Some(got))
}

/// Where the macro ended: x, y (DBU), orientation, placement status.
type Placed = (i32, i32, String, String);

fn at(x: i32, y: i32, o: &str) -> Placed {
    (x, y, o.to_string(), "LOCKED".to_string())
}

/// The reference's results, one per case (location in DBU at 2000 per micron).
#[test]
fn snapped_placements_match_the_reference() {
    let cases: [(&str, &[&str], Placed); 8] = [
        ("macro=20.003,20.007,R0", &[], at(40010, 40230, "R0")),
        ("macro=20.003,20.007,MY", &[], at(40010, 40230, "MY")),
        ("macro=20.003,20.007,R180", &[], at(40010, 40090, "R180")),
        ("macro=20.003,20.007,MX", &[], at(40010, 40090, "MX")),
        // A right-angle rotation is not snapped (MPL-36).
        ("macro=20.003,20.007,R90", &[], at(40006, 40014, "R90")),
        // -exact: no snap.
        ("macro=20.003,20.007,R0", &["--exact"], at(40006, 40014, "R0")),
        ("macro=137.1234,501.0101,R0", &[], at(274250, 1002030, "R0")),
        ("macro=137.1234,501.0101,MY", &[], at(274250, 1002030, "MY")),
    ];
    for (k, (spec, extra, want)) in cases.iter().enumerate() {
        let mut flags = vec!["--snap"];
        flags.extend_from_slice(extra);
        let (code, got) = place(&format!("c{k}"), spec, &flags);
        assert_eq!(code, 0, "{spec} {extra:?}");
        assert_eq!(got.as_ref(), Some(want), "{spec} {extra:?}");
    }
}

/// MPL-34: a box outside the core is refused before anything moves.
#[test]
fn a_box_outside_the_core_is_refused() {
    let (code, _) = place("c34", "macro=950,20,R0", &["--snap"]);
    assert_eq!(code, 1);
}

/// Without `--snap` (LibreLane's step) the same request is not snapped and is left FIRM: the
/// assertions above are not vacuous.
#[test]
fn without_snap_the_location_is_taken_as_given() {
    let (code, got) = place("nosnap", "macro=20.003,20.007,R0", &[]);
    assert_eq!(code, 0);
    let (x, y, _, status) = got.unwrap();
    assert_eq!((x, y), (40006, 40014));
    assert_eq!(status, "FIRM");
}
