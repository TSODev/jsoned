# jsoned — Performance benchmarks

This documents the incremental-patch machinery added to jsoned's edit pipeline (`tree::patch_flat`,
`pretty::patch_annotated`, `lint::patch_lint`, and their use in `undo`/`redo`) — what it replaces,
how to reproduce the numbers below, and what's still unoptimized.

## Why this exists

Every edit in jsoned used to trigger a full rebuild of the Explorer/Source panels and lint
warnings — `flatten()` + `annotate()` + `lint()`, each a full walk of the document — regardless of
how small the edit was. On large documents this made editing itself slow, not just opening the
file. The patch machinery documented here makes most edits cost proportional to the size of what
changed, not the size of the whole document.

## Methodology

**No `criterion` / external `benches/` harness.** jsoned is a binary-only crate (no `lib.rs`), so
an external bench file would only ever see `pub` items — several things benchmarked here
(`App`'s private methods: `push_undo`, `undo`, `refresh_at`, ...) are deliberately not part of the
public surface, and splitting into lib+bin just to enable `criterion` would either expose
internals that should stay private or require a non-trivial restructure. Not worth it for a
project this size.

Instead, benchmarks live **inside the crate**, gated behind `#[cfg(test)]` (zero cost/footprint in
the shipped release binary) as `#[test] #[ignore]` functions with manual `std::time::Instant`
timing:

- **`src/bench.rs`** — pure-function benchmarks (`flatten`/`patch_flat`,
  `annotate`/`patch_annotated`, `lint`/`patch_lint`). Runs 3 repeats per size and reports the
  minimum (least affected by transient OS/scheduler noise).
- **`src/app/mod.rs`, `mod bench`** — `App::undo()`'s benchmark lives here specifically because
  `undo`/`push_undo`/`refresh_flat` are private to `App`, only reachable from within `app/mod.rs`
  or its descendant modules (same reasoning as the `undo_redo_tests` correctness suite next to it).

This isn't a statistically rigorous framework (no warm-up iterations, no outlier rejection, no
confidence intervals) — but the differences measured here are 100-250x for the pure patches and
5-7x for `undo`/`redo`, far beyond what timing noise on a single dev machine could produce. Numbers
below were captured on the machine this session ran on; absolute values will differ on other
hardware, but the *relative* speedup and the shape of the results (patch cost roughly flat, full
rebuild cost scaling with document size) should reproduce anywhere.

### Running it

```sh
# Everything:
cargo test --release bench -- --ignored --nocapture

# Just the pure-function patch suite:
cargo test --release bench_full_rebuild_vs_patch -- --ignored --nocapture --test-threads=1

# Just the undo/redo suite:
cargo test --release app::bench::bench_undo_patched_vs_full_rebuild -- --ignored --nocapture --test-threads=1
```

`--test-threads=1` avoids the two suites running concurrently and contending for CPU, which adds
noise to both — use it when you want the cleanest possible numbers; omit it for a quick check.

### Synthetic fixture

Both suites generate the same shape of data: an array of `N` objects, each with 8 fields (a mix of
scalars, a 3-element string array, and a small nested object) — representative of a typical API
response or log-record shape, not a pathological worst case. Sizes tested: 1,000 / 10,000 /
50,000 / 100,000 items.

The single-node edit benchmarked in both suites targets the **last item** in the array — the worst
case for `patch_flat`/`patch_annotated`/`patch_lint`'s remaining O(N) cost (a `position()` scan
that has to traverse nearly the whole `Vec` to find the target), so these are conservative numbers,
not best-case ones.

## Reference results

Captured with `cargo test --release ... --test-threads=1` (isolated runs), single machine, one
session — see caveats above.

### Patch machinery (`flatten`/`annotate`/`lint` vs their `patch_*` counterparts)

