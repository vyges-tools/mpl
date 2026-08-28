// SPDX-License-Identifier: Apache-2.0
//! The boundary push, scored against upstream's OWN unit test for it.
//!
//! 🔑 **These ten cases are a transcription of `src/mpl/test/cpp/TestPusher.cpp`'s scenarios** —
//! the geometry, the fixture constants and the expected positions are upstream's, re-expressed
//! against our composition. They are not a second opinion about the same designs the
//! `boundary_push` gate already scores: they reach behaviour the 34-design regression suite
//! **cannot**.
//!
//! ⛔ **Three paths in this file are executed by NO design in the reference suite:**
//!   * the two-guard early returns, which the suite only ever reaches with other children present;
//!   * the HARD MACRO overlap revert — the suite's single revert is an IO blockage, so
//!     `overlapsWithHardMacro`'s trace line had never been executed on either side;
//!   * a fixed macro cluster reached by the push loop at all.
//!
//! ⚠️ **The fixture is upstream's**: a 500000 × 500000 core, 100000 × 100000 macros. Changing
//! either silently changes which boundaries come within one macro of the cluster, which is the
//! whole test.

use vyges_mpl::cluster::ClusterType;
use vyges_mpl::placement::{run_boundary_push, PushCluster, PushMacro};

const DIE: i32 = 500_000;
const CORE: (i32, i32, i32, i32) = (0, 0, DIE, DIE);
const MACRO_W: i32 = 100_000;
const MACRO_H: i32 = 100_000;

/// One macro cluster holding exactly one macro, as `addMacroCluster` builds it.
///
/// ⚠️ **The cluster's box and the macro's position are set TOGETHER and to the same corner.**
/// Upstream's helper does the same, and it matters: the pusher measures the distance from the
/// CLUSTER's soft macro and then moves the HARD macros by it, so a fixture that let the two drift
/// apart would be testing an arrangement the placer cannot produce.
struct Design {
    clusters: Vec<PushCluster>,
    macros: Vec<PushMacro>,
    root_children: Vec<(ClusterType, i64)>,
    next_id: i32,
}

impl Design {
    /// A bare root, as `TestPusher`'s tests that expect no push start from.
    fn new() -> Self {
        Design { clusters: Vec::new(), macros: Vec::new(), root_children: Vec::new(), next_id: 1 }
    }

    /// Upstream `makeRootWithStdCells`, and its comment says exactly why it exists: to give the
    /// root a standard-cell child with a NON-ZERO soft macro so the design is not mistaken for a
    /// single centralized macro array and skipped entirely.
    fn with_std_cells() -> Self {
        let mut d = Design::new();
        d.root_children
            .push((ClusterType::StdCell, MACRO_W as i64 * MACRO_H as i64));
        d.next_id += 1;
        d
    }

    fn add_macro_cluster(&mut self, name: &str, x: i32, y: i32, w: i32, h: i32) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        let index = self.macros.len();
        self.macros.push(PushMacro {
            name: format!("{name}_hard"),
            cluster_id: id,
            location: (x, y),
            width: w,
            height: h,
        });
        self.clusters.push(PushCluster {
            id,
            name: name.to_string(),
            is_fixed_macro: false,
            bbox: (x, y, x + w, y + h),
            macros: vec![index],
        });
        self.root_children.push((ClusterType::HardMacro, w as i64 * h as i64));
        id
    }

    fn add_macro(&mut self, name: &str, x: i32, y: i32) -> i32 {
        self.add_macro_cluster(name, x, y, MACRO_W, MACRO_H)
    }

    /// A cluster of TWO macros of different sizes, and a box that spans both.
    ///
    /// ⚠️ **Upstream's own grouping cannot produce this** — it groups only macros of the same size,
    /// which is why `getHardMacros().front()` is safe there. It is built here anyway, because that
    /// invariant is what makes front-versus-any unobservable, and an untestable transcription is
    /// one nobody can check.
    fn add_two_macro_cluster(
        &mut self,
        name: &str,
        first: (i32, i32, i32, i32),
        last: (i32, i32, i32, i32),
    ) {
        let id = self.next_id;
        self.next_id += 1;
        let base = self.macros.len();
        for (suffix, m) in [("first", first), ("last", last)] {
            self.macros.push(PushMacro {
                name: format!("{name}_{suffix}"),
                cluster_id: id,
                location: (m.0, m.1),
                width: m.2,
                height: m.3,
            });
        }
        self.clusters.push(PushCluster {
            id,
            name: name.to_string(),
            is_fixed_macro: false,
            bbox: (
                first.0.min(last.0),
                first.1.min(last.1),
                (first.0 + first.2).max(last.0 + last.2),
                (first.1 + first.3).max(last.1 + last.3),
            ),
            macros: vec![base, base + 1],
        });
        self.root_children.push((ClusterType::HardMacro, 1));
    }

    fn run(&mut self, io_blockages: &[(i32, i32, i32, i32)]) -> Vec<String> {
        run_boundary_push(
            ClusterType::Mixed,
            &self.root_children,
            &self.clusters,
            &mut self.macros,
            CORE,
            io_blockages,
        )
    }

    fn at(&self, index: usize) -> (i32, i32) {
        self.macros[index].location
    }
}

