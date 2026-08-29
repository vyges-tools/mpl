// SPDX-License-Identifier: Apache-2.0
//! Orientation correction: which macros may be flipped together, and when a flip is kept.

use vyges_mpl::placement::{
    keep_flip, orientation_groups, orientation_strategy, OrientationStrategy, FLIP_PASSES,
};

/// ⛔ **The branch reads backwards.** Pin-aware halos — `use_full_halo` FALSE — take the
/// RESTRICTED by-cluster path, because flipping a single macro inside a cluster could leave part of
/// it unreachable. A full halo has no such worry and flips each macro alone.
#[test]
fn pin_aware_halos_take_the_restricted_path() {
    assert_eq!(orientation_strategy(false), OrientationStrategy::ByCluster);
    assert_eq!(orientation_strategy(true), OrientationStrategy::Single);
}

/// ⚠️ **`>`, strictly — so a TIE KEEPS THE FLIP.** The flip happens first and is undone only when
/// it made things strictly worse.
#[test]
fn an_equal_wirelength_keeps_the_flip() {
    assert!(keep_flip(100.0, 100.0), "a tie keeps it");
    assert!(keep_flip(100.0, 99.0), "an improvement keeps it");
    assert!(!keep_flip(100.0, 100.001), "any worsening reverts it");
}

/// ⛔ **Two full passes, vertical then horizontal** — not two flips per macro. A macro's horizontal
/// trial is measured against a board on which every other macro's vertical decision has already
/// been made.
#[test]
fn the_passes_are_vertical_then_horizontal() {
    assert_eq!(FLIP_PASSES, [true, false]);
}

/// 🔑 **A macro belongs to BOTH a column and a row**, so it is flipped as part of one group in the
/// vertical pass and a different group in the horizontal one.
#[test]
fn a_macro_is_in_both_a_column_and_a_row() {
    // A 2 x 2 array: ids 0..3 at (0,0), (100,0), (0,100), (100,100).
    let macros = [(0usize, (0, 0)), (1, (100, 0)), (2, (0, 100)), (3, (100, 100))];
    let (cols, rows) = orientation_groups(&macros);
    assert_eq!(cols, vec![vec![0, 2], vec![1, 3]], "grouped by x");
    assert_eq!(rows, vec![vec![0, 1], vec![2, 3]], "grouped by y");
}

/// ⚠️ **The groups come out in ascending coordinate order**, because upstream's container is a
/// `std::map` — not in the order the macros were listed.
#[test]
fn groups_come_out_in_ascending_coordinate_order() {
    let macros = [(0usize, (500, 500)), (1, (100, 100)), (2, (300, 300))];
    let (cols, rows) = orientation_groups(&macros);
    assert_eq!(cols, vec![vec![1], vec![2], vec![0]], "x = 100, 300, 500");
    assert_eq!(rows, vec![vec![1], vec![2], vec![0]]);
}

/// ⚠️ A single macro is its own column and its own row.
#[test]
fn a_lone_macro_is_its_own_column_and_row() {
    let (cols, rows) = orientation_groups(&[(7usize, (10, 20))]);
    assert_eq!(cols, vec![vec![7]]);
    assert_eq!(rows, vec![vec![7]]);
}

/// ℹ️ A cluster with no macros produces no groups, and neither pass has anything to do.
#[test]
fn no_macros_gives_no_groups() {
    let (cols, rows) = orientation_groups(&[]);
    assert!(cols.is_empty());
    assert!(rows.is_empty());
}

/// ⚠️ **A whole row sharing one y is flipped together**, however wide — the grouping is by exact
/// coordinate, so a macro one unit off is in a group of its own.
#[test]
fn a_macro_one_unit_off_forms_its_own_group() {
    let macros = [(0usize, (0, 100)), (1, (200, 100)), (2, (400, 101))];
    let (_, rows) = orientation_groups(&macros);
    assert_eq!(rows, vec![vec![0, 1], vec![2]], "no tolerance at all");
}

// ---------------------------------------------------------------- flipping, and what it costs

use vyges_mpl::halo::Orient;
use vyges_mpl::placement::{
    flip_orientation, flip_real_macro, net_terminal_bbox, real_macro_wirelength, NetTerminal,
};

