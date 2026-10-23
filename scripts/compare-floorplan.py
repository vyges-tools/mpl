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

from _mplchannel import channel_flags
GUIDE = re.compile(r"set_macro_guidance_region\s+-macro_name\s+(\S+)\s+-region\s*\{([^}]*)\}")
# ⛔ `rtl_macro_placer`'s own threshold options are engine state too. Supplying them keeps the
# tree's `max_level` at 2, and `adjustSoftBlockageWeight` fires only at 1 -- so an untranslated
# threshold silently changes the soft-blockage WEIGHT, not just the clustering.
# ⚠️ The WEIGHT and utilization options too: an untranslated `-boundary_weight 0.0` leaves a term
# live that the case turned OFF, and `-target_util` ramps the utilization list from a different base.
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


def flags(case):
    path = os.path.join(tcl_dir, case + ".tcl")
    if not os.path.exists(path):
        return []
    text = io.open(path, encoding="utf-8").read()
    out = []
    out += channel_flags(text)
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

def ref_file_name(parent):
    """`writeNetFile`'s name: the parent's name with `":<>|*?/` each replaced by `--`."""
    return re.sub(r'[":<>\|\*\?/]', "--", parent)


def compare_nets(case):
    """Every parent's bundled nets, ours against upstream's file for THAT parent.

    ⛔ Upstream writes one `<parent>.net.txt` per parent it places; ours heads each parent's block
    with `# parent <name>`. Comparing only `root.net.txt` scored the first block and silently
    passed every nested parent — the multi-level problems a hierarchical design produces."""
    case_dir = os.path.join(fp_dir, case)
    refs = {f[:-len(".net.txt")]: f for f in os.listdir(case_dir) if f.endswith(".net.txt")}
    if not refs:
        return None, []
    odb = os.path.join(odb_dir, case + ".odb")
    if not os.path.exists(odb):
        return "no-odb", []
    r = subprocess.run(["./target/debug/cluster-dump", "--shaping", "--nets"]
                       + flags(case) + [odb], capture_output=True, text=True)
    blocks, current = {}, None
    for line in r.stdout.splitlines():
        if line.startswith("# parent "):
            current = ref_file_name(line[len("# parent "):])
            blocks[current] = []
        elif current is not None:
            blocks[current] += line.split()
    problems = []
    for key in sorted(set(refs) | set(blocks)):
        if key not in blocks:
            problems.append(f"{key}: upstream only")
        elif key not in refs:
            problems.append(f"{key}: ours only")
        elif blocks[key] != io.open(os.path.join(case_dir, refs[key]), encoding="utf-8").read().split():
            problems.append(f"{key}: differ")
    return ("differ" if problems else "match"), problems


nets_ok = nets_differ = 0
ok = differ = macro_path = refused = 0
for case in cases:
    # 🔑 The BUNDLED NETS are written before the anneal, so they are the earliest artifact this
    # stage produces — a wirelength difference is explained here or it is not an input problem.
    n, problems = compare_nets(case)
    if n == "match":
        nets_ok += 1
    elif n == "differ":
        nets_differ += 1
        print(f"  {case:32} NETS DIFFER  {'; '.join(problems[:3])}{' …' if len(problems) > 3 else ''}")
    ref_path = os.path.join(fp_dir, case, "root.fp.txt")
    if not os.path.exists(ref_path):
        print(f"  {case:32} macro-path (no floorplan written)"); macro_path += 1; continue
    odb = os.path.join(odb_dir, case + ".odb")
    if not os.path.exists(odb):
        print(f"  {case:32} NO ODB"); refused += 1; continue
    # ⛔ Per PARENT, like the nets: one `<parent>.fp.txt` upstream, one `# parent` block ours.
    case_dir = os.path.join(fp_dir, case)
    refs = {f[:-len(".fp.txt")]: f for f in os.listdir(case_dir) if f.endswith(".fp.txt")}
    r = subprocess.run(["./target/debug/cluster-dump", "--shaping", "--floorplan"]
                       + flags(case) + [odb], capture_output=True, text=True)
    blocks, current = {}, None
    for line in r.stdout.splitlines():
        if line.startswith("# parent "):
            current = ref_file_name(line[len("# parent "):])
            blocks[current] = []
        elif current is not None:
            blocks[current] += rows(line)
    first, total = None, 0
    for key in sorted(set(refs) | set(blocks)):
        want = rows(io.open(os.path.join(case_dir, refs[key]), encoding="utf-8").read()) if key in refs else None
        got = blocks.get(key)
        if got is None or want is None:
            first = first or f"{key}: {'upstream' if got is None else 'ours'} only"
            continue
        total += len(got)
        if got != want:
            by_name = dict(want)
            diff = next((f"{name}: {geom} != {by_name.get(name)}" for name, geom in got
                         if by_name.get(name) != geom), f"{len(got)} clusters, upstream has {len(want)}")
            first = first or f"{key}: {diff}"
    if first is None:
        print(f"  {case:32} match ({total} clusters, {len(refs)} parents)"); ok += 1
        continue
    differ += 1
    print(f"  {case:32} DIFFERS  {first}")

print(f"\nfloorplan: {ok} match, {differ} differ, {macro_path} macro-path, {refused} refused, of {len(cases)}")
print(f"nets:      {nets_ok} match, {nets_differ} differ")
raise SystemExit(1 if differ or refused or nets_differ else 0)
