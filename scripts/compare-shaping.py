#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Diff our coarse-shaping trace against upstream's, case by case.

🔑 **A per-stage oracle, and the only one that sees a blockage individually.** The design report
scores the pin-access work through nothing at all and the halos through one aggregate; this
compares every traversal line, every hard cluster's tilings, the depth-limit table, each base
depth and **each blockage's boundary, endpoints and depth**.

⛔ **Eight of the 34 designs anneal at the root** and are refused by name rather than
approximated. They are reported as `anneals`, never as a pass — a shorter trace that agrees as far
as it goes is not a match.

Regenerate the upstream side with, per case:
    printf 'set_debug_level MPL coarse_shaping 2\\nsource "CASE.tcl"\\n' > wrapcs_CASE.tcl
    docker run --rm -v "$P:$P" -w "$P" vyges-openroad:945a9f4-tcl \\
        bash -c 'openroad -no_init -exit wrapcs_CASE.tcl'

Usage:  python3 scripts/compare-shaping.py <odb-dir> [log-dir] [tcl-dir]
"""
import io, os, re, subprocess, sys

odb_dir = sys.argv[1] if len(sys.argv) > 1 else "/tmp/mplodb"
log_dir = sys.argv[2] if len(sys.argv) > 2 else os.path.join(odb_dir, "cslogs")
tcl_dir = sys.argv[3] if len(sys.argv) > 3 else odb_dir

MACRO_HALO = re.compile(r"set_macro_halo\s+-macro_name\s+(\S+)\s+-halo\s*\{([^}]*)\}")
BASE_HALO = re.compile(r"set_macro_base_halo\s+([0-9.\s]+)")

# ⛔ Same list as compare-report.py, for the same reason: an mpl command that changes what the
# engine reads and is not translated here is silently ignored, and the difference it causes looks
# like an engine defect.
KNOWN = ("set_macro_halo", "set_macro_base_halo", "-use_full_halo")

DEBUG = "[DEBUG MPL-coarse_shaping] "

# The unprefixed lines that still belong to the trace: a tiling continuation, and the four rows of
# the depth table, which upstream writes with `report` rather than `debugPrint`.
CONTINUATION = (
    re.compile(r"^ < -?\d+ , -?\d+ >"),
    re.compile(r"^  Pin Access Depth"),
    re.compile(r"^-{10,}$"),
    re.compile(r"^\s+(Horizontal|Vertical)\s+\|"),
)


def halo_args(case):
    """The case's halo commands, as flags. Values stay in MICRONS: the binary converts."""
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
    return out


def is_trace(line):
    return (
        line.startswith(DEBUG)
        or line == ""
        or any(p.match(line) for p in CONTINUATION)
    )


def upstream_trace(path):
    """The contiguous trace region of an openroad log.

    ⚠️ Stops at the first non-blank line that is not part of the trace rather than filtering the
    whole file. A foreign line inside the region — an MPL warning, say — then TRUNCATES the
    expected text and shows up as a difference, which is the safe direction: filtering would have
    quietly dropped it and scored the rest as a match.
    """
    lines = io.open(path, encoding="utf-8").read().split("\n")
    start = next((i for i, l in enumerate(lines) if l.startswith(DEBUG)), None)
    if start is None:
        return []
    out = []
    for line in lines[start:]:
        if not is_trace(line):
            break
        out.append(line)
    while out and out[-1] == "":
        out.pop()
    return out


cases = sorted(
    f[:-4] for f in os.listdir(odb_dir) if f.endswith(".odb")
)
if not cases:
    sys.exit(f"no .odb files in {odb_dir}")

match = differ = anneals = missing = other = only_macros = suspect = 0
for case in cases:
    log = os.path.join(log_dir, case + ".log")
    if not os.path.exists(log):
        print(f"  {case:24s} NO LOG — regenerate it")
        missing += 1
        continue

    log_text = io.open(log, encoding="utf-8").read()
    want = upstream_trace(log)
    p = subprocess.run(
        ["target/debug/cluster-dump", "--shaping", os.path.join(odb_dir, case + ".odb")]
        + halo_args(case),
        capture_output=True,
        text=True,
    )
    if p.returncode == 4:
        anneals += 1
        print(f"  {case:24s} anneals — refused, not approximated")
        continue
    if p.returncode != 0:
        other += 1
        print(f"  {case:24s} exit {p.returncode}: {p.stderr.strip().splitlines()[:1]}")
        continue

    got = p.stdout.split("\n")
    while got and got[-1] == "":
        got.pop()

    if got == want == []:
        # ⛔ **An empty trace is never reported as a match.** Both sides producing nothing is
        # exactly what a harness that silently did nothing looks like, so it has to be earned:
        # upstream emits no trace here only because MPL-27 returns from `runCoarseShaping`
        # straight after `setRootShapes`, and the warning in its log is the proof.
        if "MPL-0027" in log_text:
            only_macros += 1
            print(f"  {case:24s} only-macros (MPL-27) — no trace from either side, as expected")
        else:
            suspect += 1
            print(f"  {case:24s} ⛔ EMPTY on both sides with no MPL-27 to explain it")
    elif got == want:
        match += 1
        print(f"  {case:24s} match ({len(got)} lines)")
    else:
        differ += 1
        print(f"  {case:24s} DIFFER")
        for i in range(max(len(got), len(want))):
            a = want[i] if i < len(want) else "<missing>"
            b = got[i] if i < len(got) else "<missing>"
            if a != b:
                print(f"      line {i + 1}\n        upstream: {a!r}\n        ours:     {b!r}")
                break

total = len(cases)
print(
    f"\n{match} match, {only_macros} only-macros, {differ} differ, "
    f"{anneals} anneal (refused), {missing} without a log, {other} other, "
    f"{suspect} unexplained-empty, of {total}"
)
sys.exit(1 if (differ or other or suspect) else 0)
