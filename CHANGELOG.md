# Changelog

All notable changes to jsoned will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- Structural lint (v0.7) — automatic checks on load and after every edit: null values, empty keys `""`, nesting depth > 20; warning rows highlighted orange in Explorer; `Tab`/`Shift+Tab` navigate between warnings; auto-expands collapsed ancestors when jumping; correcting a node clears its warning instantly
- Status bar shows `[N warnings]` count and `⚠ <message>` when cursor is on a warning node

### Fixed
- Malformed JSON / YAML / TOML / CSV now prints a clean error message and exits without breaking the terminal (raw mode is now enabled only after successful parsing)

---

## [0.3.0] — 2026-06-30

### Added
- stdin / stdout pipe mode — `cat file.json | jsoned` reads from stdin (TUI renders on stderr, stdout stays free); `s` writes JSON to stdout and exits; enables `TERAPI_JSON_EDITOR=jsoned` integration
- `W` — save-as dialog: format picker popup (JSON / YAML / TOML / CSV), then Detail panel opens for filename (pre-filled with stem + new extension); Enter saves, Esc returns to format picker
- Conversion warnings: if saving as TOML and the document contains null values, a warning is shown in the status bar before attempting the write
- 3-panel TUI layout: Source (annotated JSON with line numbers), Explorer (key/type/value table), Detail (node preview)
- Inline scalar editing — `e` opens a type dropdown (String / Number / Boolean / Null) then a value editor in the Detail panel
- Type conversion for scalars: String ↔ Number ↔ Boolean ↔ Null, with sensible defaults on type change
- Node actions: delete (`d`), duplicate (`D`), copy (`y`), paste after (`p`) / before (`P`), move up (`K`) / down (`J`)
- `a` — add child to a container (Object or Array); add sibling after the current node when on a scalar
- `r` — rename a key in an Object (Detail panel opens pre-filled; preserves position; rejects duplicate keys)
- `u` / `Ctrl+R` — undo / redo with a 50-level history stack; every destructive operation is captured before it runs
- `S` — sort Object children alphabetically by key
- `E` / `C` — expand all / collapse all recursively from the selected node
- `w` — wrap the selected node in an Array (`[node]`) or Object (`{ "key": node }`); type dropdown shows Array / Object; Object prompts for a key name
- `/` — incremental search across keys and values (case-insensitive); matches highlighted green as you type; `Enter` confirms, `Esc` clears
- `n` / `N` — jump to next / previous search match with wrap-around; match counter shown in status bar (`[3/12]`)
- `gg` / `G` — jump to first / last row
- `s` — save to the original file (JSON pretty-print)
- Modified indicator in status bar when the document has unsaved changes
- Save dialog popup when quitting with unsaved changes (save / quit without saving / cancel)
- `[` / `]` — toggle Source / Detail panels
- `Ctrl+E` — toggle Explorer fullscreen (hides Source and Detail, restores previous state on second press)
- `Ctrl+C` — immediate quit (bypasses the double-`q` confirmation)

### Changed
- CSV export rewritten — dot-notation flattening + 1-level explosion of arrays of objects; arrays of scalars joined with `;`; depth-2+ arrays serialized as JSON strings; root can now be an object or an array
- Selection highlight: teal-blue background (`Indexed(25)`) — replaces the near-invisible dark grey
- Collapse/expand arrows: `▶` / `▼` — replaces `>` / `v`
- `e` restricted to scalar nodes only — pressing `e` on an Object or Array shows a hint instead of opening the editor
- Type dropdown in Edit mode shows only scalar types (String / Number / Boolean / Null) — Object and Array excluded to prevent accidental container destruction
- Quit safety: first `q` shows a confirmation hint; second `q` quits (no changes) or opens save dialog (unsaved changes)
- Status bar split into two lines: line 1 shows `filename [modified]  ·  item.0.current.time` (dot-notation path, terapi-compatible); line 2 shows contextual keybinding hints
- Hint bar (status bar line 2) background: dark grey (`Indexed(236)`) in normal mode, yellow text in edit mode, dark red (`Indexed(52)`) for quit confirmation, cyan for search mode
- Source/Explorer panel ratio changed to 30/70 (was 45/55)
- Panel titles display their toggle key: Source `[[]`, Explorer `[^E]`, Detail `[]`
- Explorer type icons: `{}` Object · `[]` Array · `"` String · `№` Number · `◆` Boolean · `∅` Null — each icon rendered in its type colour (yellow / cyan / green / yellow / magenta / dark-grey)
- Explorer Key column narrowed (42 % → 33 % of panel width) — Type and Value columns shift left by ~10 chars

### Fixed
- Stdin pipe mode crash ("Failed to initialize input reader") on macOS — crossterm's default `mio` backend fails to register `/dev/tty` with kqueue when stdin is a pipe; fix: `crossterm = { version = "0.28", features = ["use-dev-tty"] }` which uses `filedescriptor` + `poll()` directly
- Source panel scroll stabilized at startup — anchors on the selected node without jumping on large documents
- `Esc` in normal mode now clears transient status messages and resets to filename

---

## [0.1.0] — 2026-06-29

### Added
- TUI viewer — keyboard navigation, fold/unfold objects and arrays, dot-path indicator
- Mutable tree model (`JNode`) separate from `serde_json::Value` — foundation for editing
- Flat render model (`FlatRow` + `flatten()`) for efficient display of large documents
- Format conversion — JSON ↔ YAML ↔ TOML ↔ CSV (interactive and headless)
- Headless mode — `jsoned file.yaml --to json [--output file]`
- CLI via `clap` — file argument, `--to`, `--output` flags
