import io, os, subprocess, sys
odb_dir = sys.argv[1]
fp_dir = os.path.join(odb_dir, "fp")
cases = sys.argv[2:] if len(sys.argv) > 2 else sorted(os.listdir(fp_dir))
for case in cases:
    ref = os.path.join(fp_dir, case, "root.cost.txt")
    odb = os.path.join(odb_dir, case + ".odb")
    if not os.path.exists(ref) or not os.path.exists(odb):
        continue
    want = [tuple(map(float, l.split())) for l in io.open(ref) if len(l.split()) == 2]
    r = subprocess.run(["./target/debug/cluster-dump", "--shaping", "--cost", odb],
                       capture_output=True, text=True)
    got = [tuple(map(float, l.split())) for l in r.stdout.splitlines() if len(l.split()) == 2]
    if len(got) != len(want):
        print(f"{case:28} LENGTH {len(got)} vs {len(want)}")
        continue
    first = None
    for i, (g, w) in enumerate(zip(got, want)):
        # temperature is deterministic; the COST is the trajectory
        if abs(g[1] - w[1]) > 1e-4 * max(1.0, abs(w[1])):
            first = i
            break
    if first is None:
        print(f"{case:28} match ({len(got)} steps)")
    else:
        print(f"{case:28} diverges at step {first}: cost {got[first][1]} vs {want[first][1]}"
              f"   (T {got[first][0]} vs {want[first][0]})")
