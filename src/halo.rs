// SPDX-License-Identifier: Apache-2.0
//! Halo resolution — how much keep-out each side of a macro gets.
//!
//! This is the first rule a macro meets and it feeds everything downstream: the haloed bbox sets
//! the cluster's area, the soft blockage written at commit time, and whether the design is
//! declared not to fit at all. Getting a side wrong is invisible on a symmetric halo and wrong
//! everywhere on an asymmetric one.
//!
//! Upstream: `ClusteringEngine::buildMacroHalo`.

use crate::options::Halo;

/// Which side of the macro a pin is closest to.
///
/// 🔑 **The discriminant order is load-bearing.** Upstream sorts `(distance, Boundary)` pairs and
/// takes the first, so when two sides are equidistant the *enum order* breaks the tie. Upstream
/// declares them `B, L, T, R` — not the alphabetical or clockwise order one would guess.
///
/// ⚠️ **Only the order WITHIN a direction pair can ever matter** — `L` before `R`, and `B` before
/// `T`. A tie between a vertical and a horizontal edge is resolved by the pin's layer direction
/// before the enum is consulted, so `B`-vs-`L` ordering is unobservable. Found by mutation
/// testing: swapping `B` and `L` changes nothing, and a test asserting otherwise would be inert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Boundary {
    B = 0,
    L = 1,
    T = 2,
    R = 3,
}

impl Boundary {
    /// Left and right are the *vertical* edges. ⚠️ Reads backwards at first glance: it asks
    /// whether the EDGE runs vertically, not whether the direction of travel is vertical.
    pub fn is_vertical(self) -> bool {
        matches!(self, Boundary::L | Boundary::R)
    }

    /// Upstream `toString(Boundary)` — the single letter its trace prints.
    ///
    /// ⚠️ These letters are the trace's own vocabulary, not a display convenience: the
    /// `coarse_shaping` oracle is diffed line for line, so `L` may never become `Left`.
    pub fn name(self) -> &'static str {
        match self {
            Boundary::B => "B",
            Boundary::L => "L",
            Boundary::T => "T",
            Boundary::R => "R",
        }
    }
}

/// Routing direction of the layer a pin sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerDir {
    Horizontal,
    Vertical,
}

/// A pin shape on a macro master, in master coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinBox {
    pub x_min: i64,
    pub y_min: i64,
    pub x_max: i64,
    pub y_max: i64,
    /// The direction of the pin's *first* geometry layer. Upstream reads
    /// `mpin->getGeometry().begin()`, i.e. the first box's layer, even when deciding for a
    /// later box of the same pin.
    pub layer_dir: LayerDir,
}

/// Orientation, only to the extent halo resolution cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orient {
    R0,
    R180,
    Mx,
    My,
    Other,
}

/// Which boundary a pin box is closest to, with upstream's tie-breaks.
///
/// Upstream builds `[(x_min, L), (y_min, B), (width - x_max, R), (height - y_max, T)]`, sorts it,
/// and takes the first. Then: **if the two closest are equidistant AND one is a vertical edge
/// while the other is not**, the pin's layer direction decides — a vertically-routed pin prefers
/// the horizontal edge (top/bottom) and vice versa.
pub fn closest_boundary(pin: PinBox, master_width: i64, master_height: i64) -> Boundary {
    // ⚠️ Construction order is irrelevant (we sort), but the PAIRING is not: L takes x_min,
    // B takes y_min, R takes width - x_max, T takes height - y_max.
    let mut candidates = [
        (pin.x_min, Boundary::L),
        (pin.y_min, Boundary::B),
        (master_width - pin.x_max, Boundary::R),
        (master_height - pin.y_max, Boundary::T),
    ];
    // Sorts by distance, then by the Boundary discriminant -- the tie-break noted above.
    candidates.sort();

    let first = candidates[0];
    let second = candidates[1];

    let equidistant_different_directions = first.0 == second.0
        && (first.1.is_vertical() != second.1.is_vertical());

    if !equidistant_different_directions {
        return first.1;
    }

    match pin.layer_dir {
        // A vertically-routed pin reaches the horizontal edges, so prefer the NON-vertical one.
        LayerDir::Vertical => {
            if first.1.is_vertical() { second.1 } else { first.1 }
        }
        LayerDir::Horizontal => {
            if first.1.is_vertical() { first.1 } else { second.1 }
        }
    }
}

