# jsoned — Roadmap

## Vision

The goal is to be the tool that fills the gap left by every existing JSON viewer:
a fast, keyboard-first TUI that lets you **read, edit, and convert** JSON without
leaving the terminal — and without installing a GUI editor just to change one value.

The secondary goal is to be usable as an **external editor** from other terminal tools
(e.g. `TERAPI_JSON_EDITOR=jsoned` from [terapi](https://github.com/TSODev/terapi)).

---

## v0.1 — Viewer *(current)*

- [x] Mutable tree model (`JNode`) — foundation for editing
- [x] Flat render model — efficient display of large documents
- [x] Keyboard navigation — `↑/↓`, `PgUp/PgDn`, fold/unfold
- [x] Dot-path indicator in status bar
- [x] Format conversion headless — JSON ↔ YAML ↔ TOML ↔ CSV
- [x] CLI — file argument, `--to`, `--output`

---

## v0.2 — Scalar editing

- [ ] Inline editor for scalar values (`i` — pre-filled `tui-textarea`, `Enter` confirms)
- [ ] Type-aware validation on save (number format, bool values)
- [ ] Save to original file (`w`) — same format as input
- [ ] Modified indicator in status bar
- [ ] Quit confirmation when unsaved changes exist

---

## v0.3 — Structural editing + undo

- [ ] Rename a key (`r`)
- [ ] Add a field or array element after cursor (`a`)
- [ ] Delete selected node (`d`) with confirmation
- [ ] Undo / redo — history stack per session (`u` / `Ctrl+R`)
- [ ] Move a field up / down within an object (`K` / `J`)
- [ ] Duplicate a node (`D`)

---

## v0.4 — Search + navigation

- [ ] `/` — search by key or value (case-insensitive, highlighted)
- [ ] `n` / `N` — next / previous match with wrap
- [ ] Jump to root (`gg`) / end (`G`)
- [ ] Expand all / collapse all (`E` / `C`)

---

## v0.5 — Save as + format conversion TUI

- [ ] `W` — save-as dialog with format picker (json / yaml / toml / csv)
- [ ] Conversion warnings inline (e.g. null values incompatible with TOML)
- [ ] Open a second format from an already-open document

---

## v0.6 — JSON Schema validation

- [ ] `--schema schema.json` CLI flag — load a JSON Schema
- [ ] Validation errors shown inline (red highlight on failing nodes)
- [ ] Error list panel (`e` to toggle)
- [ ] Schema auto-detection from `$schema` field in document

---

## v0.7 — Advanced features

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
