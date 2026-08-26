// SPDX-License-Identifier: Apache-2.0
//! The placement stage's netlist and wirelength model.

use vyges_mpl::halo::Boundary;
use vyges_mpl::placement::{
    build_bundled_nets, compute_nets_wire_length, dist_to_nearest_region, is_outside_the_outline,
    nearest_point_in_region, pin_center, wirelength_for_unplaced_io_pins, BundledNet, Region,
    WirelengthMacro, VIRTUAL_CONNECTION_WEIGHT,
};

fn macro_at(x: i32, y: i32, width: i32, height: i32) -> WirelengthMacro {
    WirelengthMacro {
        x,
        y,
        width,
        height,
        is_cluster_of_unplaced_io_pins: false,
        is_unconstrained_io_cluster: false,
    }
}

/// ⚠️ **`>`, strictly** — so a connection is emitted once per undirected pair, and a
/// self-connection (equal ids) is dropped entirely.
#[test]
fn only_the_higher_id_of_each_pair_emits_a_net() {
    let children = vec![
        (1usize, vec![(2usize, 3.0f32)]),
        (2usize, vec![(1usize, 3.0f32), (2usize, 9.0f32)]),
    ];
    let nets = build_bundled_nets(&[], &children, &|id| id);
    assert_eq!(nets.len(), 1, "one net for the pair, none for the self-connection");
    assert_eq!(nets[0], BundledNet { source: 2, target: 1, weight: 3.0 });
}

/// 🔑 **Virtual connections come FIRST and carry a fixed weight**, ahead of every child's own.
#[test]
fn virtual_connections_lead_and_carry_their_own_weight() {
    let children = vec![(5usize, vec![(1usize, 2.0f32)])];
    let nets = build_bundled_nets(&[(3, 4)], &children, &|id| id);
    assert_eq!(nets.len(), 2);
    assert_eq!(nets[0], BundledNet { source: 3, target: 4, weight: VIRTUAL_CONNECTION_WEIGHT });
    assert_eq!(nets[0].weight, 10.0);
    assert_eq!(nets[1].source, 5, "the child's net follows");
}

/// ⚠️ The centre truncates, so an odd extent rounds down.
#[test]
fn the_pin_centre_truncates() {
    assert_eq!(pin_center(0, 100), 50);
    assert_eq!(pin_center(0, 101), 50, "not 51");
    assert_eq!(pin_center(10, 5), 12, "10 + 2.5 truncates to 12");
}

/// ⚠️ **A vertical edge clamps in Y; a horizontal one clamps in X.** Using the wrong axis returns
/// a point on the line but at the wrong end.
#[test]
fn the_nearest_point_clamps_along_the_edges_own_axis() {
    let left = Region { x0: 0, y0: 100, x1: 0, y1: 300, boundary: Boundary::L };
    assert_eq!(nearest_point_in_region(&left, (50, 200)), (0, 200), "projected onto the edge");
    assert_eq!(nearest_point_in_region(&left, (50, 400)), (0, 300), "clamped to the top end");
    assert_eq!(nearest_point_in_region(&left, (50, 10)), (0, 100), "clamped to the bottom end");

    let bottom = Region { x0: 100, y0: 0, x1: 300, y1: 0, boundary: Boundary::B };
    assert_eq!(nearest_point_in_region(&bottom, (200, 50)), (200, 0));
    assert_eq!(nearest_point_in_region(&bottom, (400, 50)), (300, 0));
}

/// ⚠️ A target level with an endpoint snaps to it rather than projecting.
#[test]
fn a_target_level_with_an_endpoint_snaps_to_it() {
    let left = Region { x0: 0, y0: 100, x1: 0, y1: 300, boundary: Boundary::L };
    assert_eq!(nearest_point_in_region(&left, (50, 300)), (0, 300));
    assert_eq!(nearest_point_in_region(&left, (50, 100)), (0, 100));
}