/// ⛔ **A "vertical flip" calls `flipY`** — mirroring about the vertical axis, which moves the
/// macro HORIZONTALLY. The name describes the mirror line, not the direction of travel.
#[test]
fn a_vertical_flip_mirrors_about_the_vertical_axis() {
    assert_eq!(flip_orientation(Orient::R0, true), Orient::My, "vertical flip gives MY");
    assert_eq!(flip_orientation(Orient::R0, false), Orient::Mx, "horizontal flip gives MX");
}

/// ⚠️ Each flip is its own inverse.
#[test]
fn flipping_twice_returns_to_the_start() {
    for orient in [Orient::R0, Orient::R180, Orient::Mx, Orient::My] {
        for vertical in [true, false] {
            let there = flip_orientation(orient, vertical);
            assert_eq!(flip_orientation(there, vertical), orient, "{orient:?} {vertical}");
        }
    }
}

/// ⚠️ `R180` is the half-turn, so each flip takes it to the mirror about the OTHER axis.
#[test]
fn a_half_turn_flips_to_the_other_mirror() {
    assert_eq!(flip_orientation(Orient::R180, true), Orient::Mx);
    assert_eq!(flip_orientation(Orient::R180, false), Orient::My);
}

/// 🔑 **The location is re-applied after the orientation** — mirroring moves the lower-left corner,
/// so the real location is written back to put the macro where placement wanted it.
#[test]
fn the_location_survives_the_flip() {
    let (orient, location) = flip_real_macro(Orient::R0, (1234, 5678), true);
    assert_eq!(orient, Orient::My);
    assert_eq!(location, (1234, 5678), "unchanged, and that is the point");
}

// ---------------------------------------------------------------- the real wirelength

/// ⚠️ **Every terminal is a POINT, never a box** — even a fixed block terminal contributes its
/// centre rather than its extent.
#[test]
fn a_net_box_is_the_spread_of_points_not_of_shapes() {
    let terminals = [
        NetTerminal::Instance(Some((100, 100))),
        NetTerminal::FixedPin((500, 300)),
    ];
    assert_eq!(net_terminal_bbox(&terminals), Some((100, 100, 500, 300)));
}

/// ⚠️ **A pin with no geometry yields nothing and is skipped entirely** — it does not contribute a
/// point at the origin.
#[test]
fn a_pin_without_geometry_is_skipped_not_placed_at_zero() {
    let terminals = [
        NetTerminal::Instance(None),
        NetTerminal::Instance(Some((100, 100))),
        NetTerminal::Instance(Some((200, 200))),
    ];
    assert_eq!(net_terminal_bbox(&terminals), Some((100, 100, 200, 200)), "not reaching to 0");
}

/// ⛔ **An unplaced block terminal contributes its CONSTRAINT REGION's nearest point**, not its own
/// position — the model asks where the pin could go, not where it currently is.
#[test]
fn an_unplaced_pin_contributes_its_region_not_its_position() {
    // Both are points as far as the merge is concerned; the difference is in what the caller
    // supplies, and the two kinds are deliberately distinct so the caller cannot confuse them.
    let fixed = [NetTerminal::Instance(Some((0, 0))), NetTerminal::FixedPin((900, 0))];
    let unplaced = [NetTerminal::Instance(Some((0, 0))), NetTerminal::UnplacedPin((100, 0))];
    assert_eq!(net_terminal_bbox(&fixed), Some((0, 0, 900, 0)));
    assert_eq!(net_terminal_bbox(&unplaced), Some((0, 0, 100, 0)));
}

/// ℹ️ A net with nothing on it contributes no box, and so no wirelength.
#[test]
fn an_empty_net_contributes_nothing() {
    assert_eq!(net_terminal_bbox(&[]), None);
    assert_eq!(real_macro_wirelength(&[Vec::new()]), 0.0);
}

/// ⚠️ The wirelength is the half-perimeter of each net's box, summed.
#[test]
fn the_wirelength_is_the_summed_half_perimeter() {
    let net_a = vec![NetTerminal::Instance(Some((0, 0))), NetTerminal::FixedPin((100, 200))];
    let net_b = vec![NetTerminal::Instance(Some((0, 0))), NetTerminal::FixedPin((50, 50))];
    assert_eq!(real_macro_wirelength(&[net_a, net_b]), 300.0 + 100.0);
}

