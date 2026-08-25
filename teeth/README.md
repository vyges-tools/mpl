# teeth — the mutation harness

Break one rule, name the test that must notice.

```
cargo run --manifest-path teeth/Cargo.toml            # every batch
cargo run --manifest-path teeth/Cargo.toml -- shaping # one batch
cargo run --manifest-path teeth/Cargo.toml -- halo    # any substring of a name or file
cargo run --manifest-path teeth/Cargo.toml -- --batches
```

⛔ **A test that cannot FAIL proves nothing.** Each entry in `src/mutations.rs` breaks one rule
and names the test that must catch it. Five outcomes, deliberately distinct:

| Outcome | Meaning |
| --- | --- |
| `caught` | the named test failed. The rule is pinned |
| `WRONG TEST` | the suite went red, the named test passed. The rule is covered by *something*; fix the expectation |
| `NOT CAUGHT` | the suite stayed green. A real hole |
| `STALE PATTERN` | the pattern did not match. Nothing was measured |
| `DOES NOT COMPILE` | the mutated source does not build. Red, and nothing was measured |

## Why this is Rust and not a shell script

Every defect the shell version had came from the shell, and each produced **a green result over
work that never happened**:

- the table was a list of bash double-quoted strings, so a raw `"` ended its row early and
  word-split the rest. A 183-row sweep silently ran **106** and printed `0 holes`.
  ⟹ The table is a typed array; a stray quote is a compile error.
- the verdict came from grepping `cargo test`'s log, and an aborted binary truncates that log at a
  point decided by thread scheduling — the same mutation classified two ways on two runs.
  ⟹ The verdict is the **exit code of one named test, run alone**.
- a mutation that did not compile went red and read as covered. ⟹ Its own outcome.
- a `kill -9` skipped the restore trap. ⟹ The backup is a file on disk, so the next run repairs
  what a killed one could not.

⚠️ This crate has **no dependency on `vyges-mpl`, and no dependencies at all**. A harness that
stops building when the code under test is broken reports nothing at the moment it matters most.
