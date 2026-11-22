#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Score `place-macro --snap` on every upstream case that runs `place_macro`.

Each such case reads LEF and DEF and calls `place_macro -macro_name M -location {x y}
[-orientation O] [-exact] [-allow_overlap] [-snap_to_site_grid]`. This replays it with the
reference's own command, `vyges-mpl place-macro --snap` (`--exact`, `--snap-to-site-grid` passed
through), on the same LEF/DEF, and compares what the case itself checks:

  - a case that ships a `.defok`: the COMPONENTS section of the written DEF, line for line;
  - otherwise the case's `.ok` log: every `MPL-0035 Macro M placed. Bounding box (...) Orientation
    O` line, against the same line our run emits (the box AFTER snapping, so the snap is scored).

What is NOT compared, and why (each printed in the summary):
  - the overlap check after the snap (`findOverlappedMacros`, MPL-0041, and `-allow_overlap`): our
    command has none. A case whose `.ok` holds an MPL-0041, or a call with `-allow_overlap`, is
    NOT COMPARABLE. On the cases scored the reference found no overlap, so its check changed
    nothing there; that is all a score here says about it.
  - any other line of the `.ok` (ODB reader messages, MPL-0036 for a right-angle rotation).

Teeth: every case is run a second time with each location moved by one micron, and that run must
NOT match. One micron is more than a track pitch or a site width in the shipped cases, so a snap
cannot pull the moved macro back to the same place.

Usage:  compare-place-macro.py <upstream-mpl-test-dir> [--opendb vyges-opendb] [--mpl vyges-mpl]
Prints one line per case and `place_macro: N/M exact`; exit 0 iff every comparable case is exact.
Other scripts import `place_macro_cases()` to count these cases as scored.
"""
import argparse
import os
import re
import shlex
import subprocess
import sys
import tempfile

PLACED = re.compile(r"Macro (\S+) placed\. Bounding box \(([-\d.]+)um, ([-\d.]+)um\), "
                    r"\(([-\d.]+)um, ([-\d.]+)um\)\. Orientation (\S+)")


def place_macro_cases(test_dir):
    """`case -> (lefs, def, [(macro, x, y, orient, flags)], golden)` for every case calling
    place_macro; `flags` is the set of the call's flag words, `golden` its `.defok` if one ships,
    else its `.ok`."""
    out = {}
    for f in sorted(os.listdir(test_dir)):
        if not f.endswith(".tcl"):
            continue
        text = open(os.path.join(test_dir, f), errors="replace").read()
        if not re.search(r"^\s*place_macro\b", text, re.M):
            continue
        lefs = re.findall(r"^\s*read_lef\s+\"?([^\s\"]+)\"?", text, re.M)
        defs = re.findall(r"^\s*read_def\s+\"?([^\s\"]+)\"?", text, re.M)
        calls = []
        for line in re.findall(r"^\s*place_macro\b(.*)$", text, re.M):
            w = shlex.split(line.replace("{", " ").replace("}", " "))
            name = w[w.index("-macro_name") + 1]
            i = w.index("-location")
            orient = w[w.index("-orientation") + 1] if "-orientation" in w else "R0"
            flags = {x for x in w if x in ("-exact", "-allow_overlap", "-snap_to_site_grid")}
            calls.append((name, w[i + 1], w[i + 2], orient, flags))
        stem = os.path.join(test_dir, f[:-4])
        golden = stem + ".defok" if os.path.exists(stem + ".defok") else stem + ".ok"
        out[f[:-4]] = (lefs, defs[0] if defs else None, calls, golden)
    return out


def components(path):
    lines, on = [], False
    for line in open(path):
        if line.startswith("COMPONENTS"):
            on = True
        if on:
            lines.append(line.rstrip())
        if line.startswith("END COMPONENTS"):
            break
    return lines


def placed_lines(text):
    return [m.groups() for m in PLACED.finditer(text)]


def run(a, case, lefs, defp, calls, golden):
    """What `golden` checks, after replaying `calls`: COMPONENTS for a `.defok`, the MPL-0035
    lines for an `.ok`. None if a step failed."""
    with tempfile.TemporaryDirectory() as t:
        odb, out, ddef = os.path.join(t, "in.odb"), os.path.join(t, "out.odb"), os.path.join(t, "out.def")
        cmd = [a.opendb, "import"] + sum((["--lef", os.path.join(a.test_dir, l)] for l in lefs), []) + \
              ["--def", os.path.join(a.test_dir, defp), "--output", odb]
        subprocess.run(cmd, capture_output=True, check=True)
        src, placed = odb, []
        # One place-macro per call, in the script's order: each call snaps against the design the
        # previous one left (the site-grid snap avoids macros already placed).
        for n, x, y, o, flags in calls:
            mcmd = [a.mpl, "place-macro", src, "--out-odb", out, "--macro", f"{n}={x},{y},{o}", "--snap"]
            mcmd += ["--exact"] if "-exact" in flags else []
            mcmd += ["--snap-to-site-grid"] if "-snap_to_site_grid" in flags else []
            r = subprocess.run(mcmd, capture_output=True, text=True,
                               env=dict(os.environ, VYGES_LOG_FORMAT="text"))
            if r.returncode:
                print(f"  {case:40} FAILED — place-macro exit {r.returncode}: {r.stderr.strip()[:200]}")
                return None
            placed += placed_lines(r.stderr)
            src = os.path.join(t, f"step{len(placed)}.odb")
            os.replace(out, src)
        if golden.endswith(".ok"):
            return placed
        subprocess.run([a.opendb, "write-def", "--input", src, "--output", ddef], capture_output=True, check=True)
        return components(ddef)


def expected(golden):
    return placed_lines(open(golden).read()) if golden.endswith(".ok") else components(golden)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("test_dir")
    ap.add_argument("--opendb", default=os.path.expanduser("~/.vyges/bin/vyges-opendb"))
    ap.add_argument("--mpl", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "target", "debug", "vyges-mpl"))
    a = ap.parse_args()
    cases = place_macro_cases(a.test_dir)
    exact = comparable = 0
    for case, (lefs, defp, calls, golden) in cases.items():
        ok_log = os.path.join(a.test_dir, case + ".ok")
        log = open(ok_log, errors="replace").read() if os.path.exists(ok_log) else ""
        if any("-allow_overlap" in f for *_, f in calls) or "MPL-0041" in log:
            print(f"  {case:40} NOT COMPARABLE — the overlap check (MPL-0041, -allow_overlap) is not modelled")
            continue
        want = expected(golden)
        if not want:
            print(f"  {case:40} NOT COMPARABLE — {os.path.basename(golden)} holds nothing this scorer reads")
            continue
        comparable += 1
        got = run(a, case, lefs, defp, calls, golden)
        moved = [(n, str(float(x) + 1), y, o, f) for n, x, y, o, f in calls]
        wrong = run(a, case, lefs, defp, moved, golden)
        what = "COMPONENTS" if golden.endswith(".defok") else "MPL-0035 box"
        if got is None:
            continue
        if got != want:
            print(f"  {case:40} DIFFERS ({what})")
            if not golden.endswith(".defok"):
                print(f"      ref  {want}\n      ours {got}")
        elif wrong == got:
            print(f"  {case:40} NO TEETH — a moved macro scores the same")
        else:
            exact += 1
            print(f"  {case:40} exact ({what}; a moved macro differs)")
    print(f"place_macro: {exact}/{comparable} exact ({len(cases) - comparable} not comparable); "
          f"not compared on any case: the MPL-0041 overlap check, .ok lines other than MPL-0035")
    sys.exit(0 if exact == comparable and comparable > 0 else 1)


if __name__ == "__main__":
    main()
