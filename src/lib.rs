// SPDX-License-Identifier: Apache-2.0
//! `vyges-mpl` — hierarchical macro placement over the OpenDB database.
//!
//! The reference implementation is an open-source `mpl`; the rules are re-derived from its source and
//! written as ours (decision B — same algorithm, our architecture). Spec and stage-by-stage
//! flowchart: the stage table in `pipeline::ORDER`, which mirrors upstream `HierRTLMP::run`.
//!
//! ## Scope — the partitioner is linked by feature
//!
//! Upstream's clustering calls TritonPart (`par`) from `breakLargeFlatCluster` to split a **flat**
//! cluster — one with no `dbModule` children — whose leaf standard cells exceed `max_std_cell_`.
//! This crate builds that call's hypergraph and applies the answer ([`flatpart`]); the answer
//! comes from [`vyges_par`] when built with the `par` feature ([`par`], Linux x86_64, which the
//! partitioner's exact stages need). ⛔ Without it — or when the partitioner refuses a stage —
//! the cluster is **refused, never approximated**, and `--describe` says which build this is.
//!
//! 🔑 Measured at pin `945a9f4`: that path is reached by **0 of upstream's 36 `rtl_macro_placer`
//! cases** (the largest suite design has 400 standard cells against a 5,000 threshold), so the
//! suite cannot witness it; a flat synthesized design does.

pub mod anneal;
pub mod apply;
pub mod cluster;
pub mod commit;
pub mod design;
pub mod driver;
pub mod engine;
pub mod feasibility;
pub mod flatpart;
pub mod dump;
pub mod halo;
pub mod ioclusters;
pub mod options;
#[cfg(feature = "par")]
pub mod par;
pub mod macroclass;
pub mod merge;
pub mod netlist;
pub mod pipeline;
pub mod placement;
#[cfg(unix)]
pub mod read;
pub mod regions;
pub mod rng;
pub mod run;
pub mod report;
pub mod shaping;
pub mod status;
pub mod thresholds;
pub mod trace;
pub mod tree;

pub use options::{parse_placer_args, Halo, MplError, PlacerOptions, Region};
pub use pipeline::{Outcome, Plan, StageId};
pub use status::{settle_status, Status};
