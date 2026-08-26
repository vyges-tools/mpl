// SPDX-License-Identifier: Apache-2.0
//! The coarse-shaping trace — **our second oracle's format**.
//!
//! Upstream emits `runCoarseShaping`'s working through the `coarse_shaping` debug channel, and at
//! level 2 that channel prints every value the stage computes: the traversal order, each hard
//! cluster's tilings, the pin-access depth limits, the base depth, and **one line per pin-access
//! blockage**. Reproduced here byte for byte, so `openroad -no_init` output and ours diff
//! directly.
//!
//! 🔑 **Why this is worth more than the design report.** `reportDesignData` scores the resolved
//! halos through a single aggregate — `Area of macros with halos` — so a compensating pair of
//! errors survives it. This trace names every blockage's boundary, endpoints and depth
//! separately, and every hard cluster's tilings separately, so it does not.
//!
//! ⚠️ **Two different micro signs, on adjacent lines.** `Base pin access depth` ends in a GREEK
//! `μm` (U+03BC); the per-blockage line ends in an ASCII `um`. Upstream's two format strings
//! genuinely differ, and normalising them to one spelling breaks the comparison.
//!
//! ⚠️ **The depth table is a `report`, not a `debugPrint`** — it carries NO `[DEBUG ...]` prefix
//! and it is gated at level 1, while the tiling lines are gated at level 2. Both appear in a
//! level-2 run, which is why the harness always asks for 2.

use crate::design::Rect;
use crate::halo::Boundary;
use crate::shaping::{DepthLimits, Tiling};

/// Upstream's own prefix for this channel. Every `debugPrint` line carries it; the depth table
/// does not.
const DEBUG: &str = "[DEBUG MPL-coarse_shaping] ";

/// A recorder for the `coarse_shaping` channel.
///
/// 🔑 **Silent by default, and that is the point.** The stage is run by the engine far more often
/// than it is scored, so the trace has to cost nothing when nobody is reading it; every method
/// returns immediately unless the recorder was built with [`CoarseTrace::recording`].
#[derive(Debug, Default)]
pub struct CoarseTrace {
    on: bool,
    out: String,
}

impl CoarseTrace {
    /// A recorder that discards everything. Used by every caller that only wants the result.
    pub fn silent() -> Self {
        Self { on: false, out: String::new() }
    }

    /// A recorder that keeps the lines, for diffing against upstream.
    ///
    /// ⚠️ `dbu_per_micron` is needed because half of these lines are in MICRONS and the other
    /// half in database units. Upstream converts with `block_->dbuToMicrons` at each site, and
    /// which sites those are is not guessable from the values.
    pub fn recording() -> Self {
        Self { on: true, out: String::new() }
    }

    pub fn is_recording(&self) -> bool {
        self.on
    }

    /// Everything recorded, in order.
    pub fn finish(self) -> String {
        self.out
    }

    fn debug_line(&mut self, body: &str) {
        if !self.on {
            return;
        }
        self.out.push_str(DEBUG);
        self.out.push_str(body);
        self.out.push('\n');
    }

    fn report_line(&mut self, body: &str) {
        if !self.on {
            return;
        }
        self.out.push_str(body);
        self.out.push('\n');
    }

    // ------------------------------------------------------------ calculateChildrenTilings

    /// ⚠️ Printed **after** the `num_macro == 0` base case returns, so a cluster with no macros
    /// never announces itself.
    pub fn determine_shapes(&mut self, name: &str) {
        self.debug_line(&format!("Determine shapes for {name}"));
    }

    pub fn is_macro_cluster(&mut self, name: &str) {
        self.debug_line(&format!("{name} is a Macro cluster"));
    }

    /// ⚠️ Both visiting lines are inside upstream's `if (!parent->getChildren().empty())`. A
    /// childless mixed cluster prints neither, and an unguarded loop that happens to iterate zero
    /// times would still print both.
    pub fn started_visiting(&mut self, name: &str) {
        self.debug_line(&format!("Started visiting children of {name}"));
    }

    pub fn done_visiting(&mut self, name: &str) {
        self.debug_line(&format!("Done visiting children of {name}"));
    }