/// ⛔ **A net is counted ONCE PER PIN OF THIS MACRO ON IT.** The loop walks the macro's own pins and
/// adds each pin's whole net — so two pins on one net contribute it twice. Since this only ever
/// compares two orientations of the same macro, the doubling cancels; it is still not the
/// wirelength of anything.
#[test]
fn a_net_reached_by_two_pins_is_counted_twice() {
    let net = vec![NetTerminal::Instance(Some((0, 0))), NetTerminal::FixedPin((100, 100))];
    let once = real_macro_wirelength(&[net.clone()]);
    let twice = real_macro_wirelength(&[net.clone(), net]);
    assert_eq!(once, 200.0);
    assert_eq!(twice, 400.0, "the same net, counted for each pin that reaches it");
}

// ---------------------------------------------------------------- the composed passes

use vyges_mpl::cluster::ClusterType;
use vyges_mpl::placement::{
    cluster_hard_macros, run_orientation_by_cluster, run_orientation_single, FlipCluster, FlipMacro,
};

fn flip_macro(name: &str, owner: &str, x: i32, y: i32, halo: (i32, i32)) -> FlipMacro {
    FlipMacro {
        name: name.to_string(),
        cluster_name: owner.to_string(),
        location: (x, y),
        // `getRealX` = `x_ + halo.left` at R0 — the halo comes OFF.
        real_location: (x + halo.0, y + halo.1),
    }
}

/// ⛔ **The line names the MACRO's cluster, not the one being flipped.** Upstream interpolates
/// `macros.front()->getCluster()->getName()`, and a macro's cluster is whichever HIGHEST-ID cluster
/// holds it — never the root, which has the lowest id. So the ROOT's own column group is reported
/// under the leaf cluster its first macro belongs to.
///
/// ⚠️ This is the difference between `Cluster root …` and `Cluster U1 …` on every all-macro design,
/// and nothing else in the trace reveals it.
#[test]
fn the_line_names_the_macros_own_cluster_not_the_group_being_flipped() {
    let macros = vec![flip_macro("u1", "U1", 10, 20, (0, 0))];
    let root = FlipCluster { id: 0, name: "root".into(), macros: vec![0] };
    let mut zero = |_: &[usize], _: bool| (0.0, 0.0);
    let out = run_orientation_by_cluster(&[root], &macros, &mut zero);
    assert!(out[0].contains("Cluster U1 "), "not `Cluster root`: {:?}", out[0]);
}

/// ⛔ **Two FULL passes, columns then rows** — not two flips per group.
#[test]
fn every_column_is_tried_before_any_row() {
    let macros = vec![
        flip_macro("a", "C", 0, 0, (0, 0)),
        flip_macro("b", "C", 100, 0, (0, 0)),
    ];
    let c = FlipCluster { id: 1, name: "C".into(), macros: vec![0, 1] };
    let mut zero = |_: &[usize], _: bool| (0.0, 0.0);
    let out = run_orientation_by_cluster(&[c], &macros, &mut zero);
    // Two columns (x 0 and 100), one row (both at y 0).
    assert_eq!(out.len(), 3);
    assert!(out[0].contains("column-wise (V)"));
    assert!(out[1].contains("column-wise (V)"));
    assert!(out[2].contains("row-wise (H)"), "every column first: {out:#?}");
}

/// ⛔ **The grouping key is the REAL coordinate and the reported one is the HALOED coordinate.**
/// Two macros with different halos can share a real column while reporting different `flip at`
/// values — and the group is keyed by the one the line does not print.
#[test]
fn the_group_keys_on_the_real_coordinate_and_reports_the_haloed_one() {
    // ⚠️ **The FRONT macro must be the one whose two coordinates DIFFER**, or the mutation that
    // reports the real coordinate instead of the haloed one prints the same number and is
    // invisible. `a` is haloed-90 / real-100; `b` is haloed-100 / real-100. Both share the real
    // column; only `a` distinguishes the two accessors.
    let macros = vec![
        flip_macro("a", "C", 90, 0, (10, 0)),
        flip_macro("b", "C", 100, 50, (0, 0)),
    ];
    let c = FlipCluster { id: 1, name: "C".into(), macros: vec![0, 1] };
    let mut zero = |_: &[usize], _: bool| (0.0, 0.0);
    let out = run_orientation_by_cluster(&[c], &macros, &mut zero);
    let v: Vec<&String> = out.iter().filter(|l| l.contains("column-wise")).collect();
    assert_eq!(v.len(), 1, "one shared column, keyed on the real coordinate: {out:#?}");
    assert!(v[0].contains("flip at 90"), "reports the FRONT macro's HALOED x, not its real 100: {:?}", v[0]);
}

