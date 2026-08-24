// SPDX-License-Identifier: Apache-2.0
//! The command contract — every key, flag, default and error from `src/mpl.tcl`.
//!
//! ⚠️ **This module exists because option spellings have hidden whole case sets twice.**
//! `tap` lost four cases to `-halo_width_x`; `ppl` lost a day to an untranslated
//! `set_slots_per_section`. Both failed the same way: **an option that fails to arrive does not
//! error, it silently does nothing**, and a no-op that reports success stays hidden.
//!
//! 🔑 It also exists because a first draft written from the command *names* invented three options
//! and gave `set_macro_halo` an `-halo_x`/`-halo_y` pair it does not have. Everything here is
//! transcribed from upstream's `mpl.tcl`, not inferred.

use std::fmt;

/// An upstream diagnostic. The number is `MPL-<n>` and matters: the cases match on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MplError {
    pub code: u16,
    pub message: String,
}

impl MplError {
    fn new(code: u16, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl fmt::Display for MplError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[ERROR MPL-{:04}] {}", self.code, self.message)
    }
}

/// A deprecation warning upstream emits but does not fail on. ⚠️ Collected rather than
/// discarded: a case whose golden contains `MPL-0074` fails if we stay silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MplWarning {
    pub code: u16,
    pub message: String,
}

/// A halo, in upstream's order.
///
/// 🔑 Upstream rule (`mpl::parse_halo`): a halo is a list of **2 or 4** values ordered
/// **left bottom right top**. Two values mean `right = left` and `top = bottom` — NOT
/// `(x, y)` applied symmetrically, though it amounts to the same thing; keeping the names
/// upstream's way is what stops the next reader from inventing an `-halo_x`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Halo {
    pub left: i64,
    pub bottom: i64,
    pub right: i64,
    pub top: i64,
}

impl Halo {
    /// Parse a halo list. `MPL-72` on a bad length, `MPL-73` on a negative value.
    pub fn parse(values: &[i64]) -> Result<Halo, MplError> {
        let (left, bottom, right, top) = match values.len() {
            2 => (values[0], values[1], values[0], values[1]),
            4 => (values[0], values[1], values[2], values[3]),
            _ => return Err(MplError::new(72, "Halo must have 2 or 4 values.")),
        };
        for v in [left, bottom, right, top] {
            if v < 0 {
                return Err(MplError::new(73, "Halo values must be non-negative."));
            }
        }
        Ok(Halo { left, bottom, right, top })
    }
}

/// `rtl_macro_placer`'s parameters, with upstream's defaults.
///
/// 🔑 Every default here is from `mpl.tcl`, and `0` for the four cluster-size keys means
/// **auto** (`setBaseThresholds` derives them), not "zero".
#[derive(Debug, Clone, PartialEq)]
pub struct PlacerOptions {
    pub max_num_macro: i64,
    pub min_num_macro: i64,
    pub max_num_inst: i64,
    pub min_num_inst: i64,
    pub tolerance: f64,
    pub max_num_level: i64,
    pub coarsening_ratio: f64,
    pub large_net_threshold: i64,
    pub fence_lx: f64,
    pub fence_ly: f64,
    pub fence_ux: f64,
    pub fence_uy: f64,
    pub area_weight: f64,
    pub outline_weight: f64,
    pub wirelength_weight: f64,
    pub guidance_weight: f64,
    pub fence_weight: f64,
    pub boundary_weight: f64,
    pub notch_weight: f64,
    pub soft_blockage_weight: f64,
    pub target_util: f64,
    pub min_ar: f64,
    pub report_directory: String,
    pub write_macro_placement: Option<String>,
    pub keep_clustering_data: bool,
    pub use_full_halo: bool,
    /// Set by the deprecated `-halo_width`/`-halo_height`, which call `set_base_halo`
    /// during argument parsing — i.e. before the engine runs.
    pub base_halo_from_flags: Option<Halo>,
}

impl Default for PlacerOptions {
    fn default() -> Self {
        Self {
            max_num_macro: 0,
            min_num_macro: 0,
            max_num_inst: 0,
            min_num_inst: 0,
            tolerance: 0.1,
            max_num_level: 2,
            coarsening_ratio: 10.0,
            large_net_threshold: 50,
            fence_lx: 0.0,
            fence_ly: 0.0,
            fence_ux: 0.0,
            fence_uy: 0.0,
            area_weight: 0.1,
            outline_weight: 100.0,
            wirelength_weight: 100.0,
            guidance_weight: 10.0,
            fence_weight: 10.0,
            boundary_weight: 50.0,
            notch_weight: 50.0,
            soft_blockage_weight: 10.0,
            target_util: 0.25,
            min_ar: 0.33,
            report_directory: "hier_rtlmp".to_string(),
            write_macro_placement: None,
            keep_clustering_data: false,
            use_full_halo: false,
            base_halo_from_flags: None,
        }
    }
}

/// A guidance region. `MPL-31/32/33` guard its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x1: i64,
    pub y1: i64,
    pub x2: i64,
    pub y2: i64,
}