| items | full_flatten | full_annotate | full_lint | **full total** | patch_flat | patch_annotated | patch_lint | **patch total** | speedup |
|---|---|---|---|---|---|---|---|---|---|
| 1,000 | 4.68ms | 14.18ms | 2.67ms | **21.53ms** | 0.028ms | 0.059ms | 0.000ms | **0.087ms** | **248x** |
| 10,000 | 57.84ms | 144.53ms | 25.49ms | **227.86ms** | 0.629ms | 0.690ms | 0.000ms | **1.318ms** | **173x** |
| 50,000 | 306.33ms | 749.30ms | 121.51ms | **1177.14ms** | 3.184ms | 2.861ms | 0.000ms | **6.045ms** | **195x** |
| 100,000 | 622.34ms | 1512.49ms | 247.41ms | **2382.24ms** | 6.327ms | 6.198ms | 0.000ms | **12.525ms** | **190x** |

`patch_lint` reads `0.000ms` because the synthetic fixture has no lint violations before or after
the edit — the fast no-op path (`old_range` empty, `new_warnings` empty, nothing to splice), which
is also the overwhelmingly common real-world case (most edits don't introduce or remove a
structural violation).

### `undo`/`redo`

| items | undo (patched, full cost) | full-rebuild equivalent | speedup |
|---|---|---|---|
| 1,000 | 6.312ms | 34.30ms | 5x |
| 10,000 | 62.252ms | 397.20ms | 6x |
| 50,000 | 352.549ms | 1858.82ms | 5x |
| 100,000 | 676.516ms | 3842.23ms | 6x |

**Why only ~5-7x and not ~150-250x like the table above**: `undo()`/`redo()` still do one full
`JNode::clone()` of the whole tree to populate the *other* stack (`self.redo_stack.push(...
root: self.root.clone())`) — a cost that predates this session's patch work and wasn't touched by
it. At small sizes this is negligible; at 100k items, cloning a string-heavy nested tree (many
small `String`/`IndexMap` allocations) takes real time and dominates the now-cheap `refresh_at`
call that follows it. This was caught *by writing this benchmark* — the status bar's timer
initially only wrapped `refresh_at`'s internal `Instant`, so it under-reported `undo`/`redo`'s true
cost (it showed ~28ms on a 50k-record file; the real number, confirmed by this benchmark and a
corrected status-bar timer, is ~350-480ms). Fixed to time the whole `undo`/`redo` call including
the clone. **Optimizing the clone itself (e.g. structural sharing via `Rc`, or diff-based undo
history instead of full snapshots) is a larger, separate project — not part of this work.**

## Real-world TUI numbers (manual, cross-referencing the above)

On a 50,000-record / ~875,000-row generated file (see below):
- Initial load (full parse + flatten + annotate + lint): **~2.6-2.8s** — inherent to opening any
  file this size, not something this patch work changes.
- Single value edit near the end of the document: **~28-48ms**.
- Delete near the end of the document: **~48ms**.
- `undo`: **~460-480ms** (post-fix, honest number — see above).

## Generating a large test file for manual testing

`examples/json_perf_gen.rs` generates synthetic JSON test data matching the shape used in this
benchmark. It's a dev-only example (its dependencies — `fake`, `indicatif`, `uuid`, `chrono` — are
`[dev-dependencies]`, never compiled into the shipped `jsoned` binary):

```sh
cargo run --release --example json_perf_gen -- --count 50000 --format json --output big.json
jsoned big.json
```

`--format json` is required — jsoned can't open `.jsonl` yet (see ROADMAP.md). Use jsoned's own
status bar performance indicator (visible automatically above 5,000 rows) to watch timing live
while editing.

## What's not covered here

- `jump_to_lint`/`expand_ancestors` — still a full rebuild, not benchmarked separately (see
  ROADMAP.md for why, and why it's lower priority).
- Initial file load (parse + full `flatten`/`annotate`/`lint`) is inherently O(N) for any tool and
  isn't something this patch work targets — it's included above only as context.
