// SPDX-License-Identifier: Apache-2.0
//! `vyges-mpl` CLI — hierarchical macro placement over a `.odb`.
//!
//! Exit status is the verdict: 0 applied, 1 refused, 2 usage/read/write error, 3 vacuous.

use std::process::ExitCode;
use vyges_mpl::pipeline::ORDER;

const USAGE: &str = "\
vyges physical mpl — hierarchical macro placement

USAGE:
  vyges physical mpl run <design.odb> [options]
  vyges physical mpl --describe
  vyges physical mpl --help

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
        "tool": "vyges-mpl",
        "version": env!("CARGO_PKG_VERSION"),
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
        "args_template": ["run", "{design}"],
        "inputs": [{"name": "design", "type": "odb"}],
        "artifacts": [{"name": "design", "type": "odb", "written_in_place": true}],
        "reports": ["status", "macros_placed", "blockages_created", "refusal"],
        "assertion": {"field": "status", "pass_when": {"eq": "applied"}},
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
        "stages": ORDER.iter().map(|s| s.upstream_name()).collect::<Vec<_>>(),
        "limits": {
            "par_partitioning": "not implemented; a flat cluster over the level threshold is refused",
        },
    })
    .to_string()
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

    /// ⚠️ The `par` limit is part of the published contract, not a footnote — a caller must be able
    /// to TEST for it rather than discover it when a flat cluster is refused mid-run.
    #[test]
    fn the_descriptor_publishes_the_par_limit() {
        let v: serde_json::Value =
            serde_json::from_str(&describe()).expect("the descriptor is valid JSON");
        assert!(v["limits"]["par_partitioning"].is_string(), "par_partitioning must be published");
    }
}
