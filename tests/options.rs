// SPDX-License-Identifier: Apache-2.0
//! Command translation — the tests that would have caught `tap`'s `-halo_width_x` and `ppl`'s
//! `set_slots_per_section`.
//!
//! ⚠️ Each of these pins a rule read from OpenROAD's `src/mpl/src/mpl.tcl`. None needs a database.
use vyges_mpl::options::{parse_placer_args, Halo, PlacerOptions, Region};

// ---------------------------------------------------------------- defaults

#[test]
fn every_default_matches_upstreams_tcl() {
    // ⚠️ Transcribed from mpl.tcl, NOT from our own Default impl -- a test that reads the value
    // it checks proves nothing. If upstream changes a default at the next pin, this fails.
    let d = PlacerOptions::default();
    assert_eq!((d.max_num_macro, d.min_num_macro), (0, 0), "0 means auto");
    assert_eq!((d.max_num_inst, d.min_num_inst), (0, 0), "0 means auto");
    assert_eq!(d.tolerance, 0.1);
    assert_eq!(d.max_num_level, 2);
    assert_eq!(d.coarsening_ratio, 10.0);
    assert_eq!(d.large_net_threshold, 50);
    assert_eq!(d.area_weight, 0.1);
    assert_eq!(d.outline_weight, 100.0);
    assert_eq!(d.wirelength_weight, 100.0);
    assert_eq!(d.guidance_weight, 10.0);
    assert_eq!(d.fence_weight, 10.0);
    assert_eq!(d.boundary_weight, 50.0);
    assert_eq!(d.notch_weight, 50.0);
    assert_eq!(d.soft_blockage_weight, 10.0);
    assert_eq!(d.target_util, 0.25);
    assert_eq!(d.min_ar, 0.33);
    assert_eq!(d.report_directory, "hier_rtlmp");
    assert_eq!((d.fence_lx, d.fence_ly, d.fence_ux, d.fence_uy), (0.0, 0.0, 0.0, 0.0));
    assert!(!d.keep_clustering_data && !d.use_full_halo);
}

#[test]
fn no_arguments_leaves_every_default_intact() {
    assert_eq!(parse_placer_args(&[]).unwrap().options, PlacerOptions::default());
}

// ---------------------------------------------------------------- arrival

#[test]
fn every_key_actually_arrives() {
    // 🔑 THE lesson from tap and ppl: an option that fails to arrive does not error, it
    // silently does nothing. This walks every key and checks the value landed.
    let cases: Vec<(&str, &str, fn(&PlacerOptions) -> String)> = vec![
        ("-max_num_macro", "7", |o| o.max_num_macro.to_string()),
        ("-min_num_macro", "3", |o| o.min_num_macro.to_string()),
        ("-max_num_inst", "900", |o| o.max_num_inst.to_string()),
        ("-min_num_inst", "60", |o| o.min_num_inst.to_string()),
        ("-tolerance", "0.25", |o| o.tolerance.to_string()),
        ("-max_num_level", "4", |o| o.max_num_level.to_string()),
        ("-coarsening_ratio", "5.5", |o| o.coarsening_ratio.to_string()),
        ("-large_net_threshold", "20", |o| o.large_net_threshold.to_string()),
        ("-fence_lx", "1.5", |o| o.fence_lx.to_string()),
        ("-fence_ly", "2.5", |o| o.fence_ly.to_string()),
        ("-fence_ux", "3.5", |o| o.fence_ux.to_string()),
        ("-fence_uy", "4.5", |o| o.fence_uy.to_string()),
        ("-area_weight", "9.5", |o| o.area_weight.to_string()),
        ("-outline_weight", "11.5", |o| o.outline_weight.to_string()),
        ("-wirelength_weight", "12.5", |o| o.wirelength_weight.to_string()),
        ("-guidance_weight", "13.5", |o| o.guidance_weight.to_string()),
        ("-fence_weight", "14.5", |o| o.fence_weight.to_string()),
        ("-boundary_weight", "15.5", |o| o.boundary_weight.to_string()),
        ("-notch_weight", "16.5", |o| o.notch_weight.to_string()),
        ("-soft_blockage_weight", "17.5", |o| o.soft_blockage_weight.to_string()),
        ("-target_util", "0.75", |o| o.target_util.to_string()),
        ("-min_ar", "0.5", |o| o.min_ar.to_string()),
        ("-report_directory", "rpt", |o| o.report_directory.clone()),
    ];
    for (key, value, read) in cases {
        let parsed = parse_placer_args(&[key, value]).unwrap_or_else(|e| panic!("{key}: {e}"));
        assert_eq!(read(&parsed.options), value, "{key} did not arrive");
        // ...and it changed something: a key whose value equals the default proves nothing.
        assert_ne!(
            read(&parsed.options),
            read(&PlacerOptions::default()),
            "{key}'s test value must differ from the default, or the test is inert"
        );
    }
}

#[test]
fn both_flags_arrive() {
    let p = parse_placer_args(&["-keep_clustering_data", "-use_full_halo"]).unwrap();
    assert!(p.options.keep_clustering_data && p.options.use_full_halo);
}

#[test]
fn write_macro_placement_arrives_as_a_path() {
    let p = parse_placer_args(&["-write_macro_placement", "out.txt"]).unwrap();
    assert_eq!(p.options.write_macro_placement.as_deref(), Some("out.txt"));
}

// ---------------------------------------------------------------- refusal

