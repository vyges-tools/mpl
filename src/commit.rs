// SPDX-License-Identifier: Apache-2.0
//! `run`'s write-back: the commit stage's rows applied to the database as upstream's
//! `commitMacroPlacementToDb` and `generateTemporaryStdCellsPlacement` leave it.
//!
//! 🔑 **The rows are the commit gate's own.** [`crate::driver`] under `--commit` prints exactly what
//! the commit gate scores against the golden DEFs (`MACRO`, `BLOCKAGE`, `GROUP`, `CELL`), so `run`
//! writes what was scored rather than a second derivation of it.
//!
//! What upstream writes, per macro in `inst_to_hard` (`commitMacroPlacementToDb`):
//! - a movable one is snapped and set `LOCKED` (the snap is already in the row's position);
//! - unless its halo is soft, a SOFT placement blockage over its haloed box, ASSOCIATED with the
//!   instance (`dbBlockage::create(block, …, inst)` then `setSoft()`) — a fixed macro casts one too.
//!
//! and per leaf standard cell (`setTemporaryStdCellLocation`): a location and `PLACED`.

use std::collections::HashMap;

/// One line of the commit dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A placed macro: the bounding box's lower-left corner (as the DEF records it) and its DEF
    /// orientation.
    Macro { name: String, x: i32, y: i32, orient: String },
    /// A soft placement blockage over `name`'s haloed box.
    Blockage { name: String, x0: i32, y0: i32, x1: i32, y1: i32 },
    /// A `-keep_clustering_data` group: name and members.
    Group { name: String, members: Vec<String> },
    /// A standard cell's temporary place.
    Cell { name: String, x: i32, y: i32 },
}

/// The commit dump, line by line. Any other line is an error: the dump is the contract.
pub fn parse(dump: &str) -> Result<Vec<Row>, String> {
    let num = |s: &str, line: &str| s.parse::<i32>().map_err(|_| format!("not a number {s:?} in {line:?}"));
    let mut rows = Vec::new();
    for line in dump.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split_whitespace().collect();
        rows.push(match f.as_slice() {
            ["MACRO", name, x, y, orient] => {
                Row::Macro { name: name.to_string(), x: num(x, line)?, y: num(y, line)?, orient: orient.to_string() }
            }
            ["BLOCKAGE", name, x0, y0, x1, y1] => Row::Blockage {
                name: name.to_string(),
                x0: num(x0, line)?,
                y0: num(y0, line)?,
                x1: num(x1, line)?,
                y1: num(y1, line)?,
            },
            ["GROUP", name, members @ ..] => {
                Row::Group { name: name.to_string(), members: members.iter().map(|m| m.to_string()).collect() }
            }
            ["CELL", name, x, y, _] => Row::Cell { name: name.to_string(), x: num(x, line)?, y: num(y, line)? },
            _ => return Err(format!("not a commit row: {line:?}")),
        });
    }
    Ok(rows)
}

/// The database's name for a DEF orientation — the inverse of
/// [`crate::placement::def_orientation_name`].
pub fn odb_orient(def: &str) -> Option<&'static str> {
    Some(match def {
        "N" => "R0",
        "W" => "R90",
        "S" => "R180",
        "E" => "R270",
        "FN" => "MY",
        "FW" => "MXR90",
        "FS" => "MX",
        "FE" => "MYR90",
        _ => return None,
    })
}

/// The blockages in the order upstream creates them: over `inst_to_hard`, a `PtrMap` — the
/// database's map that compares its keys BY OBJECT ID, not by address — so instance id order.
/// ⚠️ **No DEF can see this order**: the writer sorts the `BLOCKAGES` section by rectangle. It is
/// the database's blockage table order, which a later stage reading the `.odb` iterates — so it
/// is pinned here, by unit test, and nowhere else.
pub fn blockage_order<'r>(rows: &'r [Row], id_of: &dyn Fn(&str) -> Option<u32>) -> Result<Vec<&'r Row>, String> {
    let mut keyed = Vec::new();
    for r in rows {
        if let Row::Blockage { name, .. } = r {
            keyed.push((id_of(name).ok_or_else(|| format!("no instance named {name}"))?, r));
        }
    }
    keyed.sort_by_key(|(id, _)| *id);
    Ok(keyed.into_iter().map(|(_, r)| r).collect())
}

/// What [`apply`] wrote.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Applied {
    pub macros: usize,
    pub blockages: usize,
    pub cells: usize,
}

