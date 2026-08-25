#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Diff our design-data report against upstream's, case by case.

🔑 **This is the only oracle that sees a resolved HALO before the commit stage.** The hierarchy
dump does not print halos at all, so without this the halo rules are exercised by one MPL-65
verdict and scored by nothing.

⛔ **`set_macro_halo` is a COMMAND, not database state.** Preparing the `.odb` from the case's
.tcl captures `set_io_pin_constraint` — which writes onto the ports — and captures nothing of the
halo commands. This script re-reads them from the .tcl and passes them on the command line; a
harness that skipped that would score two cases against an input upstream never used.

Usage:  python3 scripts/compare-report.py <odb-dir> [tcl-dir] [golden-file]
"""
import io, os, re, subprocess, sys

odb_dir = sys.argv[1] if len(sys.argv) > 1 else "/tmp/mplodb"
tcl_dir = sys.argv[2] if len(sys.argv) > 2 else odb_dir
golden = sys.argv[3] if len(sys.argv) > 3 else "tests/goldens/design_report.txt"

MACRO_HALO = re.compile(r"set_macro_halo\s+-macro_name\s+(\S+)\s+-halo\s*\{([^}]*)\}")
BASE_HALO = re.compile(r"set_macro_base_halo\s+([0-9.\s]+)")

# ⛔ Every mpl command that changes what the engine reads has to be translated here. Three were
# missed on the first run of this harness -- `set_macro_base_halo`, `-use_full_halo`, and the
# MICRON unit on `set_macro_halo` -- and each produced a difference that looked like an engine
# defect. Anything in a .tcl that is not in this list is being silently ignored.
KNOWN = ("set_macro_halo", "set_macro_base_halo", "-use_full_halo")


def halo_args(case):
    """The case's halo commands, as flags. Values stay in MICRONS: the binary converts.

    ⚠️ Two values mirror into four — `{2 4}` means (2, 4, 2, 4), which is `parse_halo`'s own
    rule. Treating them as (2, 4, 0, 0) silently shrinks two sides.
    """
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


gold, cur = {}, None
for line in io.open(golden, encoding="utf-8"):
    line = line.rstrip("\n")
    if line.startswith("@@@CASE "):
        cur = line[8:].strip(); gold[cur] = []
    elif cur is not None and line:
        gold[cur].append(line)

if not gold:
    print(f"ERROR: no cases in {golden}"); raise SystemExit(2)

ok = fail = refused = 0
for case, want in sorted(gold.items()):
    path = os.path.join(odb_dir, case + ".odb")
    if not os.path.exists(path):
        print(f"  {case:24} NO ODB"); refused += 1; continue
    extra = halo_args(case)
    if extra is None:
        refused += 1; continue
    p = subprocess.run(["target/debug/cluster-dump", "--report"] + extra + [path],
                       capture_output=True, text=True)
    if p.returncode != 0:
        print(f"  {case:24} REFUSED: {p.stderr.strip()[:60]}"); refused += 1; continue
    got = [l for l in p.stdout.splitlines() if l.strip()]
    if got == want:
        print(f"  {case:24} match"); ok += 1
    else:
        fail += 1; print(f"  {case:24} DIFFERS")
        for a, b in zip(want, got):
            if a != b:
                print(f"      upstream |{a}|"); print(f"      ours     |{b}|")
        if len(want) != len(got):
            print(f"      line count: upstream {len(want)}, ours {len(got)}")

print(f"\n{ok} match, {fail} differ, {refused} refused, of {len(gold)}")
raise SystemExit(0 if fail == 0 and refused == 0 else 1)