#[test]
fn an_unknown_key_is_an_error_not_a_shrug() {
    // ⛔ Silently ignoring this is exactly how an engine runs a different command than the
    // case asked for, and reports success.
    assert!(parse_placer_args(&["-halo_width_x", "5"]).is_err(), "tap's actual trap spelling");
    assert!(parse_placer_args(&["-signature_net_threshold", "5"]).is_err(), "an option I invented");
    assert!(parse_placer_args(&["-target_dead_space", "5"]).is_err(), "another I invented");
    assert!(parse_placer_args(&["-blockage_weight", "5"]).is_err(), "and a third");
}

#[test]
fn a_key_without_a_value_is_an_error() {
    assert!(parse_placer_args(&["-target_util"]).is_err());
}

#[test]
fn a_non_numeric_value_is_an_error_not_a_silent_default() {
    let e = parse_placer_args(&["-target_util", "banana"]).unwrap_err();
    assert!(e.message.contains("banana"), "the bad value is named: {e}");
}

// ---------------------------------------------------------------- deprecations

#[test]
fn macro_blockage_weight_aliases_soft_and_warns_mpl70() {
    let p = parse_placer_args(&["-macro_blockage_weight", "42"]).unwrap();
    assert_eq!(p.options.soft_blockage_weight, 42.0, "it aliases the modern key");
    assert!(p.warnings.iter().any(|w| w.code == 70), "MPL-0070 is emitted");
}

#[test]
fn giving_both_blockage_weights_is_mpl69() {
    let e = parse_placer_args(&[
        "-macro_blockage_weight", "42", "-soft_blockage_weight", "43",
    ])
    .unwrap_err();
    assert_eq!(e.code, 69);
    // Order must not matter.
    let e2 = parse_placer_args(&[
        "-soft_blockage_weight", "43", "-macro_blockage_weight", "42",
    ])
    .unwrap_err();
    assert_eq!(e2.code, 69);
}

#[test]
fn halo_width_alone_sets_height_to_it_and_warns_mpl74() {
    // Upstream: `set halo_height $halo_width` when only -halo_width is given, then
    // `mpl::set_base_halo $halo_width $halo_height $halo_width $halo_height`.
    let p = parse_placer_args(&["-halo_width", "5"]).unwrap();
    assert_eq!(
        p.options.base_halo_from_flags,
        Some(Halo { left: 5, bottom: 5, right: 5, top: 5 })
    );
    assert!(p.warnings.iter().any(|w| w.code == 74));
}

#[test]
fn halo_height_alone_sets_width_to_it() {
    // The mirror case, which upstream spells out separately -- and which an implementation
    // that only handled -halo_width would silently get wrong.
    let p = parse_placer_args(&["-halo_height", "9"]).unwrap();
    assert_eq!(
        p.options.base_halo_from_flags,
        Some(Halo { left: 9, bottom: 9, right: 9, top: 9 })
    );
}

#[test]
fn halo_width_and_height_together_keep_both() {
    let p = parse_placer_args(&["-halo_width", "5", "-halo_height", "9"]).unwrap();
    assert_eq!(
        p.options.base_halo_from_flags,
        Some(Halo { left: 5, bottom: 9, right: 5, top: 9 }),
        "left/right take the width, bottom/top the height"
    );
}

// ---------------------------------------------------------------- halo list

#[test]
fn a_two_value_halo_mirrors_into_four() {
    // 🔑 Order is left bottom right top. There is no -halo_x/-halo_y.
    assert_eq!(
        Halo::parse(&[3, 7]).unwrap(),
        Halo { left: 3, bottom: 7, right: 3, top: 7 }
    );
}

#[test]
fn a_four_value_halo_is_left_bottom_right_top() {
    // ⚠️ If someone reads this as (l, r, b, t) the geometry is wrong in a way that only
    // shows up on an asymmetric halo -- which is why the values here are all different.
    assert_eq!(
        Halo::parse(&[1, 2, 3, 4]).unwrap(),
        Halo { left: 1, bottom: 2, right: 3, top: 4 }
    );
}

#[test]
fn a_halo_of_any_other_length_is_mpl72() {
    for bad in [vec![], vec![1], vec![1, 2, 3], vec![1, 2, 3, 4, 5]] {
        assert_eq!(Halo::parse(&bad).unwrap_err().code, 72, "{bad:?}");
    }
}

#[test]
fn a_negative_halo_value_is_mpl73() {
    assert_eq!(Halo::parse(&[1, -2]).unwrap_err().code, 73);
    assert_eq!(Halo::parse(&[1, 2, 3, -4]).unwrap_err().code, 73, "checked in all four slots");
    assert_eq!(Halo::parse(&[0, 0]).unwrap().left, 0, "zero is allowed; only negative is not");
}

// ---------------------------------------------------------------- region

#[test]
fn a_region_is_four_values_and_must_not_be_inverted() {
    assert_eq!(Region::parse(&[1, 2, 3, 4]).unwrap(), Region { x1: 1, y1: 2, x2: 3, y2: 4 });
    assert_eq!(Region::parse(&[1, 2, 3]).unwrap_err().code, 31);
    assert_eq!(Region::parse(&[9, 2, 3, 4]).unwrap_err().code, 32, "x1 > x2");
    assert_eq!(Region::parse(&[1, 9, 3, 4]).unwrap_err().code, 33, "y1 > y2");
    // A zero-area region is legal: upstream tests `>`, not `>=`.
    assert!(Region::parse(&[5, 5, 5, 5]).is_ok());
}
