#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Diff our placed cluster geometry against upstream's `root.fp.txt`, cluster by cluster.

🔑 **The sharpest oracle this stage has.** The penalty table summarises a placement into nine
numbers, so a term can agree while the geometry behind it does not — and a term can differ while
the geometry is identical, which is the case worth separating. This compares the placement itself:
name, x, y, width, height, per cluster.

⛔ **Written from `best_sa->getMacros()` AFTER `fillDeadSpace`** and before the write-back, so our
side must fill too. `--floorplan` does.

⚠️ **The reference files must be captured per case.** Every case writes to `results/`, and docker
writes as root, so a host-side wipe fails silently and a case that produces no floorplan inherits
the previous one's. `run_hmp_fp.sh` wipes inside the container; four directories shared one
checksum before that was fixed. A design with no `root.fp.txt` took the MACRO path and is reported
as such, not as a failure.

Usage:  python3 scripts/compare-floorplan.py <odb-dir> [fp-dir] [tcl-dir]
"""
import io, os, re, subprocess, sys

odb_dir = sys.argv[1] if len(sys.argv) > 1 else "/tmp/mplodb"
fp_dir = sys.argv[2] if len(sys.argv) > 2 else os.path.join(odb_dir, "fp")
tcl_dir = sys.argv[3] if len(sys.argv) > 3 else odb_dir

MACRO_HALO = re.compile(r"set_macro_halo\s+-macro_name\s+(\S+)\s+-halo\s*\{([^}]*)\}")
BASE_HALO = re.compile(r"set_macro_base_halo\s+([0-9.\s]+)")
GUIDE = re.compile(r"set_macro_guidance_region\s+-macro_name\s+(\S+)\s+-region\s*\{([^}]*)\}")
# ⛔ `rtl_macro_placer`'s own threshold options are engine state too. Supplying them keeps the
# tree's `max_level` at 2, and `adjustSoftBlockageWeight` fires only at 1 -- so an untranslated
# threshold silently changes the soft-blockage WEIGHT, not just the clustering.
THRESH = {
    "-max_num_inst": "--max-num-inst",
    "-min_num_inst": "--min-num-inst",
    "-max_num_macro": "--max-num-macro",
    "-min_num_macro": "--min-num-macro",
}


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
    return out


def rows(text):
    """name -> (x, y, w, h), in order."""
    out = []
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 5:
            out.append((parts[0], tuple(parts[1:])))
    return out


cases = sorted(os.listdir(fp_dir)) if os.path.isdir(fp_dir) else []
if not cases:
    print(f"ERROR: no reference floorplans in {fp_dir} -- run run_hmp_fp.sh")
    raise SystemExit(2)

def compare(case, artifact, flag, label):
    """One stage dump, ours against upstream's. Returns 'match', 'differ' or None if absent."""
    ref_path = os.path.join(fp_dir, case, artifact)
    if not os.path.exists(ref_path):
        return None
    odb = os.path.join(odb_dir, case + ".odb")
    if not os.path.exists(odb):
        return "no-odb"
    want = io.open(ref_path, encoding="utf-8").read().split()
    r = subprocess.run(["./target/debug/cluster-dump", "--shaping", flag]
                       + flags(case) + [odb], capture_output=True, text=True)
    return "match" if r.stdout.split() == want else "differ"


nets_ok = nets_differ = 0
ok = differ = macro_path = refused = 0
for case in cases:
    # 🔑 The BUNDLED NETS are written before the anneal, so they are the earliest artifact this
    # stage produces — a wirelength difference is explained here or it is not an input problem.
    n = compare(case, "root.net.txt", "--nets", "nets")
    if n == "match":
        nets_ok += 1
    elif n == "differ":
        nets_differ += 1
        print(f"  {case:32} NETS DIFFER")
    ref_path = os.path.join(fp_dir, case, "root.fp.txt")
    if not os.path.exists(ref_path):
        print(f"  {case:32} macro-path (no floorplan written)"); macro_path += 1; continue
    odb = os.path.join(odb_dir, case + ".odb")
    if not os.path.exists(odb):
        print(f"  {case:32} NO ODB"); refused += 1; continue
    want = rows(io.open(ref_path, encoding="utf-8").read())
    r = subprocess.run(["./target/debug/cluster-dump", "--shaping", "--floorplan"]
                       + flags(case) + [odb], capture_output=True, text=True)
    got = rows(r.stdout)
    if got == want:
        print(f"  {case:32} match ({len(got)} clusters)"); ok += 1
        continue
    differ += 1
    by_name = dict(want)
    first = None
    for name, geom in got:
        if by_name.get(name) != geom:
            first = f"{name}: {geom} != {by_name.get(name)}"
            break
    if first is None:
        first = f"{len(got)} clusters, upstream has {len(want)}"
    print(f"  {case:32} DIFFERS  {first}")

print(f"\nfloorplan: {ok} match, {differ} differ, {macro_path} macro-path, {refused} refused, of {len(cases)}")
print(f"nets:      {nets_ok} match, {nets_differ} differ")
raise SystemExit(1 if differ or refused else 0)
