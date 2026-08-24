// SPDX-License-Identifier: Apache-2.0
//! The one place that touches the database on the way in.
//!
//! 🔑 **Everything else in this crate works on [`Design`]**, which is plain data. That is what
//! keeps the placement rules testable with values instead of with a fixture, and it is why this
//! file has no logic in it beyond translation — a rule that lives here is a rule that cannot be
//! tested without a database.

use crate::design::{Design, Instance, MasterKind, Module, Rect};
use std::collections::HashMap;
use vyges_opendb::Db;

/// Read the design's instances and module hierarchy.
///
/// ⚠️ **`is_ignorable_macro` is not set here.** It marks a fixed macro that does not overlap the
/// placement area, and the placement area depends on the global fence, which is a *command*
/// input rather than a database fact. [`mark_ignorable_macros`] applies it once that is known.
pub fn read_design(db: &Db) -> Result<Design, String> {
    let core = Rect {
        x_min: db.block_get_core_area_x_min() as i64,
        y_min: db.block_get_core_area_y_min() as i64,
        x_max: db.block_get_core_area_x_max() as i64,
        y_max: db.block_get_core_area_y_max() as i64,
    };
    let die = Rect {
        x_min: db.block_get_die_area_x_min() as i64,
        y_min: db.block_get_die_area_y_min() as i64,
        x_max: db.block_get_die_area_x_max() as i64,
        y_max: db.block_get_die_area_y_max() as i64,
    };

    let mut instances = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for name in db.block_get_insts() {
        let master = db.inst_get_master(&name);
        let bbox = db.inst_bbox(&name).map_err(|e| format!("bbox of {name}: {e}"))?;
        // ⚠️ Four values, in order; a short vector means the instance has no bbox and would
        // otherwise silently read as a zero-area cell at the origin.
        if bbox.len() < 4 {
            return Err(format!("instance {name} has no bounding box"));
        }
        by_name.insert(name.clone(), instances.len());
        instances.push(Instance {
            is_block: db.inst_is_block(&name),
            is_fixed: db.inst_is_fixed(&name),
            bbox: Rect {
                x_min: bbox[0] as i64,
                y_min: bbox[1] as i64,
                x_max: bbox[2] as i64,
                y_max: bbox[3] as i64,
            },
            master: MasterKind {
                is_pad: db.master_is_pad(&master),
                is_cover: db.master_is_cover(&master),
                is_end_cap: db.master_is_end_cap(&master),
            },
            is_ignorable_macro: false,
            name,
        });
    }

    // The module hierarchy, depth-first from the top.
    let top_name = db.block_get_top_module();
    let mut modules: Vec<Module> = Vec::new();
    let mut stack = vec![(top_name, None::<usize>)];
    while let Some((module_name, parent)) = stack.pop() {
        let idx = modules.len();
        let insts = db
            .module_get_insts(&module_name)
            .iter()
            .filter_map(|n| by_name.get(n).copied())
            .collect();
        modules.push(Module {
            hierarchical_name: db.module_get_hierarchical_name(&module_name),
            name: module_name.clone(),
            insts,
            children: Vec::new(),
        });
        if let Some(p) = parent {
            modules[p].children.push(idx);
        }
        // `module_get_children` yields the child module INSTANCES; each resolves to its master.
        for mod_inst in db.module_get_children(&module_name) {
            let master = db.modinst_get_master(&mod_inst);
            if !master.is_empty() {
                stack.push((master, Some(idx)));
            }
        }
    }

    Ok(Design { instances, modules, top: 0, core_area: core, die_area: die })
}

/// Mark the fixed macros that fall outside the placement area, which are dropped from
/// clustering entirely.
///
/// Returns their names, because upstream reports each one.
pub fn mark_ignorable_macros(design: &mut Design, placement_area: &Rect) -> Vec<String> {
    let mut ignored = Vec::new();
    for inst in &mut design.instances {
        if inst.is_block && inst.is_fixed && !inst.bbox.overlaps(placement_area) {
            inst.is_ignorable_macro = true;
            ignored.push(inst.name.clone());
        }
    }
    ignored
}

/// Read the block's ports as the clustering stage needs them.
///
/// ⚠️ **`is_fixed` is the FIRST PIN's placement status, not the terminal's** — upstream reads
/// `getFirstPinPlacementStatus`, and a terminal can carry pins whose status differs from it.
pub fn read_pins(db: &Db) -> Vec<crate::ioclusters::Pin> {
    db.block_get_b_terms()
        .into_iter()
        .map(|name| {
            let status = db.bterm_get_first_pin_placement_status(&name);
            crate::ioclusters::Pin {
                // OpenDB spells a fixed status several ways; all of them mean "do not move".
                is_fixed: matches!(status.as_str(), "FIRM" | "LOCKED" | "FIXED"),
                bbox: Rect {
                    x_min: db.bterm_get_b_box_x_min(&name) as i64,
                    y_min: db.bterm_get_b_box_y_min(&name) as i64,
                    x_max: db.bterm_get_b_box_x_max(&name) as i64,
                    y_max: db.bterm_get_b_box_y_max(&name) as i64,
                },
                constraint: db.bterm_constraint_region(&name).ok().flatten().map(|(a, b, c, d)| {
                    Rect { x_min: a as i64, y_min: b as i64, x_max: c as i64, y_max: d as i64 }
                }),
                name,
            }
        })
        .collect()
}