/// ⛔ **`RootIsHardMacroCluster`** — a design that is nothing but macros returns immediately and
/// touches no macro. ⚠️ The root's own macros are not even gathered: `fetchMacroClusters` walks
/// the root's CHILDREN.
#[test]
fn a_root_that_is_a_macro_cluster_is_not_pushed() {
    let mut macros = vec![PushMacro {
        name: "root_macro".to_string(),
        cluster_id: 0,
        location: (10_000, 10_000),
        width: MACRO_W,
        height: MACRO_H,
    }];
    let trace = run_boundary_push(ClusterType::HardMacro, &[], &[], &mut macros, CORE, &[]);
    assert!(trace.is_empty(), "no trace at all: {trace:?}");
    assert_eq!(macros[0].location, (10_000, 10_000));
}

/// ⛔ **`SingleCentralizedMacroArray`** — one macro cluster under the root and nothing else that
/// counts. The array is already where it was meant to be, so the push declines.
#[test]
fn a_single_centralized_macro_array_is_not_pushed() {
    let mut d = Design::new();
    d.add_macro("macro_cluster", 10_000, 200_000);
    let trace = d.run(&[]);
    assert!(trace.is_empty(), "no trace at all: {trace:?}");
    assert_eq!(d.at(0), (10_000, 200_000));
}

/// **`MacroPushedToLeftBoundary`** — 10000 from the left edge, less than one macro width.
#[test]
fn a_macro_within_one_width_of_the_left_edge_is_pushed_to_it() {
    let mut d = Design::with_std_cells();
    d.add_macro("macro_cluster", 10_000, 0);
    let trace = d.run(&[]);
    assert_eq!(d.at(0).0, 0);
    assert_eq!(
        trace,
        vec![
            "[DEBUG MPL-boundary_push] Macro Cluster macro_cluster",
            "Distance to Close Boundaries:",
            // ⚠️ `B 0` is REPORTED and then SKIPPED — the cluster already sits on the bottom edge,
            // and a zero distance is not an attempted push.
            "B 0",
            "L 10000",
            "[DEBUG MPL-boundary_push] Moved macro_cluster in the direction of L.",
        ]
    );
}

/// **`MacroPushedToRightBoundary`.**
#[test]
fn a_macro_within_one_width_of_the_right_edge_is_pushed_to_it() {
    let mut d = Design::with_std_cells();
    d.add_macro("macro_cluster", DIE - MACRO_W - 10_000, 0);
    d.run(&[]);
    assert_eq!(d.at(0).0, DIE - MACRO_W);
}

/// **`MacroPushedToBottomBoundary`.**
#[test]
fn a_macro_within_one_height_of_the_bottom_edge_is_pushed_to_it() {
    let mut d = Design::with_std_cells();
    d.add_macro("macro_cluster", 0, 5_000);
    d.run(&[]);
    assert_eq!(d.at(0).1, 0);
}

/// **`MacroPushedToTopBoundary`.**
#[test]
fn a_macro_within_one_height_of_the_top_edge_is_pushed_to_it() {
    let mut d = Design::with_std_cells();
    d.add_macro("macro_cluster", 0, DIE - MACRO_H - 10_000);
    d.run(&[]);
    assert_eq!(d.at(0).1, DIE - MACRO_H);
}