/// ⚠️ **Minimised on squared distance, rooted once, then truncated.**
#[test]
fn the_distance_is_rooted_once_and_truncated() {
    let near = Region { x0: 0, y0: 0, x1: 0, y1: 100, boundary: Boundary::L };
    let far = Region { x0: 1000, y0: 0, x1: 1000, y1: 100, boundary: Boundary::R };
    // From (3, 50): 3 to the left edge, 997 to the right one.
    assert_eq!(dist_to_nearest_region((3, 50), &[near, far]), Some(3));
    assert_eq!(dist_to_nearest_region((3, 50), &[far]), Some(997), "the far one alone");
    // A diagonal: from (3, 150) to the left edge's top end (0, 100) is sqrt(9 + 2500) = 50.08 -> 50.
    assert_eq!(dist_to_nearest_region((3, 150), &[near]), Some(50));
    assert_eq!(dist_to_nearest_region((0, 0), &[]), None, "no regions at all");
}

/// ⚠️ **The PIN is compared against the outline's DIMENSIONS**, and it is `>` — so a pin exactly
/// on the boundary is inside.
#[test]
fn a_pin_on_the_boundary_counts_as_inside() {
    let outline = (1000, 800);
    // A macro whose centre lands exactly on each limit.
    // Centre lands exactly on (1000, 800): on the limit, so inside.
    assert!(!is_outside_the_outline(&macro_at(900, 700, 200, 200), outline));
    assert!(is_outside_the_outline(&macro_at(901, 700, 200, 200), outline), "one past is outside");
}

/// 🔑 **Normalised by the total weight AND the outline's semi-perimeter**, so the result is a
/// fraction rather than a distance.
#[test]
fn the_wirelength_is_normalised_twice() {
    let macros = vec![macro_at(0, 0, 100, 100), macro_at(900, 0, 100, 100)];
    let nets = vec![BundledNet { source: 0, target: 1, weight: 2.0 }];
    let outline = (1000, 1000);
    let got = compute_nets_wire_length(&nets, &nets, &macros, outline, 4000, &[], &|_| None);
    // Centres are (50, 50) and (950, 50): half-perimeter 900, weight 2, sum 2, semi-perimeter 2000.
    assert_eq!(got, (2.0 * 900.0) / 2.0 / 2000.0);
}

/// ⚠️ A zero total weight returns zero rather than dividing by it.
#[test]
fn a_zero_weight_sum_gives_zero_rather_than_a_division() {
    let macros = vec![macro_at(0, 0, 100, 100), macro_at(900, 0, 100, 100)];
    let nets = vec![BundledNet { source: 0, target: 1, weight: 0.0 }];
    let got = compute_nets_wire_length(&nets, &nets, &macros, (1000, 1000), 4000, &[], &|_| None);
    assert_eq!(got, 0.0);
    assert!(got.is_finite());
}

/// ⛔ **The weight sum comes from a SEPARATE list.** Upstream sums over its member while measuring
/// the argument — one character apart — and its only caller passes the same list, so the two agree
/// today. This pins that the two are genuinely distinct inputs.
#[test]
fn the_weight_sum_is_taken_from_its_own_list() {
    let macros = vec![macro_at(0, 0, 100, 100), macro_at(900, 0, 100, 100)];
    let measured = vec![BundledNet { source: 0, target: 1, weight: 2.0 }];
    let summed = vec![BundledNet { source: 0, target: 1, weight: 8.0 }];
    let got = compute_nets_wire_length(&measured, &summed, &macros, (1000, 1000), 4000, &[], &|_| None);
    assert_eq!(got, (2.0 * 900.0) / 8.0 / 2000.0, "measured with one, divided by the other");
}

/// 🔑 **A macro outside the outline is charged the whole die**, which dominates any refinement.
///
/// ⚠️ **The parameter is `Rect::margin()` — `2*dx + 2*dy` — and the function halves it.** So a
/// 1000 x 1000 die is passed `4000` and charges `2000`. Passing the semi-perimeter instead doubles
/// every out-of-outline cost.
#[test]
fn a_macro_outside_the_outline_is_charged_the_whole_die() {
    let mut io = macro_at(0, 0, 0, 0);
    io.is_cluster_of_unplaced_io_pins = true;
    io.is_unconstrained_io_cluster = true;
    let macros = vec![macro_at(5000, 0, 100, 100), io];
    let nets = vec![BundledNet { source: 0, target: 1, weight: 3.0 }];
    let regions = [Region { x0: 0, y0: 0, x1: 0, y1: 1000, boundary: Boundary::L }];
    let got =
        compute_nets_wire_length(&nets, &nets, &macros, (1000, 1000), 4000, &regions, &|_| None);
    // 3 * (4000 / 2) = 6000, then normalised by weight 3 and semi-perimeter 2000.
    assert_eq!(got, 6000.0 / 3.0 / 2000.0);
}

