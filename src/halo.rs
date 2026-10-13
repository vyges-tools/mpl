// SPDX-License-Identifier: Apache-2.0
//! Halo resolution — how much keep-out each side of a macro gets.
//!
//! This is the first rule a macro meets and it feeds everything downstream: the haloed bbox sets
//! the cluster's area, the soft blockage written at commit time, and whether the design is
//! declared not to fit at all. Getting a side wrong is invisible on a symmetric halo and wrong
//! everywhere on an asymmetric one.
//!
//! Upstream: `ClusteringEngine::buildMacroHalo` and `buildPinAwareHalo`.

use crate::options::{Channel, Halo};

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

/// Raise each side to at least the other halo's same side — upstream `Halo::flooredToHalo`.
///
/// ⚠️ Per side, NOT "whichever halo is bigger": the result can take two sides from each.
pub fn floored_to_halo(halo: Halo, floor: Halo) -> Halo {
    Halo {
        left: halo.left.max(floor.left),
        bottom: halo.bottom.max(floor.bottom),
        right: halo.right.max(floor.right),
        top: halo.top.max(floor.top),
    }
}

/// Raise each side to at least half the minimum channel — upstream `Halo::flooredToChannel`.
///
/// 🔑 **Half, because a channel is shared by the two macros either side of it.** Left and right
/// take `width / 2`, bottom and top `height / 2`, in integer DBU — the division truncates, so an
/// odd channel loses one unit, as upstream's `int / 2` does.
pub fn floored_to_channel(halo: Halo, channel: Channel) -> Halo {
    Halo {
        left: halo.left.max(channel.width / 2),
        bottom: halo.bottom.max(channel.height / 2),
        right: halo.right.max(channel.width / 2),
        top: halo.top.max(channel.height / 2),
    }
}

fn uniform(v: i64) -> Halo {
    Halo { left: v, bottom: v, right: v, top: v }
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

/// The pin-aware halo — upstream `buildPinAwareHalo`.
///
/// Every side starts at `minimum_spacing`; a side that some SIGNAL pin is closest to is raised to
/// the channel-floored value. ⚠️ **A side with no pin facing it keeps `minimum_spacing`**, not half
/// the channel — that is the entire point of the mode: the keep-out need only be wide where pins
/// escape.
///
/// `pins` must already be filtered to SIGNAL pins; power and ground do not widen a halo.
pub fn pin_aware_halo(
    pins: &[PinBox],
    master_width: i64,
    master_height: i64,
    channel: Channel,
    minimum_spacing: i64,
) -> Halo {
    let halo = floored_to_channel(uniform(minimum_spacing), channel);
    let mut min_halo = uniform(minimum_spacing);
    for pin in pins {
        match closest_boundary(*pin, master_width, master_height) {
            Boundary::B => min_halo.bottom = halo.bottom,
            Boundary::L => min_halo.left = halo.left,
            Boundary::T => min_halo.top = halo.top,
            Boundary::R => min_halo.right = halo.right,
        }
    }
    min_halo
}

/// The whole rule, in upstream's order — `buildMacroHalo`.
///
/// 1. `minimum_spacing` on every side, floored to half the minimum channel;
/// 2. a DEF halo on the instance floors that per side — ⚠️ **soft or hard alike**: this function
///    never reads the soft flag (only the commit-time blockage does);
/// 3. otherwise, only when `-pin_aware_channels` is set, the pin-aware halo REPLACES it — so a
///    macro with a DEF halo is never pin-aware;
/// 4. a FIXED macro has the result reoriented, on every path.
#[allow(clippy::too_many_arguments)]
pub fn build_macro_halo(
    inst_halo: Option<Halo>,
    channel: Channel,
    pin_aware_channels: bool,
    pins: &[PinBox],
    master_width: i64,
    master_height: i64,
    minimum_spacing: i64,
    is_fixed: bool,
    orient: Orient,
) -> Halo {
    let mut halo = floored_to_channel(uniform(minimum_spacing), channel);
    if let Some(def) = inst_halo {
        halo = floored_to_halo(halo, def);
    } else if pin_aware_channels {
        halo = pin_aware_halo(pins, master_width, master_height, channel, minimum_spacing);
    }
    if is_fixed {
        return reorient_for_fixed(halo, orient);
    }
    halo
}
