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
- [ ] Wrap node in Object / Array
- [x] `a` on a scalar — add sibling after current node (Object: key + value; Array: value only)

---

## v0.5 — Search + navigation

- [ ] `/` — search by key or value (case-insensitive, highlighted)
- [ ] `n` / `N` — next / previous match with wrap
- [ ] Jump to root (`gg`) / end (`G`)

---

## v0.6 — Save as + format conversion TUI

- [ ] `W` — save-as dialog with format picker (json / yaml / toml / csv)
- [ ] Conversion warnings inline (e.g. null values incompatible with TOML)
- [ ] Open a second format from an already-open document

---

## v0.7 — JSON Schema validation

- [ ] `--schema schema.json` CLI flag — load a JSON Schema
- [ ] Validation errors shown inline (red highlight on failing nodes)
- [ ] Error list panel (`e` to toggle)
- [ ] Schema auto-detection from `$schema` field in document

---

## v0.8 — Advanced features

- [ ] **Semantic diff** — compare two files side-by-side (`jsoned --diff a.json b.json`)
- [ ] **jq filter** — `/` prefix runs a jq expression, results shown in a preview panel
- [ ] **Multi-tab** — open multiple files, `Tab` to switch
- [ ] **Large file performance** — lazy flatten for documents > 10k nodes
- [ ] **JSONLines** — stream-friendly format (one JSON object per line)

---

## Integration targets

- **terapi** — `TERAPI_JSON_EDITOR=jsoned` opens a response body for editing or inspection
- **$EDITOR fallback** — `jsoned` as a drop-in for `vi`/`nano` when editing JSON configs
- **stdin / stdout** — `cat file.json | jsoned` and `:w` writes to stdout (pipe-friendly)