/// ⚠️ **The single path is a different line shape entirely** — `Inst {name} flip {V|H}`, with no
/// coordinate and the macro's own name.
#[test]
fn the_single_path_prints_inst_lines_with_no_coordinate() {
    let macros = vec![flip_macro("MACRO_1", "MACRO_1", 5, 5, (0, 0))];
    let mut zero = |_: usize, _: bool| (0.0, 0.0);
    let out = run_orientation_single(&macros, &[0], &mut zero);
    assert_eq!(
        out,
        vec![
            "[DEBUG MPL-flipping] Inst MACRO_1 flip V orig_WL 0 new_WL 0",
            "[DEBUG MPL-flipping] Inst MACRO_1 flip H orig_WL 0 new_WL 0",
        ]
    );
}

/// ⚠️ **An integral wirelength prints with NO decimal point.** The value is a `float` formatted
/// with `{}`, and every captured reference line is integral — which is what an `int64_t`
/// accumulation with a late cast produces.
#[test]
fn an_integral_wirelength_prints_without_a_decimal_point() {
    let macros = vec![flip_macro("a", "C", 0, 0, (0, 0))];
    let c = FlipCluster { id: 1, name: "C".into(), macros: vec![0] };
    let mut wl = |_: &[usize], _: bool| (218770.0, 19050.0);
    let out = run_orientation_by_cluster(&[c], &macros, &mut wl);
    assert!(out[0].ends_with("orig_WL 218770 new_WL 19050"), "{:?}", out[0]);
}

// ---------------------------------------------------------------- getHardMacros

/// ⛔ **A cluster's hard macros are its leaf macros PLUS every macro under its modules**, walked
/// recursively. 🔑 This is why an all-macro ROOT holds every macro while owning no leaf: it holds
/// the top module.
#[test]
fn a_clusters_hard_macros_include_everything_under_its_modules() {
    // module 0 owns inst 1 (a macro) and inst 2 (a cell); module 1 is its child and owns macro 3.
    let insts = |m: usize| match m {
        0 => vec![1, 2],
        1 => vec![3],
        _ => vec![],
    };
    let children = |m: usize| if m == 0 { vec![1] } else { vec![] };
    let is_macro = |i: usize| i != 2;

    let got = cluster_hard_macros(ClusterType::Mixed, &[], &[0], &insts, &children, &is_macro);
    assert_eq!(got, vec![1, 3], "the cell is skipped and the child module is walked");
}

/// ⚠️ **The leaf macros come FIRST**, in their own order, and the module walk is appended. The first
/// entry is what the trace reports and what the push threshold reads.
#[test]
fn the_leaf_macros_come_before_the_module_walk() {
    let insts = |m: usize| if m == 0 { vec![9] } else { vec![] };
    let children = |_: usize| vec![];
    let got = cluster_hard_macros(ClusterType::HardMacro, &[7], &[0], &insts, &children, &|_| true);
    assert_eq!(got, vec![7, 9]);
}

/// ⛔ **A standard-cell cluster returns NOTHING**, before either source is consulted — so it never
/// claims a macro, which is what keeps `setCluster` pointing at a macro cluster.
#[test]
fn a_std_cell_cluster_has_no_hard_macros() {
    let insts = |_: usize| vec![1, 2, 3];
    let children = |_: usize| vec![];
    let got = cluster_hard_macros(ClusterType::StdCell, &[4], &[0], &insts, &children, &|_| true);
    assert!(got.is_empty(), "not even its own leaf macros");
}

// ---------------------------------------------------------------- the database transform

use vyges_mpl::placement::{iterm_avg_xy, iterm_bbox, transform_point, transform_rect, DbOrient};

/// ⛔ **Rotation is about the ORIGIN, then the offset is added** — `dbInst::getTransform()` builds
/// the transform from the instance's origin, so a rotated cell's geometry swings around (0, 0)
/// before being translated, not around its own box.
#[test]
fn the_rotation_happens_about_the_origin_before_the_offset() {
    // (10, 0) rotated 90° is (0, 10) — then translated by (100, 200).
    assert_eq!(transform_point((10, 0), DbOrient::R90, (100, 200)), (100, 210));
    // Rotating about the point's own position would leave it at (110, 200).
}

