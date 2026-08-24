// SPDX-License-Identifier: Apache-2.0
//! The stage pipeline — `HierRTLMP::run` as data rather than as control flow.
//!
//! 🔑 **Why this is a list and not a function.** `pdn` ended up with ~50 focused functions plus one
//! 4,600-line composer, and that shape paid off during correlation: stages had to be swapped,
//! repeated and stopped-after to find where a divergence *started*, because a final DEF count
//! cancels wins against losses. What it could not do cheaply was reorder — that meant editing
//! control flow. Here the composer is an ordered list, so the same operations are `Plan` settings.
//!
//! ⚠️ **This list IS upstream `HierRTLMP::run`, in order.** A stage added here that is not in
//! upstream's sequence — or missing from it — is a test failure, not a review comment: see
//! `tests/pipeline.rs`, which transcribes that sequence independently.

use std::fmt;

/// Every stage of `HierRTLMP::run`, in upstream's order.
///
/// The discriminants are the spec's §2 numbering, so a stage cannot be silently renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StageId {
    /// §2.1 — `runMultilevelAutoclustering`: build the physical hierarchy.
    MultilevelAutoclustering = 1,
    /// `commitClusteringDataToDb`, only under `-keep_clustering_data`.
    CommitClusteringData = 2,
    /// `resetSAParameters`, only when the design has no standard cells.
    ResetSaParameters = 3,
    /// §2.2 — `runCoarseShaping`: tilings, pin-access and placement blockages.
    CoarseShaping = 4,
    /// §2.3 — `runHierarchicalMacroPlacement`: the annealer.
    HierarchicalMacroPlacement = 5,
    /// §2.4 — `Pusher::pushMacrosToCoreBoundaries`.
    PushToBoundaries = 6,
    /// `updateMacrosOnDb`.
    UpdateMacrosOnDb = 7,
    /// §2.5 — `generateTemporaryStdCellsPlacement` + `correctAllMacrosOrientation`.
    TemporaryStdCellsAndOrientation = 8,
    /// §2.6 — `commitMacroPlacementToDb` + `writeMacroPlacement`: snap, LOCK, soft blockages.
    CommitMacroPlacement = 9,
    /// `computeWireLength` — reporting only.
    ComputeWireLength = 10,
}

impl StageId {
    /// Upstream's function name, verbatim, so the spec table and this file diff by eye.
    pub fn upstream_name(self) -> &'static str {
        match self {
            StageId::MultilevelAutoclustering => "runMultilevelAutoclustering",
            StageId::CommitClusteringData => "commitClusteringDataToDb",
            StageId::ResetSaParameters => "resetSAParameters",
            StageId::CoarseShaping => "runCoarseShaping",
            StageId::HierarchicalMacroPlacement => "runHierarchicalMacroPlacement",
            StageId::PushToBoundaries => "pushMacrosToCoreBoundaries",
            StageId::UpdateMacrosOnDb => "updateMacrosOnDb",
            StageId::TemporaryStdCellsAndOrientation => "generateTemporaryStdCellsPlacement",
            StageId::CommitMacroPlacement => "commitMacroPlacementToDb",
            StageId::ComputeWireLength => "computeWireLength",
        }
    }
}

impl fmt::Display for StageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.upstream_name())
    }
}

/// What a stage did. ⛔ `Skipped` and `EarlyReturn` both carry a REASON, because
/// "it did nothing" without a why is how a no-op reports success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ran,
    Skipped(&'static str),
    /// Upstream returns from `run()` here — e.g. `skip_macro_placement_` when there are no
    /// unfixed macros (`MPL-0017`). The pipeline stops and the reason reaches `settle_status`.
    EarlyReturn(&'static str),
}

/// The canonical order. ⚠️ **Order is the algorithm**: the same correct steps sequenced
/// differently is a different algorithm, and no golden comparison will tell you which you have.
pub const ORDER: &[StageId] = &[
    StageId::MultilevelAutoclustering,
    StageId::CommitClusteringData,
    StageId::ResetSaParameters,
    StageId::CoarseShaping,
    StageId::HierarchicalMacroPlacement,
    StageId::PushToBoundaries,
    StageId::UpdateMacrosOnDb,
    StageId::TemporaryStdCellsAndOrientation,
    StageId::CommitMacroPlacement,
    StageId::ComputeWireLength,
];

/// How to run the pipeline. The default is "upstream's order, every stage, once".
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// Run up to and including this stage, then stop. The per-stage oracle.
    pub stop_after: Option<StageId>,
    /// Run only these stages, in `ORDER`'s sequence. For testing one stage in isolation.
    pub only: Option<Vec<StageId>>,
    /// Run this stage `n` times in place. Idempotence checking.
    pub repeat: Option<(StageId, usize)>,
    /// Exchange two stages. ⚠️ **An order probe**: if swapping them does NOT change the
    /// result, either the dependency we assumed is not real or one of them is inert.
    pub swap: Option<(StageId, StageId)>,
}

impl Plan {
    /// The stage sequence this plan asks for, resolved against `ORDER`.
    pub fn sequence(&self) -> Vec<StageId> {
        let mut seq: Vec<StageId> = match &self.only {
            Some(only) => ORDER.iter().copied().filter(|s| only.contains(s)).collect(),
            None => ORDER.to_vec(),
        };

        if let Some((a, b)) = self.swap {
            let (ia, ib) = (seq.iter().position(|&s| s == a), seq.iter().position(|&s| s == b));
            if let (Some(ia), Some(ib)) = (ia, ib) {
                seq.swap(ia, ib);
            }
        }

        if let Some((target, n)) = self.repeat {
            if let Some(i) = seq.iter().position(|&s| s == target) {
                // Repeat in place, so the stage's neighbours are unchanged.
                for _ in 1..n {
                    seq.insert(i, target);
                }
            }
        }

        if let Some(stop) = self.stop_after {
            // ⚠️ Truncate after the LAST occurrence, so `repeat` + `stop_after` compose.
            if let Some(i) = seq.iter().rposition(|&s| s == stop) {
                seq.truncate(i + 1);
            }
        }

        seq
    }
}