/// The rows written into `db`. `GROUP` rows are refused: upstream nests its groups (a child's
/// under its parent's), which the flat rows do not carry.
pub fn apply(db: &mut vyges_opendb::Db, rows: &[Row]) -> Result<Applied, String> {
    if let Some(Row::Group { name, .. }) = rows.iter().find(|r| matches!(r, Row::Group { .. })) {
        return Err(format!("group {name}: clustering groups are not written (upstream nests them)"));
    }
    let mut done = Applied::default();
    for r in rows {
        if let Row::Macro { name, x, y, orient } = r {
            let o = odb_orient(orient).ok_or_else(|| format!("{name}: no orientation {orient}"))?;
            // Orientation first, then the box's corner: `flipRealMacro`'s order.
            db.set_inst_orient(name, o).map_err(|e| format!("{name}: orientation {o}: {e}"))?;
            db.set_inst_location(name, *x, *y).map_err(|e| format!("{name}: location: {e}"))?;
            db.set_inst_placement_status(name, "LOCKED").map_err(|e| format!("{name}: status: {e}"))?;
            done.macros += 1;
        }
    }
    let ids: HashMap<String, u32> = rows
        .iter()
        .filter_map(|r| match r {
            Row::Blockage { name, .. } => db.inst_id(name).ok().map(|id| (name.clone(), id)),
            _ => None,
        })
        .collect();
    for r in blockage_order(rows, &|n| ids.get(n).copied())? {
        if let Row::Blockage { name, x0, y0, x1, y1 } = r {
            db.create_blockage(*x0, *y0, *x1, *y1, name, true).map_err(|e| format!("{name}: blockage: {e}"))?;
            done.blockages += 1;
        }
    }
    for r in rows {
        if let Row::Cell { name, x, y } = r {
            db.set_inst_location(name, *x, *y).map_err(|e| format!("{name}: location: {e}"))?;
            db.set_inst_placement_status(name, "PLACED").map_err(|e| format!("{name}: status: {e}"))?;
            done.cells += 1;
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_kind_parses_and_anything_else_is_refused() {
        let rows = parse("BLOCKAGE m1 0 0 10 20\nCELL c1 5 6 N\nGROUP g a b\nMACRO m1 100 200 FS\n").unwrap();
        assert_eq!(rows[0], Row::Blockage { name: "m1".into(), x0: 0, y0: 0, x1: 10, y1: 20 });
        assert_eq!(rows[1], Row::Cell { name: "c1".into(), x: 5, y: 6 });
        assert_eq!(rows[2], Row::Group { name: "g".into(), members: vec!["a".into(), "b".into()] });
        assert_eq!(rows[3], Row::Macro { name: "m1".into(), x: 100, y: 200, orient: "FS".into() });
        assert!(parse("cluster 3 placed").is_err(), "a trace line is not a commit row");
        assert!(parse("MACRO m1 1.5 2 N").is_err(), "positions are database units");
    }

    /// The DEF names back to the database's: `FS` is `MX`, `FN` is `MY`.
    #[test]
    fn each_def_orientation_maps_back_to_its_own() {
        use crate::placement::{def_orientation_name, DbOrient};
        for (o, odb) in [
            (DbOrient::R0, "R0"), (DbOrient::R90, "R90"), (DbOrient::R180, "R180"), (DbOrient::R270, "R270"),
            (DbOrient::MY, "MY"), (DbOrient::MYR90, "MYR90"), (DbOrient::MX, "MX"), (DbOrient::MXR90, "MXR90"),
        ] {
            assert_eq!(odb_orient(def_orientation_name(o)), Some(odb), "{o:?}");
        }
        assert_eq!(odb_orient("R0"), None, "only DEF names are rows' names");
    }

    /// Upstream creates them over `inst_to_hard`, a map keyed by instance and ordered by object id —
    /// not in the dump's sorted-by-name order.
    #[test]
    fn blockages_are_created_in_instance_id_order() {
        let rows = parse("BLOCKAGE a 0 0 1 1\nBLOCKAGE b 0 0 1 1\nBLOCKAGE c 0 0 1 1\nMACRO a 0 0 N").unwrap();
        let id = |n: &str| Some(match n { "a" => 7, "b" => 2, "c" => 5, _ => return None });
        let names: Vec<&str> = blockage_order(&rows, &id)
            .unwrap()
            .into_iter()
            .map(|r| match r { Row::Blockage { name, .. } => name.as_str(), _ => unreachable!() })
            .collect();
        assert_eq!(names, ["b", "c", "a"]);
        assert!(blockage_order(&rows, &|_| None).is_err(), "an unknown instance is an error");
    }
}
