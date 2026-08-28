#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Assert that every upstream case is either SCORED or EXCEPTED BY NAME.

🔑 **The point is the denominator.** Every other gate in this engine scores the set it enumerates
for itself, so `34/34` is true and says nothing about the 38 cases upstream actually ships. This
one compares against the upstream directory and fails on anything unclassified.

⛔ **A case in neither set is a FAILURE, not a skip.** Without that, an exception list is
documentation, and documentation rots — this engine's mutation table went stale under a refactor
and nothing noticed until the sweep was re-run.

⚠️ **An exception that no longer corresponds to an upstream case is ALSO a failure.** A stale
entry silently shrinks the denominator, which is the same defect pointing the other way.

Usage:  python3 scripts/check-case-coverage.py <upstream-mpl-test-dir> [odb-dir]
"""
import io, os, re, sys

if len(sys.argv) < 2:
    sys.exit(__doc__.strip().splitlines()[-1])
up_dir = sys.argv[1]
odb_dir = sys.argv[2] if len(sys.argv) > 2 else os.path.expanduser("~/vyges-test/mpl-scope-probe")
here = os.path.dirname(os.path.abspath(__file__))

shipped = {f[:-4] for f in os.listdir(up_dir) if f.endswith(".tcl")}
if not shipped:
    sys.exit(f"no .tcl cases in {up_dir} — wrong directory?")
scored = {f[:-4] for f in os.listdir(odb_dir) if f.endswith(".odb")}

CODES = {
    "needs-feature",
    "refused-by-design",
    "upstream-bug",
    "oracle-unavailable",
    "architecture",
    "not-a-case",
}

# ⛔ **`architecture` is the only code we grant OURSELVES**, so it is the only one with evidence
# requirements the script can check. Everything else points at something outside us — an unbuilt
# feature, an upstream bug, a decision on the record. This one says "the reference tests something
# our design has no equivalent of", and the programme has been wrong that way before: `ppl`'s
# annealing was written off as unreproducible on principle and the premise was never checked.
#
# ⚠️ The script cannot judge whether the claim is TRUE. It can insist the claim is CHECKABLE:
# that it names where the architectural difference is written down, and what would falsify it.
REGISTER = re.compile(r"\bclass\s+[A-E]\d?\b", re.IGNORECASE)
FALSIFIER = re.compile(r"\bfalsifi|\bwould make it applicable|\bwrong if\b", re.IGNORECASE)

excepted, reasons, bad = set(), {}, []
for lineno, line in enumerate(
    io.open(os.path.join(here, "upstream-exceptions.txt"), encoding="utf-8"), 1
):
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    parts = line.split(None, 2)
    if len(parts) < 3:
        bad.append(f"line {lineno}: needs `<case>  <code>  <detail>`")
        continue
    case, code, detail = parts
    if code not in CODES:
        bad.append(f"line {lineno}: unknown code {code!r} — one of {sorted(CODES)}")
        continue
    if code == "architecture":
        if not REGISTER.search(detail):
            bad.append(
                f"{case}: `architecture` must cite a divergence-register class "
                f"(e.g. 'class D2'). An unwritten difference is not established enough "
                f"to except a test on."
            )
        if not FALSIFIER.search(detail):
            bad.append(
                f"{case}: `architecture` must say what would FALSIFY it. "
                f"A claim with no falsifier is how `ppl`'s annealing was written off."
            )
    if code == "upstream-bug" and "#" not in detail:
        bad.append(f"{case}: `upstream-bug` must cite the issue number")
    if code == "needs-feature" and len(detail) < 20:
        bad.append(f"{case}: `needs-feature` must NAME the feature — it is the burn-down")
    excepted.add(case)
    reasons.setdefault(code, []).append(case)

unclassified = shipped - scored - excepted
stale = excepted - shipped
# ⚠️ A case both scored AND excepted is a contradiction: it says the engine cannot do something it
# demonstrably does. It reads as harmless and quietly overstates the exception list.
both = scored & excepted

for case in sorted(unclassified):
    print(f"  ⛔ {case:38} UNCLASSIFIED — score it or except it by name")
for case in sorted(stale):
    print(f"  ⛔ {case:38} STALE exception — no such upstream case")
for case in sorted(both):
    print(f"  ⛔ {case:38} scored AND excepted")
for message in bad:
    print(f"  ⛔ {message}")

print(f"\nshipped {len(shipped)} = scored {len(scored & shipped)} + excepted {len(excepted)}")
for code in sorted(reasons):
    print(f"  {code:20} {len(reasons[code]):3}  {', '.join(sorted(reasons[code]))}")
burn = len(reasons.get("needs-feature", []))
arch = len(reasons.get("architecture", []))
print(f"\nburn-down (needs-feature): {burn}")
# ⚠️ Reported separately and always, even at zero. A growing `architecture` count is the shape of
# an engine quietly excusing itself from the reference rather than converging on it.
print(f"self-granted (architecture): {arch}")
sys.exit(1 if (unclassified or stale or both or bad) else 0)
