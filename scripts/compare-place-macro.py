#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Score `place-macro` on every upstream case that runs `place_macro`.

Each such case reads LEF and DEF, calls `place_macro -macro_name M -location {x y} -orientation O`
and writes the design back, compared against its `.defok`. This replays it: the same LEF/DEF into a
database, `vyges-mpl place-macro --macro M=x,y,O`, the result written as DEF, and the COMPONENTS
section compared line for line with the case's `.defok`.

⚠️ Our `place-macro` is LibreLane's manual placement — it never snaps to tracks. The upstream command
snaps unless the orientation is a right angle or `-exact` is given, so a case whose upstream run
would snap is reported NOT COMPARABLE rather than scored either way.

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

RIGHT_ANGLE = {"R90", "R270", "MXR90", "MYR90"}


def place_macro_cases(test_dir):
    """`case -> (lefs, def, [(macro, x, y, orient, exact)], defok)` for every case calling place_macro."""
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
            calls.append((name, w[i + 1], w[i + 2], orient, "-exact" in w))
        out[f[:-4]] = (lefs, defs[0] if defs else None, calls, os.path.join(test_dir, f[:-4] + ".defok"))
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


def run(a, case, lefs, defp, calls):
    """COMPONENTS after replaying `calls` with `vyges-mpl place-macro`, or None if a step failed."""
    with tempfile.TemporaryDirectory() as t:
        odb, out, ddef = os.path.join(t, "in.odb"), os.path.join(t, "out.odb"), os.path.join(t, "out.def")
        cmd = [a.opendb, "import"] + sum((["--lef", os.path.join(a.test_dir, l)] for l in lefs), []) + \
              ["--def", os.path.join(a.test_dir, defp), "--output", odb]
        subprocess.run(cmd, capture_output=True, check=True)
        mcmd = [a.mpl, "place-macro", odb, "--out-odb", out] + sum((["--macro", f"{n}={x},{y},{o}"] for n, x, y, o, _ in calls), [])
        r = subprocess.run(mcmd, capture_output=True, text=True)
        if r.returncode:
            print(f"  {case:40} FAILED — place-macro exit {r.returncode}: {r.stderr.strip()[:200]}")
            return None
        subprocess.run([a.opendb, "write-def", "--input", out, "--output", ddef], capture_output=True, check=True)
        return components(ddef)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("test_dir")
    ap.add_argument("--opendb", default=os.path.expanduser("~/.vyges/bin/vyges-opendb"))
    ap.add_argument("--mpl", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "target", "debug", "vyges-mpl"))
    a = ap.parse_args()
    cases = place_macro_cases(a.test_dir)
    exact = comparable = 0
    for case, (lefs, defp, calls, defok) in cases.items():
        if any(not (o in RIGHT_ANGLE or ex) for _, _, _, o, ex in calls):
            print(f"  {case:40} NOT COMPARABLE — the upstream run snaps; ours does not")
            continue
        comparable += 1
        got = run(a, case, lefs, defp, calls)
        # Teeth: the same case with every location moved by one micron must NOT match the golden.
        moved = [(n, str(float(x) + 1), y, o, ex) for n, x, y, o, ex in calls]
        wrong = run(a, case, lefs, defp, moved)
        if got is None:
            continue
        if got != components(defok):
            print(f"  {case:40} DIFFERS")
        elif wrong == got:
            print(f"  {case:40} NO TEETH — a moved macro scores the same")
        else:
            exact += 1
            print(f"  {case:40} exact (a moved macro differs)")
    print(f"place_macro: {exact}/{comparable} exact ({len(cases) - comparable} not comparable)")
    sys.exit(0 if exact == comparable and comparable > 0 else 1)


if __name__ == "__main__":
    main()
