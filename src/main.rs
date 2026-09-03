// SPDX-License-Identifier: Apache-2.0
//! `vyges-mpl` CLI — hierarchical macro placement over a `.odb`.
//!
//! Exit status is the verdict: 0 applied, 1 refused, 2 usage/read/write error, 3 vacuous.

use std::process::ExitCode;
use vyges_mpl::pipeline::ORDER;

const USAGE: &str = "\
vyges physical mpl — hierarchical macro placement

USAGE:
  vyges physical mpl place-macro <design.odb> --macro NAME=X,Y[,ORIENT] [--macro ...]
                                 [--dbu] [--out-odb FILE]
  vyges physical mpl --describe
  vyges physical mpl --help

NOT WIRED:
  `run` -- the automatic hierarchical placement -- is NOT reachable from this command line and
  exits 2 if you ask for it. The pipeline IS implemented and correlated against OpenROAD; it is
  driven by the `cluster-dump` binary, which every correlation gate uses. What is missing is the
  entry point, not the algorithm. It is listed here rather than in USAGE because a usage line is
  a promise.

PLACE-MACRO:
  LibreLane `Classic` step 16, `Odb.ManualMacroPlacement` -- the manual placement a real harden
  flow uses. X,Y are the macro ORIGIN in MICRONS (as the MACROS config states them); pass --dbu
  to give database units instead. ORIENT is R0/R90/R180/R270/MX/MY/MXR90/MYR90, default R0.
  Each macro is left FIXED, which is the status LibreLane's step produces.

STATUS:
  0 applied   macros placed and committed
  1 refused   the design cannot be processed as asked (see LIMITS)
  2 usage     bad arguments, or the design could not be read or written
  3 vacuous   nothing to do -- no unfixed macros