/// Every case of `dbTransform::apply(Point&)`, transcribed from `geom.h`'s rotations.
#[test]
fn every_orientation_matches_the_reference_mapping() {
    let p = (3, 5);
    let z = (0, 0);
    assert_eq!(transform_point(p, DbOrient::R0, z), (3, 5));
    assert_eq!(transform_point(p, DbOrient::R90, z), (-5, 3), "rotate90 is (x,y) -> (-y,x)");
    assert_eq!(transform_point(p, DbOrient::R180, z), (-3, -5));
    assert_eq!(transform_point(p, DbOrient::R270, z), (5, -3), "rotate270 is (x,y) -> (y,-x)");
    assert_eq!(transform_point(p, DbOrient::MY, z), (-3, 5));
    assert_eq!(transform_point(p, DbOrient::MX, z), (3, -5));
    // ⛔ Mirror FIRST, then rotate. MYR90 negates x to (-3,5), then rotate90 gives (-5,-3).
    assert_eq!(transform_point(p, DbOrient::MYR90, z), (-5, -3));
    // MXR90 negates y to (3,-5), then rotate90 gives (5,3).
    assert_eq!(transform_point(p, DbOrient::MXR90, z), (5, 3));
}

/// ⛔ **A mirrored box is RE-NORMALISED.** Both corners are transformed independently and
/// `Rect::init` re-orders them, so the result is well-formed rather than inside out.
#[test]
fn a_mirrored_box_comes_back_well_formed() {
    let r = (10, 20, 30, 40);
    let got = transform_rect(r, DbOrient::MY, (0, 0));
    assert_eq!(got, (-30, 20, -10, 40), "x mirrored and re-ordered, y untouched");
    assert!(got.2 > got.0 && got.3 > got.1, "not inside out");
}

/// ⛔ **`getAvgXY` is NOT the centre of the bounding box.** Every box contributes both corners and
/// the divisor is `2 x boxes`, so a terminal split across several boxes is weighted by box COUNT.
///
/// 🔑 Two small boxes at the ends and one box spanning them have the same bbox and different
/// averages — which is the whole reason this is a separate accessor.
#[test]
fn the_average_weights_by_box_count_not_by_extent() {
    // Two boxes: one tiny at x 0..2, one wide at x 10..30. Bounding box is 0..30, centre 15.
    let boxes = [(0, 0, 2, 2), (10, 0, 30, 2)];
    let avg = iterm_avg_xy(&boxes, DbOrient::R0, (0, 0)).unwrap();
    assert_eq!(avg.0, 10, "(0+2+10+30)/4 = 10, not the bbox centre of 15");

    let bbox = iterm_bbox(&boxes, DbOrient::R0, (0, 0)).unwrap();
    assert_eq!(bbox, (0, 0, 30, 2));
    assert_ne!(avg.0, (bbox.0 + bbox.2) / 2, "the two accessors genuinely disagree");
}

/// ⚠️ **The average truncates toward zero**, because upstream ends in `int(xx)` on a `double`.
#[test]
fn the_average_truncates_toward_zero() {
    // Sum 0+1 = 1 over 2 -> 0.5 -> 0.
    assert_eq!(iterm_avg_xy(&[(0, 0, 1, 1)], DbOrient::R0, (0, 0)).unwrap(), (0, 0));
    // Negative: -1 + 0 = -1 over 2 -> -0.5 -> 0, NOT -1. Truncation, not flooring.
    assert_eq!(iterm_avg_xy(&[(-1, -1, 0, 0)], DbOrient::R0, (0, 0)).unwrap(), (0, 0));
}

/// ⚠️ **A terminal with no geometry has no position at all** — upstream warns (ODB-34) and returns
/// false, and the caller then merges NOTHING rather than merging the origin.
#[test]
fn a_terminal_without_geometry_has_no_position() {
    assert_eq!(iterm_avg_xy(&[], DbOrient::R0, (100, 100)), None);
    assert_eq!(iterm_bbox(&[], DbOrient::R0, (100, 100)), None);
}

/// ⚠️ The DEF spellings, where the "flipped" names describe the mirror axis rather than a facing.
#[test]
fn the_def_orientation_names_map_as_the_format_defines_them() {
    assert_eq!(DbOrient::from_def("N"), Some(DbOrient::R0));
    assert_eq!(DbOrient::from_def("W"), Some(DbOrient::R90), "io_pads1 places a pad at W");
    assert_eq!(DbOrient::from_def("FS"), Some(DbOrient::MX));
    assert_eq!(DbOrient::from_def("FN"), Some(DbOrient::MY));
    assert_eq!(DbOrient::from_def("nonsense"), None);
}
