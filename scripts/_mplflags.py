# SPDX-License-Identifier: Apache-2.0
"""A case's `rtl_macro_placer` command state, as engine flags — shared by the commit scorer and the
whole-file `run` gate, so both run the engine on the same input.

⛔ Same list, same reason, as compare-flip.py: a `.odb` cannot carry a command, so every option the
case sets on its command line has to be re-read from the `.tcl`.
"""
import io
import os
import re

from _mplchannel import channel_flags

GUIDE = re.compile(r"set_macro_guidance_region\s+-macro_name\s+(\S+)\s+-region\s*\{([^}]*)\}")
NUMERIC = {
    "-boundary_weight": "--boundary-weight",
    "-notch_weight": "--notch-weight",
    "-guidance_weight": "--guidance-weight",
    "-target_util": "--target-util",
}
THRESH = {
    "-max_num_inst": "--max-num-inst",
    "-min_num_inst": "--min-num-inst",
    "-max_num_macro": "--max-num-macro",
    "-min_num_macro": "--min-num-macro",
}


def case_flags(tcl_dir, case):
    path = os.path.join(tcl_dir, case + ".tcl")
    if not os.path.exists(path):
        return []
    text = io.open(path, encoding="utf-8").read()
    out = []
    out += channel_flags(text)
    # ⛔ Only under this flag does `commitClusteringDataToDb` run at all, so only then does the
    # golden carry a GROUPS section.
    if "-keep_clustering_data" in text:
        out += ["--keep-clustering-data"]
    for name, vals in GUIDE.findall(text):
        out += ["--macro-guide", f"{name}={','.join(vals.split())}"]
    for tcl_opt, flag in THRESH.items():
        m = re.search(re.escape(tcl_opt) + r"\s+(\d+)", text)
        if m:
            out += [flag, m.group(1)]
    for tcl_opt, flag in NUMERIC.items():
        m = re.search(re.escape(tcl_opt) + r"\s+([0-9.]+)", text)
        if m:
            out += [flag, m.group(1)]
    return out