/// ⛔ **The product is formed in `f32`, and that is deliberately COARSE.** A die distance runs to
/// millions of database units; `f32` carries about seven significant digits, so the charge is
/// quantised. Computing it in `f64` gives a more accurate number and a different search — this
/// pins which one upstream does.
#[test]
fn the_out_of_outline_charge_is_quantised_by_f32() {
    let mut io = macro_at(0, 0, 0, 0);
    io.is_cluster_of_unplaced_io_pins = true;
    io.is_unconstrained_io_cluster = true;
    let macros = vec![macro_at(5_000_000, 0, 100, 100), io];
    let nets = vec![BundledNet { source: 0, target: 1, weight: 1.0 }];
    let regions = [Region { x0: 0, y0: 0, x1: 0, y1: 1000, boundary: Boundary::L }];

    // 2^24 + 1 is the first integer an f32 cannot represent; the charge rounds down to 2^24.
    let margin = 2 * (16_777_217i64);
    let got = wirelength_for_unplaced_io_pins(
        &macros[0],
        &macros[1],
        1.0,
        (1000, 1000),
        margin,
        &regions,
        None,
    );
    assert_eq!(got, 16_777_216, "quantised, not 16_777_217");
    assert_ne!(got, 16_777_217, "which is what an f64 product would have given");
    let _ = nets;
}

/// ⛔ **`max_dist` is an `int64_t` narrowed to `int`**, so a die margin past 2^32 wraps rather than
/// saturating. Upstream's own narrowing; reproduced, not widened.
#[test]
fn a_die_margin_past_the_int_range_wraps() {
    let mut io = macro_at(0, 0, 0, 0);
    io.is_cluster_of_unplaced_io_pins = true;
    io.is_unconstrained_io_cluster = true;
    let outside = macro_at(5000, 0, 100, 100);
    let regions = [Region { x0: 0, y0: 0, x1: 0, y1: 1000, boundary: Boundary::L }];

    // Half of this is 2^31, one past the largest `int`, so it wraps to the most negative one.
    let margin = 2i64 * 2_147_483_648;
    let got =
        wirelength_for_unplaced_io_pins(&outside, &io, 1.0, (1000, 1000), margin, &regions, None);
    assert_eq!(got, -2_147_483_648, "wrapped negative, exactly as the narrowing does");
}

/// ⚠️ **Only the TARGET is tested.** With the IO cluster as the SOURCE the ordinary half-perimeter
/// path is taken instead, which is a different number.
#[test]
fn only_the_target_takes_the_io_path() {
    let mut io = macro_at(0, 500, 0, 0);
    io.is_cluster_of_unplaced_io_pins = true;
    io.is_unconstrained_io_cluster = true;
    let placed = macro_at(400, 400, 100, 100);
    let outline = (1000, 1000);
    let regions = [Region { x0: 0, y0: 0, x1: 0, y1: 1000, boundary: Boundary::L }];

    let as_target = compute_nets_wire_length(
        &[BundledNet { source: 0, target: 1, weight: 1.0 }],
        &[BundledNet { source: 0, target: 1, weight: 1.0 }],
        &[placed, io],
        outline, 4000, &regions, &|_| None,
    );
    let as_source = compute_nets_wire_length(
        &[BundledNet { source: 0, target: 1, weight: 1.0 }],
        &[BundledNet { source: 0, target: 1, weight: 1.0 }],
        &[io, placed],
        outline, 4000, &regions, &|_| None,
    );
    assert_ne!(as_target, as_source, "the model is asymmetric in the pair");
}

// ---------------------------------------------------------------- the placement penalties

use vyges_mpl::placement::{guidance_penalty, soft_blockage_penalty, BlockageMacro};

fn blockage_macro(x: i32, y: i32, w: i32, h: i32, num_macro: i32, macro_area: i64, area: i64) -> BlockageMacro {
    BlockageMacro {
        x,
        y,
        width: w,
        height: h,
        num_macro,
        cluster_macro_area: macro_area,
        cluster_area: area,
    }
}

