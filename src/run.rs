// SPDX-License-Identifier: Apache-2.0
//! `vyges-mpl run` — the automatic placement, written to the database.
//!
//! ℹ️ In the library so a binary that links a flat-cluster partitioner runs the SAME command.

use std::process::ExitCode;

/// ⛔ Before any database exists: the database library then logs to the events trail (stderr)
/// only, and stdout carries nothing but the report a caller parses.
pub fn init_logging() {
    vyges_opendb::init_events_logging();
}

/// A diagnostic as a structured event (`MPL-ERROR`, `vyges-events/1.0`) instead of a bare stderr
/// line; text after a blank line (the usage) stays plain help.
pub fn diag_line(msg: String) {
    let (head, help) = match msg.split_once("\n\n") {
        Some((h, rest)) => (h.to_string(), Some(rest.to_string())),
        None => (msg, None),
    };
    let text = head.strip_prefix("vyges-mpl place-macro: ").or_else(|| head.strip_prefix("vyges-mpl: ")).unwrap_or(&head).to_string();
    vyges_events::emit(&vyges_events::Event::new("vyges-mpl", vyges_events::Severity::Error, text).with_code("MPL-ERROR"));
    if let Some(h) = help {
        eprintln!("{h}");
    }
}

macro_rules! diag {
    ($($t:tt)*) => { diag_line(format!($($t)*)) };
}

/// The driver's options that `run` passes through, each with a value: command state a database
/// cannot carry.
pub const RUN_VALUED: [&str; 11] = [
    "--partition-calls", "--min-channel", "--macro-guide", "--boundary-weight", "--notch-weight", "--guidance-weight",
    "--target-util", "--max-num-inst", "--min-num-inst", "--max-num-macro", "--min-num-macro",
];

/// Whether `run` takes `flag` — what the descriptor may advertise.
pub fn run_main_accepts(flag: &str) -> bool {
    matches!(flag, "--out-odb" | "--out-def" | "--pin-aware-channels") || RUN_VALUED.contains(&flag)
}

/// `run`: the automatic placement — the driver's commit stage, written to the database.
///
/// 🔑 **One derivation.** The driver under `--shaping --commit` prints the rows the commit gate
/// scores against the golden DEFs; `run` applies exactly those rows ([`crate::commit`]), so
/// what is written is what was scored.
///
/// ⛔ **A refusal writes nothing.** The driver runs to completion before the database is opened
/// for writing, so a refused or vacuous run leaves the design as it was.
pub fn run_main(args: &[String], partitioner: Option<&mut dyn crate::flatpart::FlatPartitioner>) -> ExitCode {
    let mut out_odb: Option<String> = None;
    let mut out_def: Option<String> = None;
    let mut path: Option<String> = None;
    let mut pass: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--out-odb" | "--out-def" => {
                i += 1;
                let Some(v) = args.get(i) else { return run_usage_err(&format!("{a} needs a FILE")) };
                if a == "--out-odb" { out_odb = Some(v.clone()) } else { out_def = Some(v.clone()) }
            }
            "--pin-aware-channels" => pass.push(a.to_string()),
            // the report is always JSON; accepted because `vyges mcp` appends it (emits_json)
            "--json" => {}
            "--keep-clustering-data" => {
                diag!("vyges-mpl run: --keep-clustering-data is not supported: clustering groups are not written");
                return ExitCode::from(1);
            }
            _ if RUN_VALUED.contains(&a) => {
                i += 1;
                let Some(v) = args.get(i) else { return run_usage_err(&format!("{a} needs a value")) };
                pass.extend([a.to_string(), v.clone()]);
            }
            _ if a.starts_with("--") => return run_usage_err(&format!("unknown option {a}")),
            _ if path.is_some() => return run_usage_err(&format!("one design only, got {a:?} too")),
            _ => path = Some(a.to_string()),
        }
        i += 1;
    }
    let Some(path) = path else { return run_usage_err("run needs <design.odb>") };

    let mut driver_args = vec!["--shaping".to_string(), "--commit".to_string()];
    driver_args.extend(pass);
    driver_args.push(path.clone());
    let mut dump = Vec::new();
    if let Err(e) = crate::driver::run_with(&driver_args, &mut dump, partitioner) {
        diag!("vyges-mpl run: {}", e.message);
        // The driver's 4 is a shaping refusal; to a caller it is a refusal like any other.
        let (code, status) = match e.code {
            1 | 4 => (1, "refused"),
            3 => (3, "vacuous"),
            c => (c, "error"),
        };
        // `refusal` names a refusal only; a vacuous run or an error gives its `reason`.
        let key = if status == "refused" { "refusal" } else { "reason" };
        println!("{}", serde_json::json!({
            "tool": "vyges-mpl", "command": "run", "status": status, key: e.message,
        }));
        return ExitCode::from(code);
    }
    let rows = match std::str::from_utf8(&dump).map_err(|e| e.to_string()).and_then(crate::commit::parse) {
        Ok(r) => r,
        Err(e) => {
            diag!("vyges-mpl run: the commit stage's output is unreadable: {e}");
            return ExitCode::from(2);
        }
    };

    let mut db = match vyges_opendb::Db::open(&path) {
        Ok(d) => d,
        Err(e) => {
            diag!("vyges-mpl run: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let done = match crate::commit::apply(&mut db, &rows) {
        Ok(d) => d,
        Err(e) => {
            diag!("vyges-mpl run: {e}");
            return ExitCode::from(1);
        }
    };
    // ⛔ Zero macros written is not `applied`, whatever else was.
    let status = crate::status::settle_status(done.macros, None);
    if status != crate::status::Status::Applied {
        diag!("vyges-mpl run: no macro was placed; nothing was written");
        println!("{}", serde_json::json!({"tool": "vyges-mpl", "command": "run", "status": status.as_str()}));
        return ExitCode::from(status.exit_code());
    }
    let dest = out_odb.unwrap_or_else(|| path.clone());
    if let Err(e) = db.write(&dest) {
        diag!("vyges-mpl run: cannot write {dest}: {e}");
        return ExitCode::from(2);
    }
    if let Some(def) = &out_def {
        if let Err(e) = db.write_def(def) {
            diag!("vyges-mpl run: cannot write {def}: {e}");
            return ExitCode::from(2);
        }
    }
    println!("{}", serde_json::json!({
        "tool": "vyges-mpl",
        "command": "run",
        "status": status.as_str(),
        "macros_placed": done.macros,
        "blockages_created": done.blockages,
        "cells_placed": done.cells,
        "odb_written": dest,
        "def_written": out_def,
    }));
    ExitCode::SUCCESS
}

fn run_usage_err(msg: &str) -> ExitCode {
    diag!("vyges-mpl run: {msg}");
    ExitCode::from(2)
}
