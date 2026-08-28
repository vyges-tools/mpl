#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Diff our boundary-push trace against upstream's, line for line.

🔑 **The first oracle after the annealers.** Every gate before this one stops at a placement
result; this scores what `Pusher::pushMacrosToCoreBoundaries` does with it -- which macro clusters
it visits, in what order, how far each one measures itself from the core, which pushes it attempts
and which it reverts.

⛔ **Two of the lines carry NO `[DEBUG ...]` prefix.** `Distance to Close Boundaries:` and its rows
are `logger_->report`, not `debugPrint`, so the trace is a mix -- and a version that prefixed them
uniformly would differ on every design that reaches the header.

⚠️ **The header prints even when NOTHING is close enough to push.** `centralization1` emits the
cluster name and the header and no rows at all; a reader who treats the header as implying a push
will score that design as a truncation.

Regenerate the upstream side with `run_push.sh`, which sets
`set_debug_level MPL boundary_push 1` and `flipping 1`.
⚠️ It deliberately does NOT set `hierarchical_macro_placement 2`: `reportLocations` segfaults there
on any design carrying a placement blockage (upstream #11241) and would truncate the log.

Usage:  python3 scripts/compare-push.py <odb-dir> [pushlog-dir] [tcl-dir]
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

DEBUG = "[DEBUG MPL-boundary_push] "
# The unprefixed lines that still belong to the trace: the header and one row per boundary.
HEADER = "Distance to Close Boundaries:"
ROW = re.compile(r"^[BLTR] -?\d+$")


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
    return line.startswith(DEBUG) or line == HEADER or bool(ROW.match(line))


def upstream_trace(path):
    """The contiguous boundary-push region of an openroad log.

    ⚠️ Stops at the first line that is not part of the trace rather than filtering the whole file.
    The `flipping` channel follows immediately, so filtering would silently splice two stages'
    output together and score the join as a match.
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

match = differ = no_push = missing = other = refused = 0
for case in cases:
    log = os.path.join(log_dir, case + ".log")
    if not os.path.exists(log):
        print(f"  {case:32} NO LOG — regenerate it")
        missing += 1
        continue
    want = upstream_trace(log)

    path = os.path.join(odb_dir, case + ".odb")
    p = subprocess.run(
        ["./target/debug/cluster-dump", "--shaping", "--push"] + flags(case) + [path],
        capture_output=True,
        text=True,
    )
    if p.returncode == 3:
        # Vacuous, or no report — upstream prints no push trace for those either, but that has to
        # be earned rather than assumed.
        if not want:
            refused += 1
            print(f"  {case:32} refused, and upstream pushed nothing")
        else:
            differ += 1
            print(f"  {case:32} ⛔ REFUSED while upstream pushed {len(want)} lines")
        continue
    if p.returncode != 0:
        other += 1
        print(f"  {case:32} exit {p.returncode}: {p.stderr.strip().splitlines()[:1]}")
        continue

    got = [l for l in p.stdout.split("\n") if l != ""]

    if got == want == []:
        # ⛔ **Both sides silent is never a match on its own.** The push declines on two guards --
        # an all-macro design, and a single centralized macro array -- and a harness that did
        # nothing at all looks exactly the same. It is counted separately.
        no_push += 1
        print(f"  {case:32} no push from either side")
    elif got == want:
        match += 1
        print(f"  {case:32} match ({len(got)} lines)")
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
    f"\npush: {match} match, {differ} differ, {no_push} no-push (both silent), "
    f"{refused} refused, {missing} without a log, {other} other, of {len(cases)}"
)
sys.exit(1 if (differ or other) else 0)
