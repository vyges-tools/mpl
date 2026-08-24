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
        "args_template": ["run", "{design}"],
        "inputs": [{"name": "design", "type": "odb"}],
        "artifacts": [{"name": "design", "type": "odb", "written_in_place": true}],
        "reports": ["status", "macros_placed", "blockages_created", "refusal"],
        "assertion": {"field": "status", "pass_when": {"eq": "applied"}},
        "consumes": ["ifp", "pad", "ppl"],
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
            eprintln!("vyges-mpl: not implemented yet -- the placement algorithm is not written");
            ExitCode::from(2)
        }
    }
}
