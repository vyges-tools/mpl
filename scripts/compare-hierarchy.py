#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Diff our physical hierarchy against upstream's, case by case.

⛔ **Prepare each .odb with EVERY command up to `rtl_macro_placer`, not just the reads.**
Pin constraints, halos and guidance regions are all set by commands a read-only grep silently
drops -- and a design missing them still clusters, just differently. That produced five false
mismatches before it was noticed, and the engine was right every time.

Usage:  python3 scripts/compare-hierarchy.py <odb-dir> [golden-file]
"""
import io, subprocess, sys, os

odb_dir = sys.argv[1] if len(sys.argv) > 1 else "/tmp/mplodb"
golden = sys.argv[2] if len(sys.argv) > 2 else "tests/goldens/macro_only_hierarchy.txt"

gold, cur = {}, None
for line in io.open(golden, encoding="utf-8"):
    line = line.rstrip("\n")
    if line.startswith("@@@CASE "):
        cur = line[8:].strip(); gold[cur] = []
    elif cur is not None and line and not line.startswith("["):
        gold[cur].append(line)

if not gold:
    print(f"ERROR: no cases in {golden}"); raise SystemExit(2)

ok = fail = refused = 0
for case, want in sorted(gold.items()):
    path = os.path.join(odb_dir, case + ".odb")
    if not os.path.exists(path):
        print(f"  {case:24} NO ODB"); refused += 1; continue
    p = subprocess.run(["target/debug/cluster-dump", path], capture_output=True, text=True)
    if p.returncode != 0:
        print(f"  {case:24} REFUSED: {p.stderr.strip()[:60]}"); refused += 1; continue
    got = [l for l in p.stdout.splitlines() if l.strip()]
    if got == want:
        print(f"  {case:24} match ({len(want)} lines)"); ok += 1
    else:
        fail += 1; print(f"  {case:24} DIFFERS")
        for a, b in zip(want, got):
            if a != b:
                print(f"      upstream |{a}|"); print(f"      ours     |{b}|"); break
        if len(want) != len(got):
            print(f"      line count: upstream {len(want)}, ours {len(got)}")

print(f"\n{ok} match, {fail} differ, {refused} refused, of {len(gold)}")
raise SystemExit(0 if fail == 0 and refused == 0 else 1)
