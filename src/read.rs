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
                is_pad_without_signal: matches!(
                    db.master_get_type(&master).unwrap_or_default().as_str(),
                    "PAD_POWER" | "PAD_SPACER"
                ),
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

/// Read every net, reduced to the terminals the clustering cares about.
///
/// 🔑 **A terminal is identified by index, not by name**, from here on: instances by their position
/// in [`Design::instances`], block ports by their position in the pin list from [`read_pins`]. A
/// name that does not resolve is dropped rather than guessed — an unresolvable terminal is not
/// connectivity, and inventing one would tie unrelated clusters together.
///
/// ⚠️ The instance pin arrives as `inst/mterm`; the split is on the LAST separator, because an
/// instance name in a hierarchical design contains them and a master terminal name does not.
pub fn read_nets(
    db: &Db,
    design: &Design,
    pins: &[crate::ioclusters::Pin],
) -> Vec<crate::netlist::DbNet> {
    let inst_index: HashMap<&str, usize> =
        design.instances.iter().enumerate().map(|(i, x)| (x.name.as_str(), i)).collect();
    let pin_index: HashMap<&str, usize> =
        pins.iter().enumerate().map(|(i, p)| (p.name.as_str(), i)).collect();

    let mut out = Vec::new();
    for name in db.net_names() {
        let sig = db.net_sigtype(&name);
        let mut iterms = Vec::new();
        for term in db.net_iterms(&name) {
            let Some((inst, pin)) = term.rsplit_once('/') else { continue };
            let Some(&i) = inst_index.get(inst) else { continue };
            iterms.push(crate::netlist::InstTerm {
                inst: i,
                is_output: db.iterm_get_io_type(inst, pin) == "OUTPUT",
            });
        }
        let mut bterms = Vec::new();
        for term in db.net_bterms(&name) {
            let Some(&b) = pin_index.get(term.as_str()) else { continue };
            bterms.push(crate::netlist::PortTerm {
                bterm: b,
                is_input: db.bterm_direction(&term) == "INPUT",
            });
        }
        out.push(crate::netlist::DbNet {
            name,
            // A supply net carries no placement information; `isValidNet` drops it.
            is_supply: sig == "POWER" || sig == "GROUND",
            iterms,
            bterms,
        });
    }
    out
}

/// Everything halo resolution needs from the database about one macro.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroGeometry {
    pub master_width: i64,
    pub master_height: i64,
    /// ⚠️ SIGNAL pins only — power and ground never widen a halo.
    pub pins: Vec<crate::halo::PinBox>,
    pub inst_halo: Option<(crate::options::Halo, bool)>,
    pub orient: crate::halo::Orient,
}

/// Layer number → `(spacing, direction)`, read once.
fn layer_table(db: &Db) -> HashMap<i64, (i64, crate::halo::LayerDir)> {
    let mut out = HashMap::new();
    for (name, dir) in db.layers_with_direction().unwrap_or_default() {
        let dir = if dir == "VERTICAL" {
            crate::halo::LayerDir::Vertical
        } else {
            crate::halo::LayerDir::Horizontal
        };
        out.insert(db.layer_get_number(&name) as i64, (db.layer_get_spacing(&name) as i64, dir));
    }
    out
}

/// Upstream `getMinimumSpacing`: the widest layer spacing any macro's geometry sits on.
///
/// ⚠️ **Obstructions AND every terminal's pins**, with no signal-type filter — unlike the halo
/// rule right next to it, which is signal-only. Taking one and not the other silently narrows
/// every pin-aware halo.
pub fn minimum_spacing(db: &Db, design: &Design) -> i64 {
    let layers = layer_table(db);
    let spacing = |n: i64| layers.get(&n).map_or(0, |(s, _)| *s);
    let mut out = 0;
    for inst in design.instances.iter().filter(|i| i.is_block) {
        let master = db.inst_get_master(&inst.name);
        for (layer, ..) in db.master_obstruction_boxes(&master).unwrap_or_default() {
            out = out.max(spacing(layer));
        }
        for term in db.master_get_m_terms(&master) {
            for (layer, ..) in db.mterm_pin_boxes(&master, &term).unwrap_or_default() {
                out = out.max(spacing(layer));
            }
        }
    }
    out
}

/// Read each macro's master dimensions, signal pin shapes, instance halo and orientation.
///
/// 🔑 **The layer direction of a pin box is the direction of its MPin's FIRST box**, not of the
/// box itself — upstream reads `mpin->getGeometry().begin()` even while examining a later box of
/// the same pin. That is why the shapes have to arrive grouped by MPin.
pub fn read_macro_geometry(db: &Db, design: &Design) -> Vec<Option<MacroGeometry>> {
    let layers = layer_table(db);
    let mut out = vec![None; design.instances.len()];
    for (i, inst) in design.instances.iter().enumerate() {
        if !inst.is_block {
            continue;
        }
        let master = db.inst_get_master(&inst.name);
        let mut pins = Vec::new();
        for term in db.master_get_m_terms(&master) {
            if db.mterm_get_sig_type(&master, &term) != "SIGNAL" {
                continue;
            }
            for p in 0..db.num_mterm_get_m_pins(&master, &term) {
                let boxes = db.mpin_boxes(&master, &term, p).unwrap_or_default();
                let Some(&(first, ..)) = boxes.first() else { continue };
                let layer_dir =
                    layers.get(&first).map_or(crate::halo::LayerDir::Horizontal, |(_, d)| *d);
                for (_, x0, y0, x1, y1) in boxes {
                    pins.push(crate::halo::PinBox {
                        x_min: x0 as i64,
                        y_min: y0 as i64,
                        x_max: x1 as i64,
                        y_max: y1 as i64,
                        layer_dir,
                    });
                }
            }
        }
        out[i] = Some(MacroGeometry {
            master_width: db.master_get_width(&master) as i64,
            master_height: db.master_get_height(&master) as i64,
            pins,
            inst_halo: db.inst_halo(&inst.name).unwrap_or(None).map(|(l, b, r, t, soft)| {
                (crate::options::Halo { left: l, bottom: b, right: r, top: t }, soft)
            }),
            orient: match db.inst_get_orient(&inst.name).as_str() {
                "R180" => crate::halo::Orient::R180,
                "MX" => crate::halo::Orient::Mx,
                "MY" => crate::halo::Orient::My,
                "R0" => crate::halo::Orient::R0,
                _ => crate::halo::Orient::Other,
            },
        });
    }
    out
}

/// Placement blockages, as rectangles.
///
/// ⚠️ Upstream takes **every** blockage the block holds — soft and hard alike — and unions them.
/// Filtering by softness here would understate the occupied area and pass a design that upstream
/// refuses.
pub fn read_blockages(db: &Db) -> Vec<Rect> {
    db.blockage_boxes()
        .unwrap_or_default()
        .into_iter()
        .map(|(x0, y0, x1, y1)| Rect {
            x_min: x0 as i64,
            y_min: y0 as i64,
            x_max: x1 as i64,
            y_max: y1 as i64,
        })
        .collect()
}