/// 🔑 **A cluster that is mostly standard cells costs little; one that is all macros costs the
/// full overlap.** That ratio is the whole point of the term.
#[test]
fn the_soft_blockage_cost_scales_with_macro_dominance() {
    let blockage = [(0, 0, 100, 100)];
    let all_macro = [blockage_macro(0, 0, 100, 100, 1, 1000, 1000)];
    let half_macro = [blockage_macro(0, 0, 100, 100, 1, 500, 1000)];

    let full = soft_blockage_penalty(&all_macro, &[0], &blockage, 50.0);
    let half = soft_blockage_penalty(&half_macro, &[0], &blockage, 50.0);
    assert_eq!(full, 10_000.0, "the whole overlap");
    assert_eq!(half, 5_000.0, "half of it");
}

/// ⚠️ A macro holding no macros, or a cluster of zero area, contributes nothing — the second guard
/// also stops the dominance ratio dividing by zero.
#[test]
fn a_cluster_with_no_macros_or_no_area_is_skipped() {
    let blockage = [(0, 0, 100, 100)];
    let no_macros = [blockage_macro(0, 0, 100, 100, 0, 1000, 1000)];
    assert_eq!(soft_blockage_penalty(&no_macros, &[0], &blockage, 50.0), 0.0);

    // A macro count keeps the normalisation alive while the zero-area cluster is skipped.
    let zero_area = [
        blockage_macro(0, 0, 100, 100, 1, 0, 0),
        blockage_macro(500, 500, 10, 10, 1, 10, 10),
    ];
    let got = soft_blockage_penalty(&zero_area, &[0, 1], &blockage, 50.0);
    assert_eq!(got, 0.0, "the overlapping one has no area, the other does not overlap");
}

/// ⛔ **A diagonal miss has a POSITIVE area product** — both dimensions negative. The guard is
/// what stops it being counted as an overlap.
#[test]
fn a_diagonally_missing_blockage_is_not_counted() {
    let blockage = [(0, 0, 10, 10)];
    let far = [blockage_macro(100, 100, 10, 10, 1, 100, 100)];
    assert_eq!(soft_blockage_penalty(&far, &[0], &blockage, 50.0), 0.0);
}

/// ⚠️ Normalised by the TOTAL macro count across the sequence, not by the blockage count.
#[test]
fn the_soft_blockage_penalty_divides_by_the_macro_count() {
    let blockage = [(0, 0, 100, 100)];
    let one = [blockage_macro(0, 0, 100, 100, 1, 100, 100)];
    let two = [
        blockage_macro(0, 0, 100, 100, 1, 100, 100),
        blockage_macro(900, 900, 10, 10, 1, 100, 100),
    ];
    let a = soft_blockage_penalty(&one, &[0], &blockage, 50.0);
    let b = soft_blockage_penalty(&two, &[0, 1], &blockage, 50.0);
    assert_eq!(b, a / 2.0, "the second macro doubles the divisor without adding overlap");
}

/// ℹ️ No blockages, or a zero weight, and the term is dark.
#[test]
fn the_soft_blockage_term_is_dark_without_blockages_or_weight() {
    let m = [blockage_macro(0, 0, 100, 100, 1, 100, 100)];
    assert_eq!(soft_blockage_penalty(&m, &[0], &[], 50.0), 0.0);
    assert_eq!(soft_blockage_penalty(&m, &[0], &[(0, 0, 100, 100)], 0.0), 0.0);
}

/// 🔑 **A macro wholly inside its guide scores ZERO** — the penalty is the shortfall from the best
/// possible overlap, not the overlap.
#[test]
fn a_macro_inside_its_guide_costs_nothing() {
    let macros = [macro_at(100, 100, 50, 50)];
    let guides = [(0usize, (0, 0, 1000, 1000))];
    assert_eq!(guidance_penalty(&guides, &macros, 10.0, 2000), 0.0);
}

/// ⚠️ **A macro entirely outside its guide pays the FULL best-possible overlap**, which is bounded
/// by the smaller of the two extents on each axis — not by the guide's area.
#[test]
fn a_macro_outside_its_guide_pays_the_best_possible_overlap() {
    // A 50 x 50 macro far from a 1000 x 1000 guide: best possible is 50 x 50 = 2500 dbu².
    let macros = [macro_at(5000, 5000, 50, 50)];
    let guides = [(0usize, (0, 0, 1000, 1000))];
    let got = guidance_penalty(&guides, &macros, 10.0, 10);
    assert_eq!(got, 2500.0 / 100.0, "2500 dbu² at 10 dbu per micron is 25 µm²");
}