/// ⛔ **`FixedMacroCluster`** — and note how upstream BUILDS it: `setClusterType(HardMacroCluster)`
/// and THEN `setAsFixedMacro`, on the same cluster. That is the reference's own confirmation that
/// a fixed macro cluster's TYPE is `HardMacro` and the fixed-ness is a separate flag.
///
/// ⚠️ It is skipped inside the loop, so it emits NO trace line at all — not even its name.
#[test]
fn a_fixed_macro_cluster_is_skipped_without_a_trace_line() {
    let mut d = Design::with_std_cells();
    d.add_macro("fixed_cluster", 10_000, 10_000);
    d.clusters[0].is_fixed_macro = true;

    let trace = d.run(&[]);
    assert!(trace.is_empty(), "skipped before its name is printed: {trace:?}");
    assert_eq!(d.at(0), (10_000, 10_000));
}

/// ⛔ **`PushRevertedHorizontal`** — the horizontal push would land on another cluster's macro, so
/// it is reverted; the vertical one is kept.
///
/// 🔑 **This is the HARD MACRO revert, and NO design in the reference suite reaches it** — the
/// suite's one revert is an IO blockage. Both the branch and its trace line are exercised only
/// here.
#[test]
fn a_horizontal_push_onto_another_macro_is_reverted() {
    let mut d = Design::with_std_cells();
    d.add_macro("macro1", 10_000, 10_000);
    d.add_macro_cluster("macro2", 0, 10_000, 5_000, 5_000);

    let trace = d.run(&[]);
    assert_eq!(d.at(0), (10_000, 0), "bottom kept, left reverted");

    // 🔑 The WHOLE trace, because three separate rules only show in the order of these lines:
    // `Moved` is printed BEFORE the overlap test and so appears for the reverted push too; the
    // obstacle is named by the macro it hit; and macro2 — visited next, and now too far from
    // every edge to move — prints its name and the header and NOTHING ELSE.
    assert_eq!(
        trace,
        vec![
            "[DEBUG MPL-boundary_push] Macro Cluster macro1",
            "Distance to Close Boundaries:",
            "B 10000",
            "L 10000",
            "[DEBUG MPL-boundary_push] Moved macro1 in the direction of B.",
            "[DEBUG MPL-boundary_push] Moved macro1 in the direction of L.",
            "[DEBUG MPL-boundary_push] \tFound overlap with HardMacro macro2_hard. Push will be \
             reverted.",
            "[DEBUG MPL-boundary_push] Macro Cluster macro2",
            "Distance to Close Boundaries:",
            // ⚠️ macro2 already SITS on the left edge, so its distance is reported as `0` and
            // then skipped without a `Moved` line. Reported and attempted are different things.
            // ⛔ Its VERTICAL pick is `T`, not `B`: the nearer edge is the bottom at 10000, but
            // `distance_to_bottom < distance_to_top` chooses between them and the THRESHOLD then
            // rejects it — 10000 is not within macro2's own 5000 height. Nothing is emitted for a
            // boundary that fails the threshold, so only one row appears.
            "L 0",
        ]
    );
}

/// ⛔ **A cluster too far from every edge prints its name and the header and NOTHING ELSE.**
/// The header is inside the `debugCheck` block, above the loop over the map; the early return for
/// an empty map is further down, in `pushMacroClusterToCoreBoundaries`. `centralization1` is this
/// case in the regression suite, and a version that skipped the header would agree with every
/// design that has something to push and differ only there.
#[test]
fn a_cluster_far_from_every_edge_still_prints_the_header() {
    let mut d = Design::with_std_cells();
    // Dead centre: 200000 from each edge, and the threshold is one macro — 100000.
    d.add_macro("macro_cluster", 200_000, 200_000);

    let trace = d.run(&[]);
    assert_eq!(
        trace,
        vec![
            "[DEBUG MPL-boundary_push] Macro Cluster macro_cluster",
            "Distance to Close Boundaries:",
        ]
    );
    assert_eq!(d.at(0), (200_000, 200_000), "unmoved");
}

/// ⛔ **`PushRevertedVertical`** — the mirror: the bottom push is blocked and the left one lands.
#[test]
fn a_vertical_push_onto_another_macro_is_reverted() {
    let mut d = Design::with_std_cells();
    d.add_macro("macro1", 10_000, 10_000);
    d.add_macro_cluster("macro2", 10_000, 0, 5_000, 5_000);

    d.run(&[]);
    assert_eq!(d.at(0), (0, 10_000), "left kept, bottom reverted");
}

