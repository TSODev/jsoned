# jsoned — Roadmap

## Vision

The goal is to be the tool that fills the gap left by every existing JSON viewer:
a fast, keyboard-first TUI that lets you **read, edit, and convert** JSON without
leaving the terminal — and without installing a GUI editor just to change one value.

The secondary goal is to be usable as an **external editor** from other terminal tools
(e.g. `TERAPI_JSON_EDITOR=jsoned` from [terapi](https://github.com/TSODev/terapi)).

---

## v0.1 — Viewer ✅

- [x] Mutable tree model (`JNode`) — foundation for editing
- [x] Flat render model — efficient display of large documents
- [x] Keyboard navigation — `↑/↓`, `PgUp/PgDn`, fold/unfold
- [x] Dot-path indicator in status bar
- [x] Format conversion headless — JSON ↔ YAML ↔ TOML ↔ CSV
- [x] CLI — file argument, `--to`, `--output`

---

## v0.2 — Scalar editing ✅

- [x] 3-panel TUI layout — Source / Explorer / Detail
- [x] Breadcrumb path in status bar
- [x] `e` — inline scalar editor: type dropdown (String / Number / Boolean / Null) + value editor in Detail panel
- [x] Type conversion between scalar types with sensible defaults
- [x] `s` — save to original file (JSON pretty-print)
- [x] Modified indicator in status bar

---

## v0.3 — Node actions ✅

- [x] `a` — add child (Object or Array parent)
- [x] `d` — delete selected node
- [x] `D` — duplicate selected node
- [x] `K` / `J` — move node up / down within its parent
- [x] `y` — copy node to clipboard
- [x] `p` / `P` — paste after / before cursor
- [x] Save dialog popup when quitting with unsaved changes
- [x] Double-`q` confirmation before quit
- [x] `[` / `]` — toggle Source / Detail panels

---

## v0.4 — Structural editing ✅

- [x] `r` — rename a key (objects only)
- [x] `u` / `Ctrl+R` — undo / redo (50-level history stack per session)
- [x] `S` — sort Object children alphabetically by key
- [x] `E` / `C` — expand all / collapse all recursively from selected node
- [x] Wrap node in Object / Array (`w`)
- [x] `a` on a scalar — add sibling after current node (Object: key + value; Array: value only)

---

## v0.5 — Search + navigation ✅

- [x] `/` — search by key or value (case-insensitive, highlighted)
- [x] `n` / `N` — next / previous match with wrap
- [x] Jump to root (`gg`) / end (`G`)

---

## v0.6 — Save as + format conversion TUI ✅

- [x] `W` — save-as dialog with format picker (JSON / YAML / TOML / CSV)
- [x] Conversion warnings inline (null values incompatible with TOML shown in status bar)
- [ ] Open a second format from an already-open document

---

## v0.7 — Structural lint (no schema required) ✅

Lightweight automatic checks with no external schema. Runs on load and after every edit.

- [x] Warn on empty keys `""` (null values are valid JSON — flagged only on TOML export)
- [x] Warn on excessive nesting depth (> 20)
- [x] Warning rows highlighted orange in Explorer; `Tab`/`Shift+Tab` to navigate
- [x] Status bar shows `[N warnings]` count and inline message on cursor node
- [x] Correcting a node clears its warning instantly (inline correction)
- [x] Clean error message on malformed file (raw mode enabled only after successful parse)

---

## Plugin system — jq ✅ (shipped ahead of schedule, supersedes the v0.9 jq filter item below)

An internal, compiled-in plugin registry (`Plugin` trait in `src/plugin.rs`) so filters/transforms
can be added without touching `app`/`ui` logic. Not a dynamic/external plugin system — adding a
plugin still means adding a Rust module and registering it in `plugin::registry()`.

- [x] `Plugin` trait — `run(&self, input: &JNode, arg: &str) -> Result<JNode>`
- [x] `|` — opens a Plugins menu, pick a plugin, type an argument, `Enter` applies the result to
      the selected node (replaces it in place; `u` undoes)
- [x] `jq` plugin — bundled `jaq` (pure-Rust jq clone), keeps the single-binary promise (no
      external `jq` dependency); multiple filter outputs collapse into a JSON array

---

## v0.8 — JSON Schema validation

Approach: JSON Schema (Draft 4→2020-12) via the `jsonschema` crate. Schema loaded from
`--schema file.json` or auto-detected from the `$schema` field in the document.

- [ ] `--schema schema.json` CLI flag — load a JSON Schema
- [ ] Auto-detection of `$schema` field in the document on open
- [ ] Validation run on load and after every edit
- [ ] Failing nodes highlighted red in Explorer; `Tab` navigates between errors
- [ ] Error count shown in status bar (`[2 errors]`)

---

## v0.9 — Advanced features

- [x] **Structural diff** ✅ (shipped ahead of schedule) — `jsoned a.json --diff b.json` opens a
      read-only, key-path diff TUI (not line-based); `--to text|json` for a headless report. See
      `src/diff.rs`/`src/diff_app.rs`. Array comparison is index-aligned, not LCS/reorder-aware —
      a known v1 limitation.
- [x] **Redact on export** ✅ (shipped ahead of schedule) — mask sensitive values (API keys,
      passwords, tokens) when saving a copy for someone else, without touching the document being
      actively edited. Exact object-key match (whole value masked) plus inline `name=value`
      matching inside string values via `regex` (catches secrets embedded as URL query
      parameters). TUI: new step in the `W` (Save as) flow. Headless: `--redact key1,key2`
      alongside `--to`. See `src/redact.rs`.
- [ ] **More plugins** — codegen (struct/type generation for a target language), web import +
      prune (fetch JSON, select a subtree to keep) — see "Plugin system" above; each will likely
      need the `Plugin` trait to grow (non-`JNode` output, async work)
- [ ] **Multi-tab** — open multiple files, `Tab` to switch
- [x] **Large file performance** ✅ (shipped ahead of schedule) — most single-node edits (rename,
      value edit, delete, add, move, paste, toggle collapse, wrap, sort, plugin run, undo, redo)
      patch only the affected subtree's contiguous block instead of a full O(N) rebuild — applied
      to all three per-edit passes (`Vec<FlatRow>` via `tree::patch_flat`, `Vec<AnnotatedLine>` via
      `pretty::patch_annotated`, `Vec<LintWarning>` via `lint::patch_lint` despite that list being
      sparse — pre-order DFS still guarantees contiguity) plus `undo`/`redo` (`UndoEntry` carries
      the same target path each edit already knew at push time, reused symmetrically in both
      directions). In a real TUI session on a 50k-record / ~875k-row file: a single value edit
      near the end of the document ~28-48ms. `undo`/`redo` still do one full `JNode::clone()` of
      the tree for the other stack (pre-existing cost, not part of this session's patch work) —
      dominates at scale, so `undo`/`redo` land at ~6-7x faster than a full rebuild rather than
      the ~100x+ seen in the pure flatten/annotate/lint patches (~460-480ms vs ~2.6s full
      rebuild at 50k records). See `App::refresh_at`, `BENCHMARK.md`. Initial file load is still
      a full parse+flatten+annotate+lint, inherently O(N), same as any tool.
      `jump_to_lint`/`expand_ancestors` remain unpatched — can
      flip multiple non-contiguous ancestors' collapse state in one call, no single obvious
      target; noted as a possible future improvement, low priority (rare navigation action).
- [x] **JSONLines** ✅ — read + write, via `parse_any`/`serialize_to`'s `"jsonl"` hint in
      `convert.rs`. One JSON value per line (blank lines skipped) parses to a single JSON array,
      one element per line — mirrors CSV import's "array of rows" shape, so editing needs zero
      special-casing anywhere in the tree/flatten/annotate/lint/patch machinery, it's just an
      array. Export mirrors `json_to_csv`'s root-handling pattern: an array root writes one
      element per line, any other root (object or bare scalar) writes as a single line — unlike
      CSV, JSONL doesn't require object-shaped rows, so there's no error case. Wired into: opening
      `.jsonl` files directly, headless `--to jsonl`, TUI Save-As (`W`, now 5 formats), and
      `--diff` (works on `.jsonl` like any other format). Initial load of a huge line-oriented
      file is still an O(N) full parse — same cost any format pays for a file that size, not
      JSONL-specific; this was the per-edit cost concern that originally deferred this item, and
      it's resolved by the lazy-flatten work above.

---

## Integration targets

- **terapi** — `TERAPI_JSON_EDITOR=jsoned` opens a response body for editing or inspection
- **$EDITOR fallback** — `jsoned` as a drop-in for `vi`/`nano` when editing JSON configs
- **stdin / stdout** ✅ — `cat file.json | jsoned` reads from stdin; `s` writes to stdout and exits (pipe-friendly, `TERAPI_JSON_EDITOR=jsoned`)
