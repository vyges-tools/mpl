// SPDX-License-Identifier: Apache-2.0
//! The `coarse_shaping` trace format.
//!
//! 🔑 **Every expected string here was COPIED from a real upstream run** — `openroad -no_init`
//! at pin `945a9f48dc6e5cc91d865daa92c45a1094cb682c` with `set_debug_level MPL coarse_shaping 2`
//! over the `mpl` regression designs. They are not reconstructions from the format strings, and
//! they are what the harness diffs against, so a "tidier" spelling of any of them is a defect.
//!
//! ⚠️ Nangate45 has **2000 database units per micron**; every micron value below is the measured
//! line's number multiplied by that.

use vyges_mpl::design::Rect;
use vyges_mpl::halo::Boundary;
use vyges_mpl::shaping::{DepthLimits, Tiling};
use vyges_mpl::trace::CoarseTrace;

const DBU: i32 = 2000;

fn rec() -> CoarseTrace {
    CoarseTrace::recording()
}

/// ⛔ A silent recorder must cost nothing and keep nothing — the stage runs far more often than
/// it is scored, and a trace that accumulated anyway would be paid for on every engine run.
#[test]
fn silent_records_nothing() {
    let mut t = CoarseTrace::silent();
    t.determine_shapes("root");
    t.is_macro_cluster("MACRO_4");
    t.base_depth(11676, DBU);
    assert!(!t.is_recording());
    assert_eq!(t.finish(), "");
}

/// Upstream: `debugPrint(..., "Determine shapes for {}", parent->getName())`.
#[test]
fn traversal_lines_match_upstream() {
    let mut t = rec();
    t.determine_shapes("root");
    t.started_visiting("root");
    t.determine_shapes("MACRO_4");
    t.is_macro_cluster("MACRO_4");
    t.done_visiting("root");
    assert_eq!(
        t.finish(),
        "[DEBUG MPL-coarse_shaping] Determine shapes for root\n\
         [DEBUG MPL-coarse_shaping] Started visiting children of root\n\
         [DEBUG MPL-coarse_shaping] Determine shapes for MACRO_4\n\
         [DEBUG MPL-coarse_shaping] MACRO_4 is a Macro cluster\n\
         [DEBUG MPL-coarse_shaping] Done visiting children of root\n"
    );
}

/// Upstream builds ONE string with embedded newlines and makes ONE `debugPrint` call, so the
/// prefix appears once and the tilings land unprefixed on the next line.
///
/// ⚠️ The trailing `"\n"` upstream appends leaves a BLANK line after the block. Measured from
/// `boundary_push1`, whose `MACRO_4` tiles as a single 200000 × 200280 macro.
#[test]
fn hard_cluster_tilings_match_upstream() {
    let mut t = rec();
    t.hard_cluster_tilings("MACRO_4", 1, &[Tiling { width: 200_000, height: 200_280 }]);
    assert_eq!(
        t.finish(),
        "[DEBUG MPL-coarse_shaping] Tiling for hard cluster MACRO_4 with 1 macros.\n\
         \x20< 200000 , 200280 >  \n\n"
    );
}

/// ⚠️ Two spaces after each `>`, so consecutive tilings are separated by them and the block still
/// ends in two spaces before the newline.
#[test]
fn several_tilings_are_separated_by_two_spaces() {
    let mut t = rec();
    t.hard_cluster_tilings(
        "MACRO_1",
        2,
        &[Tiling { width: 100, height: 200 }, Tiling { width: 200, height: 100 }],
    );
    assert!(t.finish().contains(" < 100 , 200 >   < 200 , 100 >  \n"));
}

