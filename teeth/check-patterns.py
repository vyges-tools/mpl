#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Report every mutation whose `find` pattern no longer matches its file — in seconds.

🔑 **Staleness is a TEXT question, not a behavioural one**, so it does not need the sweep. The full
sweep is ~35 minutes on the box and ~90 on the Mac; this is the same answer for the STALE column in
under a second, which is the difference between checking after every refactor and checking never.

⛔ **This is the failure mode it exists for.** On 2026-08-28 an `AreaKind` -> `ClusterType` refactor
silently rotted six push patterns and pointed one `want` at a deleted test. Nothing noticed until
the sweep was re-run, and the sweep is exactly what nobody runs mid-refactor.

⛔ **WHAT IT DOES NOT CATCH, stated so nobody trusts it further than it goes:**

* **`replace`-side rot.** Only `find` is checked against the file. A replacement can go
  uncompilable on its own — `children-placed-before-the-parent` called `place_one_parent` with five
  arguments after the function had grown to seven, and this checker reported it clean. Only the
  sweep sees that, as `DOES NOT COMPILE`.
* **A pattern that still matches but has become EQUIVALENT** — the code changed around it until the
  mutation cannot alter behaviour.
* **A `want` naming a test that no longer exists.** ⚠️ Worse, a STALE PATTERN HIDES THIS: the
  harness never reaches the test name, so it reports `0 no-such-test` while pointing at a deleted
  test. Two mutations were in exactly that state on 2026-08-28.

⟹ A clean run here means the patterns still bite the code. It does not mean the table is sound.

Usage:  python3 teeth/check-patterns.py
"""
import io, os, re, sys

here = os.path.dirname(os.path.abspath(__file__))
root = os.path.dirname(here)
src = io.open(os.path.join(here, "src", "mutations.rs"), encoding="utf-8").read()

# Each entry is `Mutation { name: r#"..."#, file: r#"..."#, find: r#"..."#, ... }`.
FIELD = re.compile(r'(name|file|find|replace|want):\s*r#"(.*?)"#,', re.DOTALL)

entries, cur = [], {}
for key, val in FIELD.findall(src):
    if key in cur:
        entries.append(cur)
        cur = {}
    cur[key] = val
if cur:
    entries.append(cur)

cache, stale, missing, multi = {}, [], [], []
for m in entries:
    if not {"name", "file", "find"} <= m.keys():
        continue
    path = os.path.join(root, m["file"])
    if path not in cache:
        try:
            cache[path] = io.open(path, encoding="utf-8").read()
        except OSError:
            cache[path] = None
    body = cache[path]
    if body is None:
        missing.append((m["name"], m["file"]))
        continue
    n = body.count(m["find"])
    if n == 0:
        stale.append((m["name"], m["file"]))
    elif n > 1:
        # ⚠️ Not an error, but worth knowing: the harness replaces the FIRST occurrence, so a
        # pattern matching twice mutates a site the author may not have meant.
        multi.append((m["name"], n))

for name, f in stale:
    print(f"  STALE      {name:52} pattern not found in {f}")
for name, f in missing:
    print(f"  NO FILE    {name:52} {f}")
for name, n in multi:
    print(f"  ambiguous  {name:52} matches {n}x; the FIRST is mutated")

print(f"\n{len(entries)} mutations: {len(stale)} stale, {len(missing)} missing-file, {len(multi)} ambiguous")
sys.exit(1 if (stale or missing) else 0)
