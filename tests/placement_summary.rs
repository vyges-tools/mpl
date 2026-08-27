// SPDX-License-Identifier: Apache-2.0
//! The `Cluster Placement Summary` table, byte for byte.

use vyges_mpl::anneal::{Normalization, Penalties, SoftWeights};
use vyges_mpl::placement::cluster_placement_summary;

/// 🔑 **Byte for byte, including the trailing space on every row and the blank line before the
/// header.** Both are in upstream's format strings, and a harness diffing this text sees them.
#[test]
fn the_table_matches_upstreams_format_exactly() {
    let weights = SoftWeights {
        area: 0.1,
        outline: 100.0,
        wirelength: 100.0,
        guidance: 10.0,
        fence: 10.0,
        boundary: 50.0,
        soft_blockage: 50.0,
        fixed_macros: 100.0,
        notch: 50.0,
    };
    let penalties = Penalties { outline: 0.5, wirelength: 0.25, ..Default::default() };
    let norms = Normalization { outline: 2.0, wirelength: 0.5, ..Default::default() };

    let got = cluster_placement_summary(
        0,
        (0, 0, 440_040, 442_400),
        &penalties,
        &weights,
        &norms,
        0.75,
        12.5,
        2000,
    );

    let expected = "\
Id: 0
Outline: (  0.00     0.00  ) ( 220.02   221.20 )

  Penalty Type  |  Weight  |  Value  |  Norm. Factor  |  Cost
---------------------------------------------------------------
           Area |   0.1000 |  0.7500 |         1.0000 |  0.0750 
        Outline | 100.0000 |  0.5000 |         2.0000 | 25.0000 
    Wire Length | 100.0000 |  0.2500 |         0.5000 | 50.0000 
       Guidance |  10.0000 |  0.0000 |         1.0000 |  0.0000 
          Fence |  10.0000 |  0.0000 |         1.0000 |  0.0000 
       Boundary |  50.0000 |  0.0000 |         1.0000 |  0.0000 
  Soft Blockage |  50.0000 |  0.0000 |         1.0000 |  0.0000 
          Notch |  50.0000 |  0.0000 |         1.0000 |  0.0000 
---------------------------------------------------------------
  Total Cost                                            12.5000 

";
    assert_eq!(got, expected, "\n--- got ---\n{got}");
}

/// ⚠️ **The outline is CENTRED in eight columns**, which is why a two-digit and a three-digit
/// coordinate line up differently — the reference's own capture shows exactly this spacing.
#[test]
fn the_outline_coordinates_are_centred_in_eight_columns() {
    let got = cluster_placement_summary(
        7,
        (0, 0, 440_040, 442_400),
        &Penalties::default(),
        &SoftWeights::placement_defaults(),
        &Normalization::default(),
        0.0,
        0.0,
        2000,
    );
    assert!(
        got.contains("Outline: (  0.00     0.00  ) ( 220.02   221.20 )\n"),
        "\n{got}"
    );
    assert!(got.starts_with("Id: 7\n"));
}

/// ⛔ **The Area row's normalisation factor is a HARDCODED `1.0`.** Confirmed against all 34
/// reference captures: every Area row reads `1.0000` while every other term spans `0.03` to `0.99`.
/// Changing the measured factor must not move that column.
#[test]
fn the_area_factor_is_a_constant_not_a_measurement() {
    let table = |area_norm: f32| {
        cluster_placement_summary(
            0,
            (0, 0, 2000, 2000),
            &Penalties::default(),
            &SoftWeights::placement_defaults(),
            &Normalization { area: area_norm, ..Default::default() },
            0.5,
            0.0,
            2000,
        )
    };
    assert_eq!(table(1.0), table(0.25), "the measured factor does not reach the table");
    assert!(table(0.25).contains("Area |   0.1000 |  0.5000 |         1.0000 |  0.0500 \n"));
}

/// ⛔ **The FIXED MACROS term is in the cost and NOT in the table.** Eight rows are printed; nine
/// terms are summed — so adding the Cost column up will not reach the total on a design with a
/// fixed macro.
#[test]
fn the_fixed_macros_term_is_absent_from_the_table() {
    let got = cluster_placement_summary(
        0,
        (0, 0, 2000, 2000),
        &Penalties { fixed_macros: 99.0, ..Default::default() },
        &SoftWeights::placement_defaults(),
        &Normalization::default(),
        0.0,
        0.0,
        2000,
    );
    assert!(!got.contains("Fixed"), "no such row: \n{got}");
    // ⚠️ The HEADER also contains " | ", so it has to be excluded — counting it gives nine and
    // reads as though the missing term were present.
    let rows = got.lines().filter(|l| l.contains(" | ") && !l.contains("Penalty Type")).count();
    assert_eq!(rows, 8, "eight rows, nine terms");
}

/// ⛔ **The Cost column is RECOMPUTED**, not taken from the cost function — so it does not have to
/// agree with the total, and on a zero factor it would print an infinity the cost function drops.
/// ℹ️ Unreachable in the suite: no reference capture contains an `inf` or a `nan`.
#[test]
fn the_cost_column_is_recomputed_and_can_diverge_from_the_total() {
    let got = cluster_placement_summary(
        0,
        (0, 0, 2000, 2000),
        &Penalties { outline: 1.0, ..Default::default() },
        &SoftWeights::placement_defaults(),
        &Normalization { outline: 0.0, ..Default::default() },
        0.0,
        // The cost function DROPPED the outline term, so the total is zero.
        0.0,
        2000,
    );
    assert!(got.contains("inf"), "the column prints an infinity: \n{got}");
    assert!(got.contains("  Total Cost                                             0.0000 \n"));
}