/// ⚠️ Partial overlap pays the difference.
#[test]
fn a_partly_overlapping_macro_pays_the_shortfall() {
    // A 100 x 100 macro half inside a 1000 x 1000 guide: best 10000, actual 5000, shortfall 5000.
    let macros = [macro_at(-50, 100, 100, 100)];
    let guides = [(0usize, (0, 0, 1000, 1000))];
    let got = guidance_penalty(&guides, &macros, 10.0, 10);
    assert_eq!(got, 5000.0 / 100.0);
}

/// ℹ️ Averaged over the guides, and dark without weight or guides.
#[test]
fn the_guidance_penalty_averages_over_its_guides() {
    let macros = [macro_at(5000, 5000, 50, 50), macro_at(100, 100, 50, 50)];
    let guides = [(0usize, (0, 0, 1000, 1000)), (1usize, (0, 0, 1000, 1000))];
    // One pays 2500, the other zero, so the mean is 1250 dbu².
    assert_eq!(guidance_penalty(&guides, &macros, 10.0, 10), 1250.0 / 100.0);
    assert_eq!(guidance_penalty(&[], &macros, 10.0, 10), 0.0);
    assert_eq!(guidance_penalty(&guides, &macros, 0.0, 10), 0.0);
}

use vyges_mpl::placement::{boundary_penalty, BoundaryMacro, Root};

fn bm(x: i32, y: i32, w: i32, h: i32, num_macro: i32) -> BoundaryMacro {
    BoundaryMacro { x, y, width: w, height: h, fixed: false, num_macro }
}

/// The root as most of these fixtures see it: a 1000 x 1000 die at the origin.
fn root() -> Root {
    Root { x: 0, y: 0, width: 1000, height: 1000 }
}

/// 🔑 **A macro on a die edge costs nothing; one in the middle costs the most.** That is the whole
/// intent of the term — push macro clusters out to the boundary.
#[test]
fn a_macro_on_the_boundary_costs_nothing_and_the_centre_costs_most() {
    let corner = [bm(0, 0, 100, 100, 1)];
    assert_eq!(boundary_penalty(&corner, &[0], (0, 0), &root(), 50.0, 10), 0.0);

    // Centred: 450 to the nearer vertical edge and 450 to the nearer horizontal one.
    let centre = [bm(450, 450, 100, 100, 1)];
    let got = boundary_penalty(&centre, &[0], (0, 0), &root(), 50.0, 10);
    assert_eq!(got, 90.0, "900 dbu at 10 dbu per micron");
}

/// ⚠️ **Only ONE axis needs to be satisfied per direction.** A macro hugging the left edge but
/// vertically central still pays the vertical distance.
///
/// ⚠️ The macro is deliberately OFF-CENTRE in y. Centred, the near and far distances are equal and
/// the fixture cannot tell `min` from `max` — a mutation proved exactly that.
#[test]
fn hugging_one_edge_only_forgives_one_axis() {
    // ly = 300, uy = 400: 300 to the bottom edge and 600 to the top, so only `min` gives 300.
    let macros = [bm(0, 300, 100, 100, 1)];
    let got = boundary_penalty(&macros, &[0], (0, 0), &root(), 50.0, 10);
    assert_eq!(got, 30.0, "x costs nothing, y costs the NEARER of 300 and 600");
}

/// ⛔ **The left and right sides are NOT symmetric.** Past the LEFT edge the raw coordinate goes
/// negative and *reduces* the penalty; past the RIGHT edge `abs` makes the same overhang
/// *increase* it. Making both `abs` would be more sensible and would not be the reference.
#[test]
fn overhanging_left_and_right_are_scored_differently() {
    // 100 wide, hanging 200 past the left edge: global_lx = -200, right distance = |1000 - -100|.
    let left = [bm(-200, 0, 100, 100, 1)];
    let got_left = boundary_penalty(&left, &[0], (0, 0), &root(), 50.0, 10);
    assert_eq!(got_left, -20.0, "NEGATIVE, and the y term is zero");

    // The mirror image, hanging 200 past the right edge: global_lx = 1100, |1000 - 1200| = 200.
    let right = [bm(1100, 0, 100, 100, 1)];
    let got_right = boundary_penalty(&right, &[0], (0, 0), &root(), 50.0, 10);
    assert_eq!(got_right, 20.0, "POSITIVE for the same overhang on the other side");
}

