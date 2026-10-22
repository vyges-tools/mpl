#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""The `.defok` ACCEPTANCE gate: `vyges-mpl run` on each case, the WHOLE written DEF against the
case's golden, all or nothing per design.

🔑 **What the commit scorer cannot see.** `compare-commit.py` reads the rows `mpl` writes and scores
them per macro; this diffs the file `run` writes — row ORDER (the BLOCKAGES section is the order the
blockages were created in), statuses, and everything `mpl` must leave alone.

⛔ **What a DEF cannot show, so this gate cannot fail on it** (each mutation-checked 2026-10-22):
- LOCKED vs FIRM — both are written `+ FIXED`;
- the order blockages are created in — the writer sorts the section by rectangle.
Both are pinned by unit tests in `src/commit.rs` instead.

⚠️ `-keep_clustering_data` cases are NOT COMPARABLE: `run` does not write clustering groups.
A refused case is reported refused; a vacuous one (no unfixed macros) is compared all the same,
since writing nothing is then the right answer and the golden should equal the input.

Usage:  python3 scripts/compare-run-def.py <odb-dir> [defok-dir] [tcl-dir] [--mpl PATH] [--opendb PATH]
`--opendb` writes the input's DEF for a vacuous case; give it a path where the binary is not on PATH.
Prints one line per case and `run-def: N/M exact`; exit 0 iff every comparable case is exact.
"""
import os
import subprocess
import sys
import tempfile

from _mplflags import case_flags

args = [a for a in sys.argv[1:]]
tools = {"--mpl": "./target/debug/vyges-mpl", "--opendb": "vyges-opendb"}
for flag in tools:
    if flag in args:
        i = args.index(flag)
        tools[flag] = args[i + 1]
        del args[i:i + 2]
mpl, opendb = tools["--mpl"], tools["--opendb"]
odb_dir = args[0] if args else "/tmp/mplodb"
defok_dir = args[1] if len(args) > 1 else os.path.join(odb_dir, "defok")
tcl_dir = args[2] if len(args) > 2 else odb_dir

cases = sorted(f[:-4] for f in os.listdir(odb_dir) if f.endswith(".odb"))
if not cases:
    sys.exit(f"no .odb files in {odb_dir}")

exact = differ = refused = skipped = other = 0
for case in cases:
    golden = os.path.join(defok_dir, case + ".defok")
    if not os.path.exists(golden):
        print(f"  {case:32} NO .defok")
        skipped += 1
        continue
    f = case_flags(tcl_dir, case)
    if "--keep-clustering-data" in f:
        print(f"  {case:32} not comparable (clustering groups)")
        skipped += 1
        continue
    with tempfile.TemporaryDirectory() as t:
        out_odb, out_def = os.path.join(t, "out.odb"), os.path.join(t, "out.def")
        r = subprocess.run([mpl, "run", os.path.join(odb_dir, case + ".odb"), "--out-odb", out_odb,
                            "--out-def", out_def] + f, capture_output=True, text=True)
        if r.returncode == 1:
            refused += 1
            print(f"  {case:32} refused")
            continue
        if r.returncode == 3:
            # Nothing written: the input is the answer.
            subprocess.run([opendb, "write-def", "--input", os.path.join(odb_dir, case + ".odb"),
                            "--output", out_def], capture_output=True, check=True)
        elif r.returncode != 0:
            other += 1
            print(f"  {case:32} exit {r.returncode}: {r.stderr.strip().splitlines()[-1:]}")
            continue
        got = open(out_def, errors="replace").read().splitlines()
    want = open(golden, errors="replace").read().splitlines()
    if got == want:
        exact += 1
        print(f"  {case:32} exact")
        continue
    differ += 1
    n = next((k for k, (a, b) in enumerate(zip(got, want)) if a != b), min(len(got), len(want)))
    print(f"  {case:32} DIFFER at line {n + 1} ({len(got)} vs {len(want)} lines)")
    print(f"      ours:   {got[n] if n < len(got) else '<end>'}")
    print(f"      golden: {want[n] if n < len(want) else '<end>'}")

scored = exact + differ
print(f"\nrun-def: {exact}/{scored} exact, {refused} refused, {skipped} not comparable, {other} errors")
sys.exit(0 if differ == 0 and other == 0 and scored > 0 else 1)
