# vyges-mpl

Hierarchical macro placement over the [OpenDB](https://github.com/vyges-tools/opendb) database:
clustering, coarse shaping, simulated-annealing placement, boundary push, and track snapping.

OpenROAD's `mpl` is the **reference implementation**. The rules here are re-derived from its
source and written against our own database and geometry layers — same algorithm and same
parameter semantics, so a disagreement with OpenROAD is a bug rather than a difference of
approach. No code is copied.

## Status

✅ **The pipeline is complete and correlated against OpenROAD's own regression suite** — clustering,
coarse shaping, the annealing search, cluster and macro placement, the boundary push, orientation
improvement with its wirelength model, the commit stage, track snapping and the clustering groups.

Correlation is measured per stage against the reference tool's own debug traces and golden DEFs,
over its 36-design suite:

| Oracle | Result |
| --- | --- |
| physical hierarchy · design report | 34 / 34 byte-exact each |
| coarse shaping · boundary push · orientation | byte-exact on every case that emits a trace |
| cluster and macro placement | every case that reaches the path |
| **golden DEFs** — macro positions, temporary standard cells, halo blockages, clustering groups | **34 / 34 exact** |

⚠️ **Every score is scoped to one upstream commit**, recorded by `--describe`. A score quoted
without its pin says nothing: the reference moves.

⛔ **A green suite is a weaker claim than it looks — see Scope.**

## Scope

⛔ **TritonPart partitioning is not implemented.** Upstream splits a *flat* cluster — one with no
module children — whose leaf standard cells exceed the level threshold by calling `par`. This
engine **refuses rather than approximating** when it meets one.

That path is reached by **none of upstream's own 36 regression cases**, so the suite is fully
scoreable without it — which is exactly why a passing suite overstates readiness. A real design with
a large flat block still needs it, and this engine will refuse rather than guess. `--describe`
publishes the limit as part of the contract, so a caller can test for it rather than discover it.

## Usage

```sh
vyges-mpl --describe     # machine-readable contract
vyges-mpl --help         # usage, exit codes, limits
```

Exit status is the verdict: `0` applied · `1` refused · `2` usage/IO · `3` vacuous.

## Licence

Apache-2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
