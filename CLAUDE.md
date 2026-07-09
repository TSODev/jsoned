# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`jsoned` — a keyboard-driven TUI (ratatui + crossterm) for viewing and editing JSON, with
structural editing, undo/redo, search, lint, a `jq` plugin, and format conversion between JSON,
YAML, TOML, CSV, and JSONL. Tagline: "`jless`, but you can actually edit things." Single-binary
crate (no `lib.rs` — everything is `mod`s under `src/`, wired up in `src/main.rs`). By TSODev,
sibling projects `terapi` and `rowdy-db`.

## Commands

```sh
cargo build --release        # release build, ~2 min cold
cargo test                    # unit tests only, runs in ~0.02s, no external services needed
cargo test <substring>        # run a single test or module, e.g. `cargo test patch_flat`
cargo test --release bench -- --ignored --nocapture   # run the two #[ignore]'d perf benchmarks
cargo clippy --release        # currently 3 pre-existing warnings (collapsible_match in app/mod.rs,
                               # too_many_arguments in ui.rs::render_row) — not a regression signal
```

There is no `tests/` integration-test directory, no CI workflow (`.github/` doesn't exist), no
`justfile`/`Makefile`, and no `.cursorrules`/Cursor/Copilot instruction files. All ~81 tests are
`#[cfg(test)] mod tests`/`patch_tests` blocks inline in the source files they test
(`tree.rs`, `pretty.rs`, `lint.rs`, `plugin.rs`, `redact.rs`, `diff.rs`, `app/mod.rs`). None require
network, Docker, env vars, or any other external infra — they're pure in-memory `JNode` tree
assertions. The two `#[ignore]`d tests (`src/bench.rs::bench_full_rebuild_vs_patch` and one in
`app/mod.rs`) are manual-timing perf benchmarks (`Instant`-based, not criterion — see the doc
comment at the top of `bench.rs` for why: this is a binary-only crate, so an external `benches/`
harness would only see `pub` items and couldn't reach `App`'s private methods).

`dev-dependencies` (`fake`, `indicatif`, `uuid`, `chrono`) exist solely for
`examples/json_perf_gen.rs`, a synthetic large-JSON generator used to produce the numbers in
BENCHMARK.md — never linked into the shipped binary.

## Architecture

### The core split: `JNode` tree vs. derived render views

Everything revolves around `JNode` (`src/tree.rs`) — a hand-rolled mutable tree
(`Object{IndexMap<String,JNode>,collapsed}` / `Array{Vec<JNode>,collapsed}` / `Scalar`), kept
deliberately separate from `serde_json::Value` so cursor position, fold state, and undo history
can live on the tree itself. `JScalar::Number` stores the raw string (not a parsed number) to
preserve source formatting like `1.0` vs `1`. A `JPath` (`Vec<JKey>`, `JKey::Field(Rc<str>)` /
`JKey::Index(usize)`) addresses any node; `JKey::Field` is `Rc<str>` specifically so cloning a path
(done constantly across flatten/annotate/lint/diff) is a refcount bump, not a string copy.

`App` (`src/app/mod.rs`, ~2000 lines, by far the largest file) owns the `JNode` root plus three
things *derived* from it on every load and every edit:
- `flat: Vec<FlatRow>` (`tree::flatten`) — one row per visible tree node, drives the Explorer table
  and cursor movement
- `annotated: Vec<AnnotatedLine>` (`pretty::annotate`) — one line per rendered JSON-source line
  with per-segment syntax color, drives the Source panel
- `lint_warnings: Vec<LintWarning>` (`lint::lint`) — empty keys, depth > 20, drives the orange
  warning highlighting and `Tab`/`Shift+Tab` navigation

**The performance-critical pattern to know before touching any edit path**: naive full-rebuild of
these three derived vectors after every keystroke was the dominant cost on large documents (see
CHANGELOG.md `[0.5.0]` — 1375ms full load down to ~0.1ms per patched edit on a 350k-row file).
Instead, `App::refresh_at(&self, target: &JPath, affects_annotated: bool)` calls
`tree::patch_flat` / `pretty::patch_annotated` / `lint::patch_lint`, each of which splices only the
contiguous block belonging to `target`'s subtree (found via a `position()` scan, since the flat
list is a pre-order DFS so a subtree is always contiguous) rather than re-walking the whole tree.
Every mutating method in `app/mod.rs` follows the same three-step shape: `push_undo(&target)` →
mutate `self.root` at that path → `refresh_at(&target, affects_annotated)`. When adding a new edit
operation, follow this shape — don't call `flatten()`/`annotate()`/`lint()` directly unless the
edit is genuinely non-local (the codebase's own example: `jump_to_lint`/`expand_ancestors` can
un-collapse multiple non-contiguous ancestors in one call, so those *do* fall back to full
rebuilds, and this is called out explicitly rather than silently approximated).
`lint::patch_lint` returns `false` (meaning "couldn't patch, caller must fall back to full
`lint()`") only in the one case a sparse warning list can't handle incrementally: a brand-new
warning appearing in a subtree that had none before.

Undo/redo (`UndoEntry { target: JPath, root: JNode }`) reuses this same `refresh_at` patching on
pop, but still does one full `JNode::clone()` of the whole tree per push (to populate whichever
stack didn't just get the new entry) — this is a known, documented, not-yet-optimized cost at
scale (see CHANGELOG `[0.5.0]` "Caveat" paragraph); don't assume undo/redo is as cheap as a normal
edit when reasoning about performance.

### Module map (why each exists, not just what it's named)

- `tree.rs` — `JNode`/`JPath`/`flatten`/patch machinery described above. Heavily unit-tested
  (`patch_tests` module) by asserting every patched result equals a full `flatten()` rebuild.
- `pretty.rs` — `annotate()`, the JSON-source pretty-printer with per-segment `SegColor`
  (Key/Str/Num/Bool/Null/Punct) tagging; each `AnnotatedLine` carries the `JPath` of its owning
  node so the UI can highlight every source line under the cursor.
- `lint.rs` — structural checks (empty object keys, depth > `MAX_DEPTH` (20)); same
  patch-vs-full-rebuild split as `tree.rs`.
- `convert.rs` — format parse/serialize (`parse_any`/`serialize_to`) for json/yaml/toml/csv/jsonl,
  all funneled through `serde_json::Value` as the intermediate representation before becoming a
  `JNode`. JSONL is modeled as "one JSON value per line ↔ a single JSON array" — no special-casing
  anywhere else in the tree/flatten/annotate/lint/patch machinery once parsed. CSV export uses
  dot-notation flattening + 1-level array explosion.
- `redact.rs` — mask-only (never delete) sensitive values before export; two passes sharing one
  key list — exact key-name match (whole value → `***REDACTED***`) and inline `name=value` regex
  match inside string values (e.g. a query-string `api_key=` embedded in a URL). Only touches the
  exported copy, never `self.root` — used by both headless `--redact` and the TUI Save-As flow.
- `diff.rs` — structural (key-path, not line-based) diff between two `JNode` trees, so reordered
  keys or reformatted whitespace produce no noise, and the two sides can even be different formats
  (`a.json --diff b.yaml`, each parsed independently by its own extension). Documented v1
  limitation in the module doc comment: array comparison is naive index-alignment, not LCS —
  inserting an element at the front of an array shows every later element as "changed".
- `diff_app.rs` — separate, read-only TUI (`DiffApp`) for interactive diff viewing; deliberately
  not sharing `App`'s edit/undo/search machinery.
- `plugin.rs` — `Plugin` trait, deliberately narrow (`JNode` in, string arg, `JNode` out) because
  that's what `jq` needs; `registry()` is a compiled-in `Vec<Box<dyn Plugin>>`, not a dynamic
  loader. The bundled `jq` plugin runs via the pure-Rust `jaq` crate family
  (`jaq-core`/`jaq-std`/`jaq-json`) specifically so no external `jq` binary is required.
- `event.rs` — thin wrapper turning `crossterm::event::poll`/`read` into `AppEvent::{Key,Tick}`
  on a 250ms tick.
- `ui.rs` — pure rendering (`ratatui`), 3-panel layout: Source (left, 30%) / Explorer+Detail
  (right, 65/35 vertical split). No app-state mutation happens here.
- `app/mod.rs` — `App` struct, all key-handling (`handle_key` + per-mode dispatchers
  `handle_key_edit`/`handle_key_search`/`handle_key_save_as`/`handle_key_plugin`), and `tui_loop`
  (the render/poll/handle loop, called from `run()`).
- `bench.rs` — `#[cfg(test)]`-gated only (declared `#[cfg(test)] mod bench;` in `main.rs`), zero
  cost in the release binary; manual `Instant`-timing benchmarks, not criterion (see Commands
  section for why).

### Entry point / modes (`src/main.rs`)

`Cli` (clap) dispatches to one of four mutually-exclusive modes before any terminal state is
touched (parse errors must print cleanly without leaving the terminal broken):
1. `--diff <b>` with `--to text|json` → `diff::diff_file_headless` (no TUI)
2. `--diff <b>` alone → `diff_app::run` (read-only diff TUI)
3. `--to <fmt>` → `convert::convert_file` (headless conversion, respects `--redact`)
4. otherwise → `app::run`, the main TUI; if stdin is piped and no file arg was given, stdin is
   read fully first and the TUI renders on **stderr** instead of stdout (so stdout stays clean for
   `s`-to-save-and-exit output) — this is what makes `TERAPI_JSON_EDITOR=jsoned` work as a
   drop-in external editor for `terapi`.

`install_panic_hook()` restores the terminal (disables raw mode, leaves alternate screen on both
stdout and stderr) before the default panic report prints — added after a real bug (CHANGELOG
`[0.5.1]`) where a panic mid-TUI left the terminal garbled until the user ran `reset`/`stty sane`.
Any new panic-capable code path added to the TUI relies on this hook already being installed at
the top of `main()`.

## Known pitfall from CHANGELOG history

`crossterm` is pinned with `features = ["use-dev-tty"]` in Cargo.toml. This isn't cosmetic: without
it, crossterm's default `mio` backend fails to register `/dev/tty` with kqueue when stdin is a pipe,
crashing stdin pipe mode (`cat file.json | jsoned`) on macOS with "Failed to initialize input
reader" (CHANGELOG `[0.3.0]`). Don't drop or downgrade that feature when touching dependencies.

## Format support matrix

JSON (key order preserved), YAML, TOML (no `null` support — TOML docs containing null get a
save-time warning), CSV (object/array root; dot-notation flatten on export), JSONL (array root;
each line = one array element). All five are equally valid inputs to every mode (open, `--to`,
both sides of `--diff`), detected by file extension, or `json` when reading stdin.
