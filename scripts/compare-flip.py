#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Diff our orientation-improvement trace against upstream's, line for line.

🔑 **The `flipping` channel, and it is the largest single oracle in this engine** — 135 lines
across the suite against the boundary push's 37.

⛔ **TWO different line shapes, and only ONE design produces the second.**
`correctMacroOrientationByCluster` prints `Cluster {name} {column-wise (V)|row-wise (H)} flip at
{coord} …`; `correctMacroOrientationSingle` prints `Inst {name} flip {V|H} …`. The branch is
`use_full_halo`, and `halos5` is the ONLY case in the suite that sets it — so a gate green without
`halos5` says nothing whatever about the single path.

⛔ **THE ROOT CLUSTER IS VISITED.** `id_to_cluster` holds it like any other, and an all-macro design
types it `HardMacroCluster` under MPL-27 — so it carries every macro and contributes its own column
and row groups. `macro_only` has ten macros and emits FOURTEEN lines per pass.

⚠️ **The grouping key and the reported value are different accessors.** Columns and rows are keyed
by `getRealX`/`getRealY`, halo OFF; the line reports `macros.front()->getX()`/`getY()`, halo ON.
They differ per macro exactly where `set_macro_halo` named one.

ℹ️ **Wirelength is reported but not yet modelled**, so both `_WL` fields are emitted as zero. That
is CORRECT for the 80 of 135 reference lines whose macros carry no signal net and wrong for the
other 55, so `structural` is counted apart from `exact` — the order, the grouping and the reported
coordinate can all be scored before the model exists.

Usage:  python3 scripts/compare-flip.py <odb-dir> [pushlog-dir] [tcl-dir]
"""
import io, os, re, subprocess, sys

odb_dir = sys.argv[1] if len(sys.argv) > 1 else "/tmp/mplodb"
log_dir = sys.argv[2] if len(sys.argv) > 2 else os.path.join(odb_dir, "pushlogs")
tcl_dir = sys.argv[3] if len(sys.argv) > 3 else odb_dir

MACRO_HALO = re.compile(r"set_macro_halo\s+-macro_name\s+(\S+)\s+-halo\s*\{([^}]*)\}")
BASE_HALO = re.compile(r"set_macro_base_halo\s+([0-9.\s]+)")
GUIDE = re.compile(r"set_macro_guidance_region\s+-macro_name\s+(\S+)\s+-region\s*\{([^}]*)\}")
# ⛔ Same list, and the same reason, as compare-placement.py: a `.odb` cannot carry a command, so
# every option the case sets on the command line has to be re-read from its `.tcl`. An untranslated
# one changes the CLUSTERING and shows up here as a push that visits the wrong clusters.
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

DEBUG = "[DEBUG MPL-flipping] "
# ⚠️ Every line in this channel carries the debug prefix — unlike `boundary_push`, whose distance
# block is `logger_->report`. There is no unprefixed continuation to keep.
WL = re.compile(r"orig_WL (\S+) new_WL (\S+)$")


def flags(case):
    path = os.path.join(tcl_dir, case + ".tcl")
    if not os.path.exists(path):
        return []
    text = io.open(path, encoding="utf-8").read()
    out = []
    for name, vals in MACRO_HALO.findall(text):
        out += ["--macro-halo", f"{name}={','.join(vals.split())}"]
    for vals in BASE_HALO.findall(text):
        out += ["--base-halo", ",".join(vals.split())]
    if "-use_full_halo" in text:
        out += ["--use-full-halo"]
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


def is_trace(line):
    return line.startswith(DEBUG)


def structural(line):
    """The line with both wirelengths blanked — what is scorable before the model exists."""
    return WL.sub("orig_WL . new_WL .", line)


def upstream_trace(path):
    """The contiguous flipping region of an openroad log.

    ⚠️ The `boundary_push` channel precedes this one in the SAME log, so the scan starts at the
    first flipping line rather than at the first debug line of any kind.
    """
    lines = io.open(path, encoding="utf-8", errors="replace").read().split("\n")
    start = next((i for i, l in enumerate(lines) if l.startswith(DEBUG)), None)
    if start is None:
        return []
    out = []
    for line in lines[start:]:
        if not is_trace(line):
            break
        out.append(line)
    return out


cases = sorted(f[:-4] for f in os.listdir(odb_dir) if f.endswith(".odb"))
if not cases:
    sys.exit(f"no .odb files in {odb_dir}")

match = differ = no_push = missing = other = refused = wl_only = 0
for case in cases:
    log = os.path.join(log_dir, case + ".log")
    if not os.path.exists(log):
        print(f"  {case:32} NO LOG — regenerate it")
        missing += 1
        continue
    want = upstream_trace(log)

    path = os.path.join(odb_dir, case + ".odb")
    p = subprocess.run(
        ["./target/debug/cluster-dump", "--shaping", "--flip"] + flags(case) + [path],
        capture_output=True,
        text=True,
    )
    if p.returncode == 3:
        # Vacuous, or no report — upstream prints no push trace for those either, but that has to
        # be earned rather than assumed.
        if not want:
            refused += 1
            print(f"  {case:32} refused, and upstream flipped nothing")
        else:
            differ += 1
            print(f"  {case:32} ⛔ REFUSED while upstream flipped {len(want)} lines")
        continue
    if p.returncode != 0:
        other += 1
        print(f"  {case:32} exit {p.returncode}: {p.stderr.strip().splitlines()[:1]}")
        continue

    got = [l for l in p.stdout.split("\n") if l != ""]

    if got == want == []:
        # ⛔ Both sides silent is never a match on its own -- a harness that did nothing looks the
        # same. Counted separately.
        no_push += 1
        print(f"  {case:32} no flipping from either side")
    elif got == want:
        match += 1
        print(f"  {case:32} match ({len(got)} lines)")
    elif [structural(l) for l in got] == [structural(l) for l in want]:
        # 🔑 Every field agrees except the wirelengths. Counted apart, never as a pass.
        wl_only += 1
        print(f"  {case:32} structural, wirelength unmodelled ({len(got)} lines)")
    else:
        differ += 1
        print(f"  {case:32} DIFFER")
        for i in range(max(len(got), len(want))):
            a = want[i] if i < len(want) else "<missing>"
            b = got[i] if i < len(got) else "<missing>"
            if a != b:
                print(f"      line {i + 1}\n        upstream: {a!r}\n        ours:     {b!r}")
                break

print(
    f"\nflip: {match} exact, {wl_only} structural (wirelength unmodelled), {differ} differ, "
    f"{no_push} none either side, {refused} refused, {missing} without a log, {other} other, "
    f"of {len(cases)}"
)
sys.exit(1 if (differ or other) else 0)