/// ⛔ **`PushRevertedBiased`, and upstream's comment names the rule:** *"The Pusher is biased by
/// the Boundary enum ordering (B > L > T > R)."*
///
/// 🔑 **This is the case that makes the enum order OBSERVABLE.** An obstacle sits diagonally, so
/// only ONE of the two pushes can be taken — and which one depends entirely on which is attempted
/// first, because the second is judged against the box the first left behind. `halo.rs` records
/// that `B`-vs-`L` was unobservable; that was true of `closest_boundary` and is false here.
#[test]
fn the_boundary_enum_order_decides_which_of_two_pushes_survives() {
    let mut d = Design::with_std_cells();
    d.add_macro("macro1", 10_000, 10_000);
    d.add_macro_cluster("macro2", 0, 0, 5_000, 5_000);

    d.run(&[]);
    assert_eq!(d.at(0), (10_000, 0), "B is attempted first and is kept; L is then blocked");
}

/// **`PushRevertedOnIOBlockageOverlap`** — the suite's one revert, in isolation.
#[test]
fn a_push_onto_an_io_blockage_is_reverted() {
    let mut d = Design::with_std_cells();
    d.add_macro("macro_cluster", 10_000, 0);

    let blockage = (0, 0, 50_000, MACRO_H);
    let trace = d.run(&[blockage]);
    assert_eq!(d.at(0), (10_000, 0), "unmoved");
    assert!(
        trace.contains(
            &"[DEBUG MPL-boundary_push] \tFound overlap with IO blockage ( 0 0 ) ( 50000 100000 ). \
              Push will be reverted."
                .to_string()
        ),
        "the blockage is printed as a Rect: {trace:#?}"
    );
}

// ---------------------------------------------------------------- beyond upstream's own cases

/// ⛔ **OURS, not upstream's.** `fetchMacroClusters` gathers a fixed cluster's macros into the flat
/// obstacle list BEFORE the loop tests `isFixedMacro()` — so a fixed macro is never pushed and
/// still blocks other pushes. Upstream has no test for this and the reference suite never reaches
/// it; the claim was read off the gather/skip split and is pinned here rather than in a comment.
#[test]
fn a_fixed_macro_cluster_still_obstructs_another_clusters_push() {
    let mut d = Design::with_std_cells();
    d.add_macro_cluster("fixed_cluster", 0, 10_000, 5_000, 5_000);
    d.clusters[0].is_fixed_macro = true;
    d.add_macro("macro1", 10_000, 10_000);

    d.run(&[]);
    assert_eq!(d.at(0), (0, 10_000), "the fixed macro did not move");
    assert_eq!(d.at(1), (10_000, 0), "and it blocked macro1's left push");
}

/// ⚠️ **The push THRESHOLD is taken from `getHardMacros().front()`**, and upstream says why at the
/// site: only macros of the same size are grouped, so any of them would do.
///
/// ⛔ **That invariant is precisely what makes front-versus-last unobservable on real input** — so
/// a mutation swapping them went NOT CAUGHT against every fixture built from upstream's own
/// scenarios, all of which hold one macro per cluster. It is not an equivalent mutant: the code
/// differs, and only the caller's invariant hides it. Pinned here with a cluster upstream would
/// never build, because the alternative is a transcription nobody can check.
#[test]
fn the_push_threshold_comes_from_the_first_macro_not_the_last() {
    let mut d = Design::with_std_cells();
    // 10000 from the left edge. The FIRST macro is 6000 wide — too narrow to admit the push; the
    // last is 100000 wide, which would admit it.
    d.add_two_macro_cluster("two", (10_000, 0, 6_000, 6_000), (10_000, 20_000, MACRO_W, MACRO_H));

    let trace = d.run(&[]);
    assert_eq!(
        trace,
        vec![
            "[DEBUG MPL-boundary_push] Macro Cluster two",
            "Distance to Close Boundaries:",
            // ⚠️ Only the bottom, and at zero. The left edge is 10000 away, which is NOT within the
            // FIRST macro's 6000 width — reading the last macro's 100000 instead would admit it and
            // add an `L 10000` row and a `Moved` line.
            "B 0",
        ]
    );
    assert_eq!(d.at(0), (10_000, 0), "nothing moved");
}
