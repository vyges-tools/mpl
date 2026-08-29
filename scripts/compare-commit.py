#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Diff the macro positions `updateMacrosOnDb` leaves against the `.defok` COMPONENTS.

🔑 **The FIRST oracle in this engine that is not a debug trace.** Eight trace gates are exhausted at
0 differ; nothing left to build emits a `set_debug_level` channel. The golden DEFs score what the
commit stage actually produces.

⛔ **This is NOT the `.defok` acceptance gate**, and must not be mistaken for it. The acceptance
gate diffs the whole file and is all-or-nothing per design — it says a design failed, never where.
This reads only the `COMPONENTS` rows for MACROS and scores them per macro, so a denominator exists
and a residual can be attributed.

⛔ **ORIENTATION AND POSITION ARE SCORED SEPARATELY, and that separation is the point.** The DEF
records the position AFTER `commitMacroPlacementToDb` has snapped it to the track grid; this stage
does not snap. So a macro whose orientation matches and whose position is off by a grid fraction is
the SNAPPER's residual, not this stage's error — and counting them together would hide exactly the
measurement this gate exists to make.

⚠️ **A FIXED macro is absent from our dump by design** — `updateMacroOnDb` skips it — but PRESENT in
the DEF, at its original position. Those are counted as `fixed (not written)` rather than missing.

Usage:  python3 scripts/compare-commit.py <odb-dir> [defok-dir] [tcl-dir]
"""
import io, os, re, subprocess, sys

odb_dir = sys.argv[1] if len(sys.argv) > 1 else "/tmp/mplodb"
defok_dir = sys.argv[2] if len(sys.argv) > 2 else os.path.join(odb_dir, "defok")
tcl_dir = sys.argv[3] if len(sys.argv) > 3 else odb_dir

# ⛔ Same list, same reason, as compare-flip.py: a `.odb` cannot carry a command, so every option
# the case sets on its command line has to be re-read from the `.tcl`.
MACRO_HALO = re.compile(r"set_macro_halo\s+-macro_name\s+(\S+)\s+-halo\s*\{([^}]*)\}")
BASE_HALO = re.compile(r"set_macro_base_halo\s+([0-9.\s]+)")
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

# `- NAME MASTER + FIXED ( x y ) ORIENT ;`, the status also being PLACED.
#
# ⛔ **The row does NOT end at the orientation.** A macro carrying a halo is written
# `... ) S + HALO 5000 10000 15000 20000 ;`, so anchoring on `;` right after the orientation
# silently matches NOTHING for exactly the designs that set a halo — `halos1` and `halos2` reported
# every macro as "not in the DEF" until this was relaxed.
COMPONENT = re.compile(
    r"^\s*-\s+(\S+)\s+(\S+)\s+\+\s+(?:FIXED|PLACED)\s+\(\s*(-?\d+)\s+(-?\d+)\s*\)\s+([A-Z]+)\b"
)


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


cases = sorted(f[:-4] for f in os.listdir(odb_dir) if f.endswith(".odb"))
if not cases:
    sys.exit(f"no .odb files in {odb_dir}")

exact = orient_only = differ = refused = missing = other = 0
snap_deltas = []
for case in cases:
    golden = os.path.join(defok_dir, case + ".defok")
    if not os.path.exists(golden):
        print(f"  {case:32} NO .defok")
        missing += 1
        continue

    p = subprocess.run(
        ["./target/debug/cluster-dump", "--shaping", "--commit"] + flags(case) + [
            os.path.join(odb_dir, case + ".odb")
        ],
        capture_output=True,
        text=True,
    )
    if p.returncode == 3:
        refused += 1
        print(f"  {case:32} refused")
        continue
    if p.returncode != 0:
        other += 1
        print(f"  {case:32} exit {p.returncode}: {p.stderr.strip().splitlines()[:1]}")
        continue

    got = {}
    bad_line = None
    for line in p.stdout.split("\n"):
        if not line.strip():
            continue
        parts = line.split()
        # ⚠️ Refuse the case rather than crashing the sweep. A stray trace line on stdout used to
        # abort the whole run at the design that leaked it, losing every case after it.
        if len(parts) != 4:
            bad_line = line
            break
        got[parts[0]] = (int(parts[1]), int(parts[2]), parts[3])
    if bad_line is not None:
        other += 1
        print(f"  {case:32} ⛔ unparseable dump line: {bad_line!r}")
        continue

    # ⚠️ **Keyed by NAME, restricted to what we emitted.** The DEF also holds standard cells, pads
    # and fixed macros; our dump names exactly the macros `updateMacroOnDb` writes, so intersecting
    # on the name is what gives this gate its denominator. A macro we FAIL to emit shows up as a
    # smaller denominator, not as a pass — which is why the count is printed beside every verdict.
    all_rows = {}
    for line in io.open(golden, encoding="utf-8", errors="replace"):
        m = COMPONENT.match(line)
        if m:
            all_rows[m.group(1)] = (int(m.group(3)), int(m.group(4)), m.group(5))
    want = {k: v for k, v in all_rows.items() if k in got}

    if not got:
        print(f"  {case:32} nothing committed (all macros fixed, or refused earlier)")
        refused += 1
        continue

    bad_orient = [n for n in got if n in want and got[n][2] != want[n][2]]
    deltas = [
        (n, got[n][0] - want[n][0], got[n][1] - want[n][1])
        for n in got
        if n in want and got[n][2] == want[n][2] and got[n][:2] != want[n][:2]
    ]
    absent = [n for n in got if n not in want]

    if bad_orient or absent:
        differ += 1
        print(f"  {case:32} ⛔ DIFFER — {len(bad_orient)} orientation, {len(absent)} not in the DEF")
        for n in (bad_orient + absent)[:2]:
            print(f"      {n}: ours {got[n]}  golden {want.get(n)}")
    elif deltas:
        orient_only += 1
        worst = max(abs(dx) for _, dx, _ in deltas), max(abs(dy) for _, _, dy in deltas)
        snap_deltas += [(abs(dx), abs(dy)) for _, dx, dy in deltas]
        print(
            f"  {case:32} orientation exact, {len(deltas)}/{len(got)} unsnapped "
            f"(max |dx|={worst[0]}, |dy|={worst[1]})"
        )
    else:
        exact += 1
        print(f"  {case:32} exact ({len(got)} macros)")

print(
    f"\ncommit: {exact} exact, {orient_only} orientation-exact but unsnapped, {differ} differ, "
    f"{refused} refused, {missing} without a .defok, {other} other, of {len(cases)}"
)
if snap_deltas:
    # 🔑 The measurement this gate exists to make: how much of the gap the SNAPPER owns.
    print(
        f"snap residual: {len(snap_deltas)} macros, "
        f"max |dx|={max(d[0] for d in snap_deltas)}, max |dy|={max(d[1] for d in snap_deltas)}"
    )
sys.exit(1 if (differ or other) else 0)
