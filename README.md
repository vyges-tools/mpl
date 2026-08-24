# vyges-mpl

Hierarchical macro placement over the [OpenDB](https://github.com/vyges-tools/opendb) database:
clustering, coarse shaping, simulated-annealing placement, boundary push, and track snapping.

OpenROAD's `mpl` is the **reference implementation**. The rules here are re-derived from its
source and written against our own database and geometry layers — same algorithm and same
parameter semantics, so a disagreement with OpenROAD is a bug rather than a difference of
approach. No code is copied.

## Status

🔨 **Early.** The stage pipeline, the verdict vocabulary and the odb write path are in place;
the placement algorithm is not. `vyges-mpl run` is not implemented yet.

## Scope

⛔ **TritonPart partitioning is not implemented.** Upstream splits a *flat* cluster — one with no
module children — whose leaf standard cells exceed the level threshold by calling `par`. This
engine **refuses rather than approximating** when it meets one.

That path is reached by none of upstream's own `mpl` regression cases, so the suite is scoreable
without it — which makes a passing suite a weaker claim than it looks. A real design with a large
flat block still needs it. `--describe` states the limit.

## Usage

```sh
vyges-mpl --describe     # machine-readable contract
vyges-mpl --help         # usage, exit codes, limits
```

Exit status is the verdict: `0` applied · `1` refused · `2` usage/IO · `3` vacuous.

## Licence

Apache-2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