/// ⚠️ An EMPTY tiling list still prints its header — upstream traces after `setTilings` with no
/// test on the list. Silence would be indistinguishable from the cluster never being shaped.
///
/// ⚠️ **THREE newlines, not two.** The header carries one, upstream appends another for the
/// tilings that are not there, and the logger adds its own — so an empty block is a header
/// followed by two blank lines, where a populated one has a single blank line.
#[test]
fn empty_tilings_still_print_a_header() {
    let mut t = rec();
    t.hard_cluster_tilings("MACRO_9", 3, &[]);
    assert_eq!(
        t.finish(),
        "[DEBUG MPL-coarse_shaping] Tiling for hard cluster MACRO_9 with 3 macros.\n\n\n"
    );
}

/// Upstream uses `logger_->report`, NOT `debugPrint` — so the table carries **no prefix**, and it
/// is gated at level 1 while the tiling lines are gated at level 2.
///
/// ⚠️ The two columns have DIFFERENT field widths: `{:>5.2f}` then `{:>6.2f}`.
#[test]
fn depth_table_matches_upstream() {
    let mut t = rec();
    t.depth_limits(
        &DepthLimits { x_min: 17_600, x_max: 44_000, y_min: 17_700, y_max: 44_240 },
        DBU,
    );
    assert_eq!(
        t.finish(),
        "\n  Pin Access Depth (μm)  |  Min  |  Max\n\
         -----------------------------------------\n\
         \x20            Horizontal  |  8.80 |  22.00\n\
         \x20              Vertical  |  8.85 |  22.12\n\n"
    );
}

/// ⚠️ **GREEK μ (U+03BC) here.** The per-blockage line one function away uses ASCII `um`;
/// upstream's two format strings genuinely differ and normalising them breaks the diff.
#[test]
fn base_depth_uses_the_greek_micro_sign() {
    let mut t = rec();
    t.base_depth(11_676, DBU);
    let got = t.finish();
    assert_eq!(got, "[DEBUG MPL-coarse_shaping] Base pin access depth: 5.838 μm\n");
    assert!(got.contains('\u{03bc}'));
}

/// ⚠️ A whole number of microns prints WITHOUT a trailing `.0` — `std::format`'s shortest
/// round-trip for a `double`, which Rust's `{}` for `f64` matches. Measured: `Depth = 6 um`.
#[test]
fn whole_microns_print_without_a_decimal_point() {
    let mut t = rec();
    t.creating_blockage(
        Boundary::L,
        &Rect { x_min: 0, y_min: 0, x_max: 0, y_max: 250_000 },
        12_000,
        DBU,
    );
    assert!(t.finish().ends_with("Depth = 6 um\n"));
}

/// ⚠️ **Doubled parentheses, and not a typo**: upstream wraps `({})` around a point that already
/// prints itself as `( x y )`. ⚠️ ASCII `um`, unlike the base-depth line.
#[test]
fn creating_blockage_matches_upstream() {
    let mut t = rec();
    t.creating_blockage(
        Boundary::L,
        &Rect { x_min: 0, y_min: 100_000, x_max: 0, y_max: 150_000 },
        23_352,
        DBU,
    );
    assert_eq!(
        t.finish(),
        "[DEBUG MPL-coarse_shaping] Creating pin access blockage in L -> \
         Region line = (( 0 100000 )) (( 0 150000 )) , Depth = 11.676 um\n"
    );
}

/// odb streams a `Rect` as two points with spaces INSIDE every parenthesis.
#[test]
fn found_blocked_region_matches_upstream() {
    let mut t = rec();
    t.found_blocked_region(
        &Rect { x_min: 0, y_min: 50_000, x_max: 0, y_max: 200_000 },
        Boundary::L,
    );
    assert_eq!(
        t.finish(),
        "[DEBUG MPL-coarse_shaping] Found blocked region ( 0 50000 ) ( 0 200000 ) in L boundary.\n"
    );
}

/// The trace's boundary vocabulary is a single letter per edge — it is diffed, not read.
#[test]
fn boundary_names_are_single_letters() {
    assert_eq!(
        [Boundary::B.name(), Boundary::L.name(), Boundary::T.name(), Boundary::R.name()],
        ["B", "L", "T", "R"]
    );
}