/// ⚠️ **Coordinates are rebased onto the root twice over** — the outline's origin is added and the
/// root's is subtracted. A cluster deep inside the die measures from the DIE's edges, not its
/// parent's.
#[test]
fn the_distance_is_measured_from_the_root_not_the_outline() {
    // A macro at the origin of an outline that itself sits 400 into the die is 400 from the edge.
    let macros = [bm(0, 0, 100, 100, 1)];
    let got = boundary_penalty(&macros, &[0], (400, 400), &root(), 50.0, 10);
    assert_eq!(got, 80.0, "400 + 400 dbu");

    // A root that does not start at the origin subtracts its own corner back out.
    let offset_root = Root { x: 400, y: 400, width: 1000, height: 1000 };
    let got = boundary_penalty(&macros, &[0], (400, 400), &offset_root, 50.0, 10);
    assert_eq!(got, 0.0, "the macro is on the root's corner after all");
}

/// ⚠️ **A macro-weighted average.** A cluster of ten pulls ten times as hard as a cluster of one,
/// in the numerator AND in the divisor.
#[test]
fn the_penalty_is_averaged_over_hard_macros_not_clusters() {
    // One central cluster of 1 macro and one cornered cluster of 9: 900 dbu * 1 over 10 macros.
    let macros = [bm(450, 450, 100, 100, 1), bm(0, 0, 100, 100, 9)];
    let got = boundary_penalty(&macros, &[0, 1], (0, 0), &root(), 50.0, 10);
    assert_eq!(got, 9.0, "90 µm of cost spread over ten macros");

    // Swap the counts and the same geometry costs nine times as much.
    let heavy_centre = [bm(450, 450, 100, 100, 9), bm(0, 0, 100, 100, 1)];
    let got = boundary_penalty(&heavy_centre, &[0, 1], (0, 0), &root(), 50.0, 10);
    assert_eq!(got, 81.0);
}

/// ⚠️ **A standard-cell cluster is invisible to this term** — zero macros contributes nothing to
/// the sum and nothing to the divisor either.
#[test]
fn a_cluster_with_no_hard_macros_neither_pays_nor_dilutes() {
    let alone = [bm(450, 450, 100, 100, 1)];
    let with_cells = [bm(450, 450, 100, 100, 1), bm(0, 0, 500, 500, 0)];
    let a = boundary_penalty(&alone, &[0], (0, 0), &root(), 50.0, 10);
    let b = boundary_penalty(&with_cells, &[0, 1], (0, 0), &root(), 50.0, 10);
    assert_eq!(a, b, "the cell cluster changed neither side of the average");
}

/// ⚠️ **A FIXED macro is skipped in both passes**, so it neither pays nor dilutes — unlike a
/// zero-macro cluster, which is skipped only in the numerator.
#[test]
fn a_fixed_macro_is_skipped_entirely() {
    let mut fixed_centre = bm(450, 450, 100, 100, 4);
    fixed_centre.fixed = true;
    let macros = [fixed_centre, bm(450, 450, 100, 100, 1)];
    let got = boundary_penalty(&macros, &[0, 1], (0, 0), &root(), 50.0, 10);
    assert_eq!(got, 90.0, "only the movable macro counts, and it is not divided by five");
}

/// ⚠️ A sequence with nothing movable in it returns zero rather than dividing by zero.
#[test]
fn nothing_movable_returns_zero() {
    let mut fixed = bm(450, 450, 100, 100, 4);
    fixed.fixed = true;
    assert_eq!(boundary_penalty(&[fixed], &[0], (0, 0), &root(), 50.0, 10), 0.0);
    assert_eq!(boundary_penalty(&[], &[], (0, 0), &root(), 50.0, 10), 0.0);
    // Movable, but no hard macros anywhere: the divisor is still zero.
    let cells = [bm(450, 450, 100, 100, 0)];
    assert_eq!(boundary_penalty(&cells, &[0], (0, 0), &root(), 50.0, 10), 0.0);
}

/// ℹ️ A zero weight leaves the term dark — which is why coarse shaping never sees it.
#[test]
fn the_boundary_term_is_dark_without_weight() {
    let macros = [bm(450, 450, 100, 100, 1)];
    assert_eq!(boundary_penalty(&macros, &[0], (0, 0), &root(), 0.0, 10), 0.0);
}

