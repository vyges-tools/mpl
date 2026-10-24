#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Diff our message lines against upstream's, case by case.

The stage scorers read debug traces, the design report and the DEF; none reads what the run SAYS.
This does: for every `[INFO|WARNING MPL-nnnn]` code the engine writes (`WRITTEN`), upstream's
lines with that code, in order, must be ours, in order, character for character. Upstream's are
read from the capture's `logs/wrap_<case>.log`; ours from the stderr of a full run
(`cluster-dump --shaping --commit`, the path the commit scorer takes).

Codes upstream printed that we do not write are listed (UNWRITTEN) and never fail a case.

⚠️ A check that cannot fail proves nothing: VYGES_MPL_LOGCHECK_SELFTEST=1 corrupts the first
compared upstream line, and every compared case must then DIFFER.

Usage:  python3 scripts/compare-log.py <probe-dir> [tcl-dir]
"""
import collections, io, os, re, subprocess, sys

probe = sys.argv[1] if len(sys.argv) > 1 else "/tmp/mplodb"
tcl_dir = sys.argv[2] if len(sys.argv) > 2 else probe

from _mplflags import case_flags

#: The codes the engine writes. A code added to the engine is added here, or it is never scored.
WRITTEN = {13, 17, 25, 26, 27, 62, 63}
LINE = re.compile(r"^\[(INFO|WARNING|ERROR|CRITICAL) MPL-(\d{4})\] .*$")
SELFTEST = os.environ.get("VYGES_MPL_LOGCHECK_SELFTEST") == "1"


def coded(text):
    return [(int(m.group(2)), m.group(0)) for m in map(LINE.match, text.splitlines()) if m]


cases = sorted(f[:-4] for f in os.listdir(probe) if f.endswith(".odb"))
if not cases:
    sys.exit(f"no .odb files in {probe}")

ok = fail = missing = 0
unwritten = collections.Counter()
for case in cases:
    log = os.path.join(probe, "logs", f"wrap_{case}.log")
    if not os.path.exists(log):
        print(f"  {case:24} NO UPSTREAM LOG"); missing += 1; continue
    ref = coded(io.open(log, encoding="utf-8", errors="replace").read())
    p = subprocess.run(["./target/debug/cluster-dump", "--shaping", "--commit"]
                       + case_flags(tcl_dir, case) + [os.path.join(probe, case + ".odb")],
                       capture_output=True, text=True)
    ours = coded(p.stderr)
    want = [l for c, l in ref if c in WRITTEN]
    got = [l for c, l in ours if c in WRITTEN]
    if SELFTEST and want:
        want[0] += " (corrupted)"
    unwritten.update(c for c, _ in ref if c not in WRITTEN)
    if want == got:
        print(f"  {case:24} match ({len(want)} lines)"); ok += 1
        continue
    fail += 1
    print(f"  {case:24} DIFFERS")
    for i in range(max(len(want), len(got))):
        a = want[i] if i < len(want) else "<none>"
        b = got[i] if i < len(got) else "<none>"
        if a != b:
            print(f"      upstream |{a}|"); print(f"      ours     |{b}|")
            break

print("UNWRITTEN codes upstream printed (cases):",
      ", ".join(f"MPL-{c:04d} x{n}" for c, n in unwritten.most_common()) or "none")
print(f"\n{ok} match, {fail} differ, {missing} without an upstream log, of {len(cases)}")
raise SystemExit(0 if fail == 0 and missing == 0 and ok > 0 else 1)
