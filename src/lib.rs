// SPDX-License-Identifier: Apache-2.0
//! `vyges-mpl` — hierarchical macro placement over the OpenDB database.
//!
//! The reference implementation is OpenROAD's `mpl`; the rules are re-derived from its source and
//! written as ours (decision B — same algorithm, our architecture). Spec and stage-by-stage
//! flowchart: the stage table in `pipeline::ORDER`, which mirrors upstream `HierRTLMP::run`.
//!
//! ## Scope — stage 1 is `mpl` MINUS `par`
//!
//! ⛔ Upstream's clustering calls TritonPart (`par`, ~469 KB) from `breakLargeFlatCluster` to split
//! a **flat** cluster — one with no `dbModule` children — whose leaf standard cells exceed
//! `max_std_cell_`. This engine does not implement that, and **refuses rather than approximating**.
//!
//! 🔑 Measured at pin `945a9f4`: that path is reached by **0 of upstream's 36 `rtl_macro_placer`
//! cases** (the largest suite design has 400 standard cells against a 5,000 threshold), so the
//! whole suite is scoreable without it. ⚠️ **That makes a green suite a weaker claim than it
//! looks** — a real design with a large flat block still needs `par`. `--describe` says so.

pub mod apply;
pub mod cluster;
pub mod design;
pub mod engine;
pub mod feasibility;
pub mod dump;
pub mod halo;
pub mod ioclusters;
pub mod options;
pub mod macroclass;
pub mod merge;
pub mod netlist;
pub mod pipeline;
#[cfg(unix)]
pub mod read;
pub mod regions;
pub mod rng;
pub mod report;
pub mod shaping;
pub mod status;
pub mod thresholds;
pub mod trace;
pub mod tree;

pub use options::{parse_placer_args, Halo, MplError, PlacerOptions, Region};
pub use pipeline::{Outcome, Plan, StageId};
pub use status::{settle_status, Status};