/// ⚠️ **The sequence's ORDER is the accumulation order**, and the sum is `f32`. Reversing it is a
/// different number in the last bits whenever the addends do not fit exactly.
#[test]
fn the_accumulation_follows_the_sequence_order() {
    // Distances chosen so the micron values are not representable exactly in binary.
    let macros = [bm(301, 0, 1, 1, 1), bm(0, 307, 1, 1, 1), bm(311, 0, 1, 1, 1)];
    let forward = boundary_penalty(&macros, &[0, 1, 2], (0, 0), &root(), 50.0, 3);
    let reverse = boundary_penalty(&macros, &[2, 1, 0], (0, 0), &root(), 50.0, 3);
    // They are close, and the point is that the code commits to one of them.
    assert!((forward - reverse).abs() < 1e-3, "same geometry either way");
    assert_eq!(forward, boundary_penalty(&macros, &[0, 1, 2], (0, 0), &root(), 50.0, 3));
}

// ---------------------------------------------------------------- the fence penalty

use vyges_mpl::placement::fence_penalty;

/// 🔑 **A macro anywhere inside its fence scores zero** — the term measures the distance to
/// having no violation, not the distance to the fence's centre.
#[test]
fn a_macro_inside_its_fence_costs_nothing() {
    let fence = [(0usize, (0, 0, 1000, 1000))];
    // Centred, and hard against each corner of the fence in turn.
    for origin in [(450, 450), (0, 0), (900, 900), (0, 900)] {
        let macros = [macro_at(origin.0, origin.1, 100, 100)];
        assert_eq!(fence_penalty(&fence, &macros, (2000, 2000), 10.0), 0.0, "{origin:?}");
    }
}

/// ⚠️ **The overshoot is a fraction of the OUTLINE, squared** — and it is the ratio that is
/// squared, not the distance.
#[test]
fn a_macro_outside_its_fence_pays_the_squared_overshoot() {
    // A 100-wide macro in a 1000-wide fence has 450 of slack; put its centre 700 from the fence
    // centre and the overshoot is 250, which is an exact eighth of a 2000-wide outline.
    let fence = [(0usize, (0, 0, 1000, 1000))];
    let macros = [macro_at(1150, 450, 100, 100)];
    let got = fence_penalty(&fence, &macros, (2000, 2000), 10.0);
    assert_eq!(got, 0.015_625, "0.125 squared, and no y term");

    // Both axes overshoot, and the two squares add.
    let macros = [macro_at(1150, 1150, 100, 100)];
    let got = fence_penalty(&fence, &macros, (2000, 2000), 10.0);
    assert_eq!(got, 0.031_25);
}

/// ⚠️ **`<=`, so a macro exactly at the limit of its slack scores zero.**
#[test]
fn a_macro_exactly_at_its_slack_limit_costs_nothing() {
    let fence = [(0usize, (0, 0, 1000, 1000))];
    // Slack is 450; a centre exactly 450 away puts the macro's edge on the fence's.
    let macros = [macro_at(900, 450, 100, 100)];
    assert_eq!(fence_penalty(&fence, &macros, (2000, 2000), 10.0), 0.0);

    let macros = [macro_at(901, 450, 100, 100)];
    assert!(fence_penalty(&fence, &macros, (2000, 2000), 10.0) > 0.0, "one unit further out");
}

/// ⚠️ **A fence smaller than the macro it constrains is treated as unsatisfiable and skipped**,
/// not as infinitely violated.
#[test]
fn a_fence_the_macro_cannot_fit_is_skipped() {
    let fence = [(0usize, (0, 0, 50, 1000))];
    let macros = [macro_at(5000, 5000, 100, 100)];
    assert_eq!(fence_penalty(&fence, &macros, (2000, 2000), 10.0), 0.0, "far away, and skipped");
}

/// ⚠️ A zero-area macro is skipped too.
#[test]
fn a_zero_area_macro_is_skipped() {
    let fence = [(0usize, (0, 0, 1000, 1000))];
    let macros = [macro_at(5000, 5000, 0, 100)];
    assert_eq!(fence_penalty(&fence, &macros, (2000, 2000), 10.0), 0.0);
}