LIMITS:
  This engine does not implement TritonPart (OpenROAD's `par`). A FLAT cluster -- one with no
  module children -- whose leaf standard cells exceed the level threshold is REFUSED rather
  than approximated. Upstream's own mpl suite never reaches that path; a large flat block does.
";

/// The machine-readable contract. ⚠️ **An assertion must name a field the engine actually
/// emits**, and the limit above must be stated here too — a consumer reads this, not the help.
fn describe() -> String {
    serde_json::json!({
        // ⛔ **`vyges-tool-descriptor/1.1`, and this engine was the only one of eight not on it.**
        // It published an ad-hoc shape with no `schema`, `name`, `summary`, `maturity` or
        // `provenance_limitations`, so `render-contracts.py` printed its published contract as
        // `### (unnamed)` with `Assertion: None`, and `vyges mcp` could not read it like the rest.
        "schema": "vyges-tool-descriptor/1.1",
        "name": "mpl",
        "summary": "hierarchical macro placement over the design database",
        // ⛔ **`structured`, and the ladder has exactly three rungs** — `discovered`, `structured`,
        // `workflow-validated`. ⚠️ **Anything else is not "more honest", it is INVALID**:
        // `Maturity::parse` returns `None` for an unknown word, the schema `enum` rejects it, and
        // an unparsed maturity degrades to `discovered`, at which point `can_assert()` is false
        // and the verdict is suppressed to `unknown` however well-formed the assertion is.
        //
        // 🔑 **The rung is about the shape of the EVIDENCE, not about feature completeness.** What
        // is unbuilt belongs in `provenance_limitations` below, not in this word. `structured` is
        // the honest rung here: the operation and result are versioned and normalised, but the
        // in-repo suite does not run the pipeline end to end against a pinned golden and assert —
        // `tests/goldens/` is not loaded by any test, and the nine correlation gates that DO
        // assert against upstream live outside this repository.
        "maturity": "structured",
        // ⛔ **The pin the BINARY was built against, not the one the repo names.** Taken from
        // `vyges_opendb`, which inherits it from `openroad-pin.yaml`, so it cannot drift from the
        // database layer this engine links.
        //
        // 🔑 **Correlation harnesses refuse to run without it.** `require_matching_pin` asks the
        // binary what it was built against and compares that with the oracle it is about to
        // launch; an engine that cannot answer gets a warning and the guard goes quiet — which is
        // precisely the case the guard exists for. On 2026-08-29 `mpl` was the only engine of six
        // that could not answer.
        //
        // ⚠️ **A score without a pin is not quotable.** Every number this engine produces is true
        // of one upstream commit; this field is what lets a reader tell which.
        "openroad_pin": vyges_opendb::OPENROAD_PIN,
        "version": env!("CARGO_PKG_VERSION"),
        // ⛔ **`place-macro`, because it is the command that RUNS.** This read
        // `["run", "{design}"]` while `run` was unwired and exited 2 — so an MCP client built
        // exactly that command and got a usage error. A contract's whole job is to say how to
        // invoke the tool; naming a command that refuses is worse than naming none.
        //
        // ⚠️ The auto-placement pipeline IS implemented and correlated — see the limitations —
        // but it is driven by `cluster-dump`, not by this entry point. When `run` is wired, this
        // is the line that moves, together with the first limitation below.
        "invocation": {
            "args_template": ["place-macro", "{design}"],
            "optional": [
                {"arg": "macro", "flag": "--macro"},
                {"arg": "dbu", "flag": "--dbu"},
                {"arg": "out_odb", "flag": "--out-odb"}
            ],
            "emits_json": true
        },
        "inputs": {
            "type": "object",
            "required": ["design", "macro"],
            "properties": {
                "design": {"type": "string", "description": "path to the design database (.odb)"},
                "macro": {"type": "string", "description": "NAME=X,Y[,ORIENT] — repeatable; X,Y is the macro ORIGIN in microns unless --dbu"},
                "dbu": {"type": "boolean", "description": "read --macro coordinates as database units, not microns"},
                "out_odb": {"type": "string", "description": "write the database here instead of in place"}
            }
        },
        // ⛔ **`odb`, not an engine list.** This previously read `["ifp", "pad", "ppl"]`, which
        // asserted that pin placement runs BEFORE macro placement. Upstream's own `test/flow.tcl`
        // does the opposite: `rtl_macro_placer` at line 37, `place_pins` at line 65 — after a
        // first `global_placement -skip_io`.
        //
        // 🔑 **`mpl` supports BOTH orders by design, and they give DIFFERENT placements.** A block
        // terminal that is already placed contributes its bbox centre; an unplaced one contributes
        // the nearest point of its constraint region. Both branches are transcribed and live. So a
        // fixed predecessor list is not just wrong, it is the wrong SHAPE of claim.
        //
        // ⚠️ Every other engine we ship declares the generic `["odb"]`. This was the outlier.
        "consumes": ["odb"],
        "produces": ["odb"],
        "artifacts": [{"role": "odb", "field": "out_odb"}],
        "reports": ["status", "macros_placed", "blockages_created", "refusal"],
        "assertion": {"id": "macros-placed", "field": "status", "pass_when": {"eq": "applied"}},
        "stages": ORDER.iter().map(|s| s.upstream_name()).collect::<Vec<_>>(),
        "limits": {
            "par_partitioning": "not implemented; a flat cluster over the level threshold is refused",
            "run_command": "the auto-placement pipeline is not reachable from this CLI; `run` is unwired",
        },
        // ⛔ **REQUIRED, and it is where the gaps go.** The maturity word is a rung on a
        // three-value ladder and cannot carry nuance; this can.
        "provenance_limitations": [
            "input_hash covers the argument vector, not the content of the .odb it names.",
            "THE `run` COMMAND IS NOT WIRED. This binary exposes `place-macro` only -- LibreLane `Classic` step 16, `Odb.ManualMacroPlacement`, the manual placement a real harden flow uses. The automatic hierarchical pipeline IS implemented and correlated, but it is driven by the `cluster-dump` binary, which is what every correlation gate uses. `place-macro` places the macros it is told to place and asserts nothing about where they should go.",
            "TritonPart (OpenROAD's `par`) is NOT implemented. A FLAT cluster -- one with no module children -- whose leaf standard cells exceed the level threshold is REFUSED rather than approximated. Upstream's own mpl suite never reaches that path; a large flat block does, so a refusal here is a real design shape and not a corner case.",
            "Correlation is measured against upstream's own 36-design regression suite at the pin above, per stage rather than on the final output: physical hierarchy and design report 34 of 34 byte-exact each; coarse shaping, boundary push and orientation byte-exact on every case that emits a trace; golden DEFs -- macro positions, temporary standard cells, halo blockages and clustering groups -- 34 of 34 exact. Those gates drive `cluster-dump`, not `place-macro`.",
            "Every score is true of ONE upstream commit, named in `openroad_pin`. The reference moves: a score quoted without its pin says nothing.",
            "`consumes` is the generic `odb` on purpose. mpl supports running BEFORE or AFTER pin placement and the two give DIFFERENT placements, so a fixed predecessor list would be the wrong shape of claim rather than merely the wrong list.",
        ],
    })
    .to_string()
}

/// `place-macro`: LibreLane `Classic` step 16, `Odb.ManualMacroPlacement`.
///
/// 🔑 **The reference here is LibreLane's `Odb.ManualMacroPlacement`, NOT OpenROAD's
/// `place_macro`.** They are different commands with different behaviour, and the flow we ship
/// runs the former: `Classic` has no RTL macro placer at all, and `ManualMacroPlacement` is an
/// `OdbpyStep` over OpenDB. OpenROAD's `mpl::placeMacro` additionally snaps to tracks, checks core
/// containment (MPL-34) and checks macro overlap (MPL-41); this step does none of those.
///
/// ⚠️ **Measured at pin `945a9f4`, on OpenROAD's `place_macro`, so the difference is on record:**
///   - a macro that is already LOCKED cannot be relocated at all -- OpenDB raises
///     `ODB-0359 Attempt to change the origin of LOCKED instance` before any check runs;
///   - a macro that is already PLACED trips `MPL-0041 ... Found overlap with other macros: <itself>`,
///     because `findOverlappedMacros` (`rtl_mp.cpp:175`) iterates every placed block instance and
///     **never skips the macro being placed**. Upstream's only shipped case cannot see this: its
///     input macro is UNPLACED.
/// ⟹ Macros must arrive UNPLACED. That is what a real flow hands this step, and it is why this
/// command does not attempt to relocate a locked instance.
///
/// 🔑 **Orientation is set BEFORE location**, matching `placeMacro`'s explicit ordering comment.
/// The final origin is the same either way, but the ordering is the reference's and is free to keep.
fn place_macro_main(args: &[String]) -> ExitCode {
    let mut path: Option<&str> = None;
    let mut out: Option<&str> = None;
    let mut in_dbu = false;
    let mut specs: Vec<(String, f32, f32, String)> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dbu" => in_dbu = true,
            "--out-odb" => {
                i += 1;
                match args.get(i) {
                    Some(v) => out = Some(v),
                    None => return usage_err("--out-odb needs a FILE"),
                }
            }
            "--macro" => {
                i += 1;
                let Some(spec) = args.get(i) else {
                    return usage_err("--macro needs NAME=X,Y[,ORIENT]");
                };
                let Some((name, rest)) = spec.split_once('=') else {
                    return usage_err(&format!("--macro wants NAME=X,Y[,ORIENT], got {spec:?}"));
                };
                let f: Vec<&str> = rest.split(',').collect();
                if f.len() < 2 || f.len() > 3 {
                    return usage_err(&format!("--macro wants X,Y[,ORIENT], got {rest:?}"));
                }
                // ⛔ **f32, NOT f64 — the reference's parameter type.** `MacroPlacer::placeMacro`
                // takes `const float& x_origin` (`mpl/include/mpl/rtl_mp.h:72`), so the coordinate
                // has already lost precision to ~7 significant digits BEFORE `micronsToDbu`
                // promotes it to double. Parsing as f64 keeps precision the reference threw away.
                // See `docs/openroad/cpp-to-rust-numeric-reference.md` §1: float beats every
                // integer type, and the narrowing is a second rounding people miss.
                let (Ok(x), Ok(y)) = (f[0].trim().parse::<f32>(), f[1].trim().parse::<f32>()) else {
                    return usage_err(&format!("--macro X,Y must be numbers, got {rest:?}"));
                };
                let orient = f.get(2).map(|s| s.trim().to_string()).unwrap_or_else(|| "R0".into());
                specs.push((name.to_string(), x, y, orient));
            }
            a if a.starts_with("--") => return usage_err(&format!("unknown option {a}")),
            a => path = Some(a),
        }
        i += 1;
    }

    let Some(path) = path else { return usage_err("place-macro needs <design.odb>") };

    // ⛔ **No macros named is VACUOUS, not success.** A pass word must never come from a run that
    // did nothing -- the convention every engine in this suite reserves status 3 for.
    if specs.is_empty() {
        eprintln!("vyges-mpl place-macro: no --macro given; nothing was placed.");
        return ExitCode::from(3);
    }

    let mut db = match vyges_opendb::Db::open(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("vyges-mpl place-macro: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };

    // ⚠️ **MICRONS by default, because that is the unit the config states.** Passing database units
    // where microns are expected turned an 8 um halo into 8 DBU once; the same trap is here.
    let dbu = db.dbu_per_micron();
    if dbu <= 0 {
        eprintln!("vyges-mpl place-macro: {path} has no DBU scale");
        return ExitCode::from(2);
    }
    let known: std::collections::HashSet<String> = db.inst_names().into_iter().collect();

    let mut placed = Vec::new();
    for (name, x, y, orient) in &specs {
        if !known.contains(name) {
            eprintln!("vyges-mpl place-macro: no instance named {name}");
            return ExitCode::from(1);
        }
        let (xd, yd) = if in_dbu {
            (*x as i32, *y as i32)
        } else {
            (microns_to_dbu(*x, dbu), microns_to_dbu(*y, dbu))
        };
        // Orientation first -- `placeMacro`'s ordering.
        if let Err(e) = db.set_inst_orient(name, orient) {
            eprintln!("vyges-mpl place-macro: {name}: cannot set orientation {orient}: {e}");
            return ExitCode::from(1);
        }
        if let Err(e) = db.set_inst_location(name, xd, yd) {
            eprintln!("vyges-mpl place-macro: {name}: cannot set location: {e}");
            eprintln!("  ⚠️ a macro that is already LOCKED cannot be relocated; this step expects\n\
                            macros to arrive UNPLACED, as a harden flow hands them.");
            return ExitCode::from(1);
        }
        // 🔑 **FIXED, not PLACED.** LibreLane's step leaves macros fixed so later placement cannot
        // move them; our own taped-out `user_project_wrapper.def` records every macro as `+ FIXED`.
        if let Err(e) = db.inst_set_placement_status(name, "FIRM") {
            eprintln!("vyges-mpl place-macro: {name}: cannot set placement status: {e}");
            return ExitCode::from(1);
        }
        // 🔑 **Upstream's own line, verbatim** — `MacroPlacer::placeMacro` closes with MPL-35
        // (`rtl_mp.cpp:161`), and it reports the BOUNDING BOX after placement, not the origin it
        // was given. This is `mpl`'s first structured event: the crate carried the `vyges-events`
        // dependency with no emission at all, so nothing it did reached the causal trail.
        let bx = db.inst_bbox(name).unwrap_or_default();
        if bx.len() == 4 {
            let um = |v: i32| v as f64 / dbu as f64;
            vyges_events::emit(
                &vyges_events::Event::new(
                    "vyges-mpl",
                    vyges_events::Severity::Info,
                    format!(
                        "MPL-0035 Macro {name} placed. Bounding box ({:.3}um, {:.3}um), \
                         ({:.3}um, {:.3}um). Orientation {orient}",
                        um(bx[0]), um(bx[1]), um(bx[2]), um(bx[3])
                    ),
                )
                .with_code("MPL-MACRO-PLACED")
                .with_objects(vec![format!("instance:{name}")]),
            );
        }
        placed.push(serde_json::json!({
            "instance": name, "x_dbu": xd, "y_dbu": yd, "orient": orient,
        }));
    }

    let dest = out.unwrap_or(path);
    if let Err(e) = db.write(dest) {
        eprintln!("vyges-mpl place-macro: cannot write {dest}: {e}");
        return ExitCode::from(2);
    }

    println!("{}", serde_json::json!({
        "tool": "vyges-mpl",
        "command": "place-macro",
        "status": "applied",
        "macros_placed": placed.len(),
        "macros": placed,
        "odb_written": dest,
    }));
    ExitCode::SUCCESS
}

fn usage_err(msg: &str) -> ExitCode {
    eprintln!("vyges-mpl place-macro: {msg}");
    ExitCode::from(2)
}


/// `dbBlock::micronsToDbu` (`dbBlock.cpp:2231`), with the reference's ARGUMENT type in front of it.
///
/// ```text
/// int dbBlock::micronsToDbu(const double microns) {
///   double dbu = microns * dbu_per_micron;
///   return static_cast<int>(std::round(dbu));      // round, NOT truncate
/// }
/// ```
///
/// ⛔ **The `f32` is the load-bearing part.** `placeMacro` takes `const float&`, so the value is
/// already a float when it reaches this; the promotion to `double` here cannot restore what the
/// float dropped. Doing the whole thing in `f64` is the tempting Rust and it is wrong.
///
/// ⚠️ **Measured range of the difference**: none at 3-decimal micron coordinates between 0.001 and
/// 5000 µm at 1000 or 2000 DBU — so this changes no realistic macro placement. It diverges at
/// arbitrary precision, e.g. 2168.228474947361 µm at 1000 DBU: f64 gives 2168228, the reference's
/// f32 path gives 2168229. Transcribed for the type, not for a bug.
fn microns_to_dbu(microns: f32, dbu_per_micron: i32) -> i32 {
    (microns as f64 * dbu_per_micron as f64).round() as i32
}

#[cfg(test)]
mod micron_conversion_tests {
    use super::microns_to_dbu;

    /// ⛔ The value that separates the two. Doing this in `f64` yields 2168228.
    #[test]
    fn the_coordinate_is_a_float_before_it_becomes_dbu() {
        let v: f32 = 2168.228474947361_f32;
        assert_eq!(microns_to_dbu(v, 1000), 2168229, "f64 would give 2168228");
    }

    /// ⚠️ And it changes nothing for a real placement — every edge-sensor macro is a whole micron.
    #[test]
    fn ordinary_coordinates_are_unaffected() {
        for (um, dbu, want) in [(1353.0_f32, 1000, 1353000), (100.0, 1000, 100000),
                                (3122.515, 1000, 3122515), (22.4, 2000, 44800)] {
            assert_eq!(microns_to_dbu(um, dbu), want, "{um} um at {dbu} dbu");
        }
    }

    /// `micronsToDbu` ROUNDS; it does not truncate.
    #[test]
    fn it_rounds_rather_than_truncating() {
        assert_eq!(microns_to_dbu(0.0006_f32, 1000), 1, "0.6 dbu rounds up");
        assert_eq!(microns_to_dbu(0.0004_f32, 1000), 0);
    }
}


fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--describe") => {
            println!("{}", describe());
            ExitCode::SUCCESS
        }
        Some("-h") | Some("--help") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("-V") | Some("--version") => {
            println!("vyges-mpl {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("place-macro") => place_macro_main(&args[1..]),
        _ => {
            // ⛔ **The ALGORITHM is written and correlated; this CLI PATH is not wired.** Those are
            // different claims and the old message conflated them, saying "the placement algorithm
            // is not written" long after it was. The pipeline runs under `cluster-dump`, which is
            // what all nine correlation gates drive, and its output is byte-exact against the
            // reference's golden DEFs.
            //
            // ⚠️ **`--describe` still advertises `args_template: ["run", "{design}"]`.** That is a
            // promise this binary does not yet keep, and it is stated here rather than left for a
            // caller to discover at runtime.
            eprintln!(
                "vyges-mpl: `run` is not wired yet.\n\
                 The placement pipeline IS implemented and correlated against OpenROAD -- it is\n\
                 driven by the `cluster-dump` binary, which every correlation gate uses. What is\n\
                 missing is this command-line entry point, not the algorithm.\n\
                 See `--describe` for the published contract and its limits."
            );
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod descriptor_tests {
    use super::describe;

    /// ⛔ **A score without a pin is not quotable**, and a correlation harness refuses to run
    /// against an engine that cannot say what it was built against. On 2026-08-29 `mpl` was the
    /// only engine of six whose binary could not answer — `ifp`, `ppl`, `pad`, `tap` and `pdn` all
    /// could — so `require_matching_pin` fell back to a warning and the guard went quiet.
    #[test]
    fn the_descriptor_reports_the_pin_this_binary_was_built_against() {
        let v: serde_json::Value =
            serde_json::from_str(&describe()).expect("the descriptor is valid JSON");
        let pin = v["openroad_pin"].as_str().expect("openroad_pin is a string");
        assert_eq!(pin, vyges_opendb::OPENROAD_PIN, "the pin must be inherited, never typed here");
        assert_eq!(pin.len(), 40, "a full commit SHA, not an abbreviation");
        assert!(
            pin.chars().all(|c| c.is_ascii_hexdigit()),
            "a commit SHA, not a tag or a branch name: {pin}"
        );
    }

    /// ⛔ **The contract must name a command that RUNS.** This descriptor advertised
    /// `args_template: ["run", "{design}"]` while `run` was unwired and exited 2 — so an MCP
    /// client built exactly that command and got a usage error. A contract's whole job is to say
    /// how to invoke the tool; naming a command that refuses is worse than naming none.
    #[test]
    fn the_advertised_command_is_one_this_binary_dispatches() {
        let v: serde_json::Value =
            serde_json::from_str(&describe()).expect("the descriptor is valid JSON");
        let cmd = v["invocation"]["args_template"][0].as_str().expect("a subcommand");
        assert_eq!(cmd, "place-macro",
                   "`{cmd}` is advertised but `place-macro` is the only wired command");
        // ⚠️ And `--help` must not promise it either: a usage line is a promise.
        assert!(!super::USAGE.contains("mpl run "),
                "USAGE advertises `run`, which this binary refuses");
        assert!(super::USAGE.contains("NOT WIRED"),
                "an unwired command must be named as unwired rather than omitted silently");
    }

    /// ⛔ **`maturity` is a closed enum of three** — `discovered`, `structured`,
    /// `workflow-validated`. An unrecognised word does NOT read as a modest claim: the consumer
    /// parses it to `None`, treats the engine as `discovered`, and suppresses the verdict to
    /// `unknown` however well-formed the assertion is.
    ///
    /// 🔑 The rung is about the shape of the EVIDENCE, not feature completeness — what is
    /// unbuilt goes in `provenance_limitations`, which is required.
    #[test]
    fn maturity_is_one_of_the_three_legal_rungs() {
        let v: serde_json::Value =
            serde_json::from_str(&describe()).expect("the descriptor is valid JSON");
        let m = v["maturity"].as_str().unwrap_or_default().to_string();
        assert!(["discovered", "structured", "workflow-validated"].contains(&m.as_str()),
                "`{m}` is not a legal maturity; an unrecognised one suppresses the verdict");
        // ⚠️ `workflow-validated` needs a pinned design in-repo that the SUITE runs end to end
        // and asserts against. `tests/goldens/` here is not loaded by any test, and the nine
        // correlation gates live outside this repository.
        assert_ne!(m, "workflow-validated",
                   "no in-repo test asserts the pipeline end to end against a pinned golden");
        assert!(!v["provenance_limitations"].as_array().expect("required").is_empty(),
                "provenance_limitations is required and states what the hash does not cover");
    }

    /// ⚠️ The `par` limit is part of the published contract, not a footnote — a caller must be able
    /// to TEST for it rather than discover it when a flat cluster is refused mid-run.
    #[test]
    fn the_descriptor_publishes_the_par_limit() {
        let v: serde_json::Value =
            serde_json::from_str(&describe()).expect("the descriptor is valid JSON");
        assert!(v["limits"]["par_partitioning"].is_string(), "par_partitioning must be published");
    }
}
