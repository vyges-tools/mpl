// SPDX-License-Identifier: Apache-2.0
//! The physical-hierarchy dump — **our oracle's format**.
//!
//! Upstream `ClusteringEngine::printPhysicalHierarchyTree`, reproduced byte for byte.
//!
//! 🔑 **This is the only place a whole clustering run can be compared against upstream's.** The
//! threshold table compared four numbers; this compares the entire tree — every cluster's name,
//! id, type, leaf-ness, counts and areas, in traversal order. It is by far the strongest check
//! this engine has, so the format has to match exactly rather than merely convey the same facts.

use crate::cluster::Cluster;

/// One line per cluster, depth-first, parents before children.
pub fn physical_hierarchy(root: &Cluster) -> String {
    let mut out = String::new();
    write_cluster(root, 0, &mut out);
    out
}

/// The number of IO pins a bundle holds. Upstream reads this from the design; the dump only
/// needs the count, so the caller supplies it.
fn write_cluster(cluster: &Cluster, level: usize, out: &mut String) {
    // ⚠️ `+---` per level, and TWO spaces before the id — both are load-bearing for a byte
    // comparison, and neither is visible in a casual reading of the output.
    for _ in 0..level {
        out.push_str("+---");
    }
    out.push_str(&format!(
        "{}  ({}) Type: {}",
        cluster.name,
        cluster.id,
        cluster.type_string()
    ));

    if cluster.is_cluster_of_unplaced_io_pins || cluster.is_io_bundle {
        // Pin-carrying clusters print a pin count and nothing else.
        out.push_str(&format!(" Pins: {}", cluster.num_io_pins));
    } else if !cluster.is_io_pad_cluster {
        // ⚠️ A leading space even when the leaf string is EMPTY, which is why a non-leaf reads
        // `Type: Mixed ,` with a space before the comma. Trimming it breaks the comparison.
        out.push_str(&format!(" {}", cluster.is_leaf_string()));

        // 🔑 `count != 0 || area != 0` — upstream's comment says the `or` is deliberate, "to
        // certify that there is no discrepancy going on". A cluster with area but no count is a
        // bug upstream wants visible, so the dump shows it rather than hiding it.
        if cluster.num_std_cell() != 0 || cluster.std_cell_area() != 0 {
            out.push_str(&format!(
                ", StdCells: {} ({} μ²)",
                cluster.num_std_cell(),
                cluster.std_cell_area()
            ));
        }
        // ⚠️ The macro field ends with a trailing comma. Upstream's format string has it; it is
        // not a separator, and dropping it as "obviously a typo" breaks every comparison.
        if cluster.num_macro() != 0 || cluster.macro_area() != 0 {
            out.push_str(&format!(
                ", Macros: {} ({} μ²),",
                cluster.num_macro(),
                cluster.macro_area()
            ));
        }
    }

    out.push('\n');

    for child in &cluster.children {
        write_cluster(child, level + 1, out);
    }
}