/// ⛔ **A skipped fence still counts towards the divisor**, so adding an unsatisfiable one dilutes
/// the whole term rather than being ignored.
#[test]
fn a_skipped_fence_still_dilutes_the_mean() {
    let macros = [macro_at(1150, 450, 100, 100), macro_at(5000, 5000, 100, 100)];
    let one = [(0usize, (0, 0, 1000, 1000))];
    let alone = fence_penalty(&one, &macros, (2000, 2000), 10.0);

    // The second fence is far too small for its macro, so it scores nothing — and halves the mean.
    let with_unsatisfiable = [(0usize, (0, 0, 1000, 1000)), (1usize, (0, 0, 10, 10))];
    let diluted = fence_penalty(&with_unsatisfiable, &macros, (2000, 2000), 10.0);
    assert_eq!(diluted, alone / 2.0);
}

/// ℹ️ No fences, or a zero weight, and the term is dark — which is every design in the suite,
/// since none of them declares a fence.
#[test]
fn the_fence_term_is_dark_without_fences_or_weight() {
    let macros = [macro_at(5000, 5000, 100, 100)];
    assert_eq!(fence_penalty(&[], &macros, (2000, 2000), 10.0), 0.0);
    let fence = [(0usize, (0, 0, 1000, 1000))];
    assert_eq!(fence_penalty(&fence, &macros, (2000, 2000), 0.0), 0.0);
}

/// ⛔ **The zero-area test WRAPS at 2^32.** It is the one place upstream multiplies two `int`
/// dimensions directly rather than going through `Rect::area()`, which widens first — so a macro
/// whose width times height is a multiple of 2^32 is treated as zero-area and skipped entirely.
///
/// ⚠️ **65536 x 65536 database units is 32.8 µm square at 2000 units per micron.** That is an
/// ordinary macro, not a pathological one, so this is reachable on a real design rather than a
/// curiosity — which is exactly why it is pinned instead of quietly widened.
#[test]
fn a_macro_whose_area_wraps_to_zero_is_skipped() {
    let fence = [(0usize, (0, 0, 1_000_000, 1_000_000))];

    // 65536 * 65536 = 2^32, which wraps to 0 in 32 bits.
    let wrapping = [macro_at(5_000_000, 5_000_000, 65_536, 65_536)];
    assert_eq!(
        fence_penalty(&fence, &wrapping, (2_000_000, 2_000_000), 10.0),
        0.0,
        "skipped as zero-area, though it is 32.8 µm square"
    );

    // One unit narrower does not wrap, and the same macro is scored.
    let ordinary = [macro_at(5_000_000, 5_000_000, 65_535, 65_536)];
    assert!(
        fence_penalty(&fence, &ordinary, (2_000_000, 2_000_000), 10.0) > 0.0,
        "one unit narrower and it is scored"
    );
}

/// ⛔ **The rounding rule is not academic: it changes a real penalty.** Upstream's
/// `guidance_penalty_ += dbuAreaToMicrons(...)` is a `float` taking a `double`, so the sum is
/// formed in `f64` and rounded once. Narrowing each addend to `f32` first rounds twice, and this
/// fixture — two guides whose shortfalls are `1.0` µm² and `2^-24 + 2^-50` µm² — lands on the
/// exact bit pattern where the two disagree.
///
/// 🔑 It takes a contrived fixture to SHOW, and no contrivance at all to HAPPEN: any long
/// accumulation of areas eventually meets one of these patterns, and a one-ulp difference in a
/// penalty redirects an annealing trajectory.
#[test]
fn the_guidance_sum_is_formed_in_f64_and_rounded_once() {
    let d: i32 = 1 << 25;
    // Both macros sit far outside their guides, so each pays its whole best-possible overlap.
    let macros = [
        macro_at(1 << 28, 1 << 28, 1 << 25, 1 << 25),
        macro_at(1 << 28, 1 << 28, (1 << 26) + 1, 1),
    ];
    let guides = [
        (0usize, (0, 0, 1 << 26, 1 << 26)),
        (1usize, (0, 0, 1 << 27, 1 << 27)),
    ];
    let got = guidance_penalty(&guides, &macros, 10.0, d);

    // 1.0 + (2^-24 + 2^-50), summed in f64 and rounded once, then averaged over two guides.
    assert_eq!(got.to_bits(), 0.500_000_06f32.to_bits(), "got {got:.10}");
    assert_ne!(got, 0.5, "narrowing each addend first would have given exactly a half");
}