    // ------------------------------------------------------------ calculateMacroTilings

    /// The hard cluster's tilings.
    ///
    /// ⚠️ `number_of_macros` is the cluster's OWN count, even when the tilings that were finally
    /// accepted came from the `n + 1` retry. Printing the retry's count would misreport every
    /// cluster that needed it.
    ///
    /// ⚠️ The body is built as ONE string with embedded newlines and handed to a single
    /// `debugPrint`, so the prefix appears once and the tilings land on an unprefixed second
    /// line. The trailing `"\n"` upstream appends leaves a BLANK line after each block.
    pub fn hard_cluster_tilings(&mut self, name: &str, number_of_macros: usize, tilings: &[Tiling]) {
        if !self.on {
            return;
        }
        let mut line =
            format!("Tiling for hard cluster {name} with {number_of_macros} macros.\n");
        for t in tilings {
            // ⚠️ Two spaces after `>`, one space inside every angle bracket. Upstream builds this
            // by concatenation and the spacing is load-bearing for a byte comparison.
            line.push_str(&format!(" < {} , {} >  ", t.width, t.height));
        }
        line.push('\n');
        self.debug_line(&line);
    }

    // ------------------------------------------------------------ pin access

    /// ⚠️ `{:>5.2}` and `{:>6.2}` — the two columns have DIFFERENT widths, and the header rule is
    /// 41 characters. A leading blank line opens the table and a trailing one closes it.
    pub fn depth_limits(&mut self, limits: &DepthLimits, dbu_per_micron: i32) {
        if !self.on {
            return;
        }
        let um = |v: i64| microns(v, dbu_per_micron);
        self.report_line("");
        self.report_line("  Pin Access Depth (μm)  |  Min  |  Max");
        self.report_line("-----------------------------------------");
        self.report_line(&format!(
            "             Horizontal  | {:>5.2} | {:>6.2}",
            um(limits.x_min),
            um(limits.x_max)
        ));
        self.report_line(&format!(
            "               Vertical  | {:>5.2} | {:>6.2}",
            um(limits.y_min),
            um(limits.y_max)
        ));
        self.report_line("");
    }

    /// ⚠️ GREEK `μm` here — the only site on this channel that uses it.
    pub fn base_depth(&mut self, depth: i64, dbu_per_micron: i32) {
        self.debug_line(&format!(
            "Base pin access depth: {} μm",
            microns(depth, dbu_per_micron)
        ));
    }

    pub fn found_blocked_region(&mut self, region: &Rect, boundary: Boundary) {
        self.debug_line(&format!(
            "Found blocked region {} in {} boundary.",
            rect(region),
            boundary.name()
        ));
    }

    /// ⚠️ **Doubled parentheses in the OUTPUT, from a single pair in the format string.**
    /// Upstream wraps `({})` around a point that already prints itself as `( x y )`, giving
    /// `(( 0 100000 ))`. Writing the doubled pair here produces THREE, which is what the first
    /// attempt did. ⚠️ ASCII `um` here, unlike the base depth's Greek `μm`.
    pub fn creating_blockage(
        &mut self,
        boundary: Boundary,
        line: &Rect,
        depth: i64,
        dbu_per_micron: i32,
    ) {
        self.debug_line(&format!(
            "Creating pin access blockage in {} -> Region line = ({}) ({}) , Depth = {} um",
            boundary.name(),
            point(line.x_min, line.y_min),
            point(line.x_max, line.y_max),
            microns(depth, dbu_per_micron)
        ));
    }
}

/// `dbuToMicrons`: a plain division, printed by `{}` — which drops a trailing `.0`, exactly as
/// `std::format` does for a `double`.
fn microns(v: i64, dbu_per_micron: i32) -> f64 {
    v as f64 / dbu_per_micron as f64
}

/// odb's `Rect` streaming: two points, spaces INSIDE every parenthesis.
fn rect(r: &Rect) -> String {
    format!("{} {}", point(r.x_min, r.y_min), point(r.x_max, r.y_max))
}

/// odb's `Point` streaming.
fn point(x: i64, y: i64) -> String {
    format!("( {x} {y} )")
}