/// The macro's declared halo, before pin-awareness narrows it.
///
/// Upstream rule:
/// - an explicit `set_macro_halo` for this instance wins outright;
/// - otherwise a **soft** `dbHalo` is taken as-is;
/// - a **hard** `dbHalo` is raised componentwise to at least the base halo;
/// - no `dbHalo` at all means the base halo.
///
/// ⚠️ The soft case does NOT floor to the base halo. A soft halo is a hint other tools already
/// respect, so upstream declines to enlarge it.
pub fn full_halo(
    explicit: Option<Halo>,
    inst_halo: Option<(Halo, bool)>, // (halo, is_soft)
    base: Halo,
) -> Halo {
    if let Some(h) = explicit {
        return h;
    }
    match inst_halo {
        Some((h, true)) => h,
        Some((h, false)) => Halo {
            left: h.left.max(base.left),
            bottom: h.bottom.max(base.bottom),
            right: h.right.max(base.right),
            top: h.top.max(base.top),
        },
        None => base,
    }
}

/// Reorient a halo for a **fixed** macro.
///
/// 🔑 Upstream does this only for fixed macros, and says why at the site: unfixed macros get their
/// orientation adjusted later by the orientation-improve pass, which fixed macros skip entirely.
/// So a fixed macro's halo would otherwise stay in unflipped coordinates forever.
///
/// `MX` and `R180` swap bottom/top; `MY` and `R180` swap left/right. `R180` does both.
pub fn reorient_for_fixed(halo: Halo, orient: Orient) -> Halo {
    let mut h = halo;
    if matches!(orient, Orient::Mx | Orient::R180) {
        std::mem::swap(&mut h.bottom, &mut h.top);
    }
    if matches!(orient, Orient::My | Orient::R180) {
        std::mem::swap(&mut h.left, &mut h.right);
    }
    h
}

/// The pin-aware halo: start at `minimum_spacing` on every side, then widen a side to the full
/// halo for each signal pin that is closest to it.
///
/// ⚠️ **A side with no pin facing it keeps `minimum_spacing`.** That is the entire point of the
/// pin-aware mode — the routing keep-out only needs to be wide where pins actually escape.
///
/// `pins` must already be filtered to SIGNAL pins; power and ground do not widen a halo.
pub fn pin_aware_halo(
    pins: &[PinBox],
    master_width: i64,
    master_height: i64,
    full: Halo,
    minimum_spacing: i64,
) -> Halo {
    let mut halo = Halo {
        left: minimum_spacing,
        bottom: minimum_spacing,
        right: minimum_spacing,
        top: minimum_spacing,
    };
    for pin in pins {
        match closest_boundary(*pin, master_width, master_height) {
            Boundary::B => halo.bottom = full.bottom,
            Boundary::L => halo.left = full.left,
            Boundary::T => halo.top = full.top,
            Boundary::R => halo.right = full.right,
        }
    }
    halo
}

/// The whole rule, in upstream's order.
#[allow(clippy::too_many_arguments)]
pub fn build_macro_halo(
    explicit: Option<Halo>,
    inst_halo: Option<(Halo, bool)>,
    base: Halo,
    use_full_halo: bool,
    pins: &[PinBox],
    master_width: i64,
    master_height: i64,
    minimum_spacing: i64,
    is_fixed: bool,
    orient: Orient,
) -> Halo {
    // ⚠️ The explicit halo returns IMMEDIATELY, before `use_full_halo` is even consulted and
    // before any reorientation. A `set_macro_halo` is taken exactly as written.
    if let Some(h) = explicit {
        return h;
    }

    let full = full_halo(None, inst_halo, base);
    if use_full_halo {
        return full;
    }

    let halo = pin_aware_halo(pins, master_width, master_height, full, minimum_spacing);
    if is_fixed {
        return reorient_for_fixed(halo, orient);
    }
    halo
}
