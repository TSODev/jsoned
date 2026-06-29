# Changelog

All notable changes to jsoned will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- 3-panel TUI layout: Source (annotated JSON with line numbers), Explorer (key/type/value table), Detail (node preview)
- Breadcrumb path indicator in status bar (e.g. `departs › Item[4] › train`)
- Inline scalar editing — `e` opens a type dropdown (String / Number / Boolean / Null) then a value editor in the Detail panel
- Type conversion for scalars: String ↔ Number ↔ Boolean ↔ Null, with sensible defaults on type change
- Node actions: delete (`d`), duplicate (`D`), copy (`y`), paste after (`p`) / before (`P`), move up (`K`) / down (`J`)
- Add child — `a` on a container (Object or Array) opens type selection and inline editor for the new entry
- Save file — `s` writes the document to the original file in JSON pretty-print format
- Modified indicator in status bar when the document has unsaved changes
- Save dialog popup when quitting with unsaved changes (save / quit without saving / cancel)
- Panel toggles — `[` hides/shows Source panel, `]` hides/shows Detail panel
- `Ctrl+C` as immediate quit (bypasses the double-`q` confirmation)
- `r` — rename a key in an Object (Detail panel opens pre-filled with the current key; preserves position; rejects duplicate keys)
- `u` / `Ctrl+R` — undo / redo with a 50-level history stack; every destructive operation is captured before it runs
- `S` — sort Object children alphabetically by key
- `E` / `C` — expand all / collapse all recursively from the selected node

### Changed
- Selection highlight: teal-blue background (Indexed 25) — replaces the near-invisible dark grey
- Collapse/expand arrows: `▶` / `▼` — replaces `>` / `v`
- `e` restricted to scalar nodes only — pressing `e` on an Object or Array shows a hint in the status bar instead of opening the editor
- Type dropdown in Edit mode shows only scalar types (String / Number / Boolean / Null) — Object and Array are excluded to prevent accidental container destruction
- Quit safety: first `q` shows a confirmation hint in the status bar; second `q` quits (no changes) or opens the save dialog (unsaved changes)
- Status bar split into two lines: line 1 shows `filename [modified]  ·  item.0.current.time` (dot-notation path, terapi-compatible); line 2 shows contextual keybinding hints
- Hint bar (status bar line 2) now has a styled background: dark grey (`Indexed(236)`) in normal mode with light grey text, yellow text in edit mode (matches active panel borders), dark red background (`Indexed(52)`) with white text for quit confirmation
- Source/Explorer panel ratio changed to 30/70 (was 45/55)
- Source and Detail panel titles now display their toggle key (`[[]` and `[]`)
- `f` — toggle Explorer fullscreen (hides Source and Detail, restores previous state on second press)

### Fixed
- Source panel scroll stabilized at startup — anchors on the selected node without jumping on large documents or on first load
- `Esc` in normal mode now clears transient status messages (e.g. "cannot delete root") and resets to filename

---

## [0.1.0] — 2026-06-29

### Added
- TUI viewer — keyboard navigation, fold/unfold objects and arrays, dot-path indicator
- Mutable tree model (`JNode`) separate from `serde_json::Value` — foundation for editing
- Flat render model (`FlatRow` + `flatten()`) for efficient display of large documents
- Format conversion — JSON ↔ YAML ↔ TOML ↔ CSV (interactive and headless)
- Headless mode — `jsoned file.yaml --to json [--output file]`
- CLI via `clap` — file argument, `--to`, `--output` flags