impl Region {
    pub fn parse(values: &[i64]) -> Result<Region, MplError> {
        if values.len() != 4 {
            return Err(MplError::new(31, "-region must be a list of 4 values."));
        }
        let (x1, y1, x2, y2) = (values[0], values[1], values[2], values[3]);
        if x1 > x2 {
            return Err(MplError::new(32, "Invalid region: x1 > x2."));
        }
        if y1 > y2 {
            return Err(MplError::new(33, "Invalid region: y1 > y2."));
        }
        Ok(Region { x1, y1, x2, y2 })
    }
}

/// The result of translating `rtl_macro_placer`'s argument list.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub options: PlacerOptions,
    pub warnings: Vec<MplWarning>,
}

/// Translate `rtl_macro_placer`'s keys and flags.
///
/// ⛔ **An unknown key is an error, never ignored.** That is the whole point of this module:
/// upstream's `sta::parse_key_args` rejects what it does not know, and an engine that shrugs at
/// an unrecognised option silently runs a different command than the case asked for.
pub fn parse_placer_args(args: &[&str]) -> Result<Parsed, MplError> {
    let mut o = PlacerOptions::default();
    let mut warnings = Vec::new();
    let mut halo_width: Option<i64> = None;
    let mut halo_height: Option<i64> = None;
    let mut saw_macro_blockage_weight = false;
    let mut saw_soft_blockage_weight = false;

    let mut i = 0;
    while i < args.len() {
        let key = args[i];
        // Flags take no value.
        match key {
            "-keep_clustering_data" => {
                o.keep_clustering_data = true;
                i += 1;
                continue;
            }
            "-use_full_halo" => {
                o.use_full_halo = true;
                i += 1;
                continue;
            }
            _ => {}
        }

        let value = args.get(i + 1).copied().ok_or_else(|| {
            MplError::new(0, format!("{key} requires a value."))
        })?;
        i += 2;

        // A key that parses but whose VALUE does not is an error, not a fallback to the default.
        let num = |v: &str| -> Result<f64, MplError> {
            v.parse::<f64>()
                .map_err(|_| MplError::new(0, format!("{key} expects a number, got {v}.")))
        };
        let int = |v: &str| -> Result<i64, MplError> {
            v.parse::<i64>()
                .map_err(|_| MplError::new(0, format!("{key} expects an integer, got {v}.")))
        };

        match key {
            "-max_num_macro" => o.max_num_macro = int(value)?,
            "-min_num_macro" => o.min_num_macro = int(value)?,
            "-max_num_inst" => o.max_num_inst = int(value)?,
            "-min_num_inst" => o.min_num_inst = int(value)?,
            "-tolerance" => o.tolerance = num(value)?,
            "-max_num_level" => o.max_num_level = int(value)?,
            "-coarsening_ratio" => o.coarsening_ratio = num(value)?,
            "-large_net_threshold" => o.large_net_threshold = int(value)?,
            "-fence_lx" => o.fence_lx = num(value)?,
            "-fence_ly" => o.fence_ly = num(value)?,
            "-fence_ux" => o.fence_ux = num(value)?,
            "-fence_uy" => o.fence_uy = num(value)?,
            "-area_weight" => o.area_weight = num(value)?,
            "-outline_weight" => o.outline_weight = num(value)?,
            "-wirelength_weight" => o.wirelength_weight = num(value)?,
            "-guidance_weight" => o.guidance_weight = num(value)?,
            "-fence_weight" => o.fence_weight = num(value)?,
            "-boundary_weight" => o.boundary_weight = num(value)?,
            "-notch_weight" => o.notch_weight = num(value)?,
            "-target_util" => o.target_util = num(value)?,
            "-min_ar" => o.min_ar = num(value)?,
            "-report_directory" => o.report_directory = value.to_string(),
            "-write_macro_placement" => o.write_macro_placement = Some(value.to_string()),
            "-soft_blockage_weight" => {
                saw_soft_blockage_weight = true;
                o.soft_blockage_weight = num(value)?;
            }
            // Deprecated alias. ⛔ MPL-69 if given together with the modern spelling.
            "-macro_blockage_weight" => {
                saw_macro_blockage_weight = true;
                warnings.push(MplWarning {
                    code: 70,
                    message: "-macro_blockage_weight is deprecated, use -soft_blockage_weight \
                              instead."
                        .to_string(),
                });
                o.soft_blockage_weight = num(value)?;
            }
            "-halo_width" => halo_width = Some(int(value)?),
            "-halo_height" => halo_height = Some(int(value)?),
            other => {
                return Err(MplError::new(
                    0,
                    format!("Unknown keyword {other} for rtl_macro_placer."),
                ))
            }
        }
    }

    if saw_macro_blockage_weight && saw_soft_blockage_weight {
        return Err(MplError::new(
            69,
            "Cannot set -macro_blockage_weight along with -soft_blockage_weight. Use only one \
             of those keys.",
        ));
    }

    // 🔑 Upstream applies the deprecated halo keys DURING argument parsing, calling
    // `set_base_halo w h w h`. Giving only one sets the other to it.
    if halo_width.is_some() || halo_height.is_some() {
        warnings.push(MplWarning {
            code: 74,
            message: "-halo_width/-halo_height are deprecated, use the set_macro_base_halo \
                      command instead."
                .to_string(),
        });
        let w = halo_width.or(halo_height).unwrap_or(0);
        let h = halo_height.or(halo_width).unwrap_or(0);
        o.base_halo_from_flags = Some(Halo { left: w, bottom: h, right: w, top: h });
    }

    Ok(Parsed { options: o, warnings })
}
