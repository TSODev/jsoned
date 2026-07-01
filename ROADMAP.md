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
- [ ] **More plugins** — codegen (struct/type generation for a target language), web import +
      prune (fetch JSON, select a subtree to keep) — see "Plugin system" above; each will likely
      need the `Plugin` trait to grow (non-`JNode` output, async work)
- [ ] **Multi-tab** — open multiple files, `Tab` to switch
- [ ] **Large file performance** — lazy/incremental flatten for documents > 10k nodes. Plan drafted
      (patch `flatten()`/`annotate()` incrementally per edit instead of a full O(N) walk each time)
      — not yet implemented.
- [ ] **JSONLines** — stream-friendly format (one JSON object per line). **Blocked on "Large file
      performance" above**: JSONL's typical use case is huge line-oriented logs/datasets — loading
      one as a giant in-memory `JNode::Array` today would immediately re-expose the exact O(N)
      per-edit cost the lazy-flatten work is fixing. Implement after lazy flatten lands, not before.

---

## Integration targets

- **terapi** — `TERAPI_JSON_EDITOR=jsoned` opens a response body for editing or inspection
- **$EDITOR fallback** — `jsoned` as a drop-in for `vi`/`nano` when editing JSON configs
- **stdin / stdout** ✅ — `cat file.json | jsoned` reads from stdin; `s` writes to stdout and exits (pipe-friendly, `TERAPI_JSON_EDITOR=jsoned`)
