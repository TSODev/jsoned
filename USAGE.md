# jsoned — Usage Guide

## Table of Contents

- [Installation](#installation)
- [Opening a file](#opening-a-file)
- [Layout](#layout)
- [Navigation](#navigation)
- [Folding and unfolding](#folding-and-unfolding)
- [Editing values](#editing-values)
- [Editing structure](#editing-structure)
- [Search](#search)
- [Structural lint](#structural-lint)
- [Plugins](#plugins)
- [Jump navigation](#jump-navigation)
- [Undo and redo](#undo-and-redo)
- [Saving](#saving)
- [Redact on export](#redact-on-export)
- [Format conversion](#format-conversion)
  - [Headless](#headless)
- [Diff mode](#diff-mode)
  - [Headless diff](#headless-diff)
- [Keybinding reference](#keybinding-reference)

---

## Installation

```sh
cargo install jsoned
```

## Opening a file

```sh
jsoned file.json       # JSON
jsoned file.yaml       # YAML
jsoned file.toml       # TOML
jsoned data.csv        # CSV → JSON array of objects
jsoned data.jsonl      # JSONL → JSON array, one element per line
jsoned                 # start with an empty object

# Stdin / stdout pipe mode
cat file.json | jsoned          # read from stdin, TUI on stderr
cat file.json | jsoned > out.json   # edit, s to write to stdout and exit

# External editor integration
TERAPI_JSON_EDITOR=jsoned terapi …  # jsoned receives body on stdin, returns on stdout
```

## Layout

jsoned uses a 3-panel layout:

```
┌─────────────┬──────────────────────────┐
│             │  Explorer [^E]           │
│   Source    ├──────────────────────────┤
│  (JSON +    │                          │
│ line nums)  │   Detail (node preview)  │
│             │                          │
└─────────────┴──────────────────────────┘
│ filename · dot.path                    │
│ contextual hints                       │
```

- **Source [[]** — annotated JSON with line numbers; highlights the selected node. Toggle with `[`.
- **Explorer [^E]** — key / type / value table; main interaction surface. Toggle fullscreen with `Ctrl+E`.
- **Detail []** — JSON preview of the selected node; becomes the value editor during `e`. Toggle with `]`.

The status bar shows two lines:
- **Line 1** — `filename [modified]  ·  item.0.current.time` — file name and dot-notation path of the selected node (compatible with terapi's path format)
- **Line 2** — contextual keybinding hints for the current mode

On large documents (more than 5,000 flattened rows), Line 1 also shows a performance indicator —
`174873 rows in 526.8ms` — the row count and time spent rebuilding the Explorer/Source panels
after the most recent load or edit. It's hidden below that threshold to avoid clutter on typical
small/medium documents; there's no toggle to force it on or off.

## Navigation

`↑` / `↓` or `k` / `j` move the cursor one row at a time.  
`PgUp` / `PgDn` jump 20 rows.

## Folding and unfolding

Press `Enter` or `Space` on an object or array to toggle it collapsed.  
Collapsed nodes show an inline preview: `{…} (3 fields)` or `[…] (5 items)`.

## Editing values

Press `e` on a **scalar** node (String, Number, Boolean, Null) to open the inline editor:

1. A type dropdown appears — choose String / Number / Boolean / Null with `↑` / `↓`, confirm with `Enter`.
2. The Detail panel becomes an editable text field pre-filled with the current value.
3. `Enter` confirms, `Esc` cancels.

Pressing `e` on an Object or Array shows a hint — use `a` / `d` to modify containers.

## Editing structure

| Key | Action |
|-----|--------|
| `r` | Rename the selected key (Object fields only) |
| `a` | Add a child to a container — or a sibling after the current node when on a scalar |
| `d` | Delete the selected node |
| `D` | Duplicate the selected node (inserted immediately after) |
| `K` | Move the selected node up within its parent |
| `J` | Move the selected node down within its parent |
| `y` | Copy the selected node to the clipboard |
| `p` | Paste clipboard after the selected node |
| `P` | Paste clipboard before the selected node |
| `w` | Wrap selected node — choose Array (`[node]`) or Object (`{ "key": node }`) |
| `W` | Save as — choose format, optionally redact keys, then enter filename (see [Redact on export](#redact-on-export)) |
| `S` | Sort Object children alphabetically by key |
| `E` | Expand all — recursively unfold the selected node and its descendants |
| `C` | Collapse all — recursively fold the selected node and its descendants |

## Search

Press `/` to open the search bar. Type to filter — matching rows are highlighted green as you type. The status bar shows `[X/N]` match count.

| Key | Action |
|-----|--------|
| `/` | Open search |
| `Enter` | Confirm and move cursor to current match |
| `Esc` | Cancel search, clear highlights |
| `n` | Jump to next match (wraps) |
| `N` | Jump to previous match (wraps) |

> Search matches visible rows only — collapsed subtrees are not searched.

## Structural lint

jsoned automatically checks the document on load and after every edit. Warning rows are highlighted in **orange** in the Explorer.

**Checks performed:**

| Warning | Cause |
|---------|-------|
| `empty key` | An Object has a key `""` |
| `excessive nesting depth` | A node is nested more than 20 levels deep |

The status bar shows `[N warnings]` when issues are found. When the cursor is on a warning node, the message appears inline: `⚠ null value`.

| Key | Action |
|-----|--------|
| `Tab` | Jump to next warning (expands collapsed ancestors if needed) |
| `Shift+Tab` | Jump to previous warning |

**Inline correction:** fixing a node (editing a null, renaming an empty key, etc.) clears its warning immediately — no manual refresh needed.

---

## Plugins

Press `|` to open the **Plugins** menu — a popup listing available transformations. Select one
with `↑`/`↓` and `Enter`, then type an argument in the prompt that opens in the Detail panel.
`Enter` runs the plugin and **replaces the selected node** with its result; `Esc` at the prompt
goes back to the menu, `Esc` at the menu cancels. If the plugin errors (e.g. a malformed
expression), the error is shown in the status bar and the prompt stays open so you can fix it.
A successful run is a normal edit — `u` undoes it like any other change.

### `jq`

Runs a [jq](https://jqlang.github.io/jq/) filter with the selected node as input (select the
root to run the filter against the whole document). Powered by the bundled
[`jaq`](https://github.com/01mf02/jaq) engine — a pure-Rust jq implementation, so no external
`jq` binary is required.

```
| → jq → .users[].name → Enter
```

If the filter produces more than one output (e.g. `.users[]`), the outputs are collected into a
JSON array and used as the replacement.

### `fake`

Generates fake/random JSON data from a small DSL and replaces the selected node with it — useful
for seeding test structures without leaving the editor. Ignores the selected node's content; the
result comes entirely from the typed expression.

```
| → fake → [10] { name, email, job } → Enter
```

Grammar: an expression is an object `{ field, field: expr, ... }`, an array `[N] expr` (`N` is
always required, no default), or a leaf type. A bare field name is sugar for `field: field` (the
key doubles as the type). Leaves can take optional numeric arguments in parentheses.

| Leaf | Produces | Optional args |
|---|---|---|
| `name`, `first_name`, `last_name`, `username` | identity strings | — |
| `email`, `phone`, `address`, `city`, `country`, `zipcode`, `url` | contact strings | — |
| `job`, `company` | professional strings | — |
| `word` | one word | — |
| `words(n)` | `n`-word phrase (default 3) | `n` |
| `sentence(n)` | `n`-word sentence (default 6) | `n` |
| `paragraph(n)` | `n`-sentence paragraph (default 3) | `n` |
| `number(min,max)` | integer, inclusive range (default 0-1000) | `min,max` |
| `float(min,max)` | float, range (default 0.0-1.0) | `min,max` |
| `bool(pct)` | boolean, `pct`% chance of `true` (default 50) | `pct` |
| `uuid` | a v4 UUID string | — |
| `date(minYear,maxYear)` | date string, random day within the range (default: last 20 years) | `minYear,maxYear` |
| `datetime(minYear,maxYear)` | date + random time of day, same range rules as `date` | `minYear,maxYear` |

English data by default. `email` always resolves to `@example.{com,net,org}` — safe to paste
anywhere, never a real address.

#### Locale (`@fr`)

Append `@fr` to `name`, `first_name`, `last_name`, `phone`, `date`, or `datetime` for French
output, e.g. `name@fr` or `{ name: name@fr, phone: phone@fr }`. Only these six leaves are
supported — the rest of the underlying data (city names, company names, job titles, lorem text)
isn't actually localized for French in the `fake` crate, so `city@fr` is a hard error (`fake
error: 'fr' has no localized data for 'city' — omit @fr`) rather than silently returning English
data under a French label. Any locale other than `fr` (e.g. `name@es`) is also an error — no
other locale is wired up yet.

For `date`/`datetime`, `@fr` only changes the rendered *format* (`%d/%m/%Y` instead of
`%Y-%m-%d`), not the underlying random value — dates aren't culture-specific data the way names
are, and `fake`'s own locale system doesn't localize date formats either (verified: no locale
overrides the format string, so `date` generation is hand-rolled against `chrono` directly rather
than routed through `fake`).

> Plugins are compiled into the binary — there's no dynamic loading of external/third-party
> plugins (yet). Adding one means adding a Rust module that implements the `Plugin` trait.

| Key | Action |
|-----|--------|
| `\|` | Open Plugins menu |
| `↑` / `↓` | Select plugin |
| `Enter` | Confirm selection / run plugin |
| `Esc` | Back to menu (from prompt) / cancel (from menu) |

---

## Jump navigation

| Key | Action |
|-----|--------|
| `gg` | Jump to first row |
| `G` | Jump to last row |

## Undo and redo

| Key | Action |
|-----|--------|
| `u` | Undo the last change (up to 50 levels, per session) |
| `Ctrl+R` | Redo the last undone change |

## Saving

| Key | Action |
|-----|--------|
| `s` | Save to the original file (JSON pretty-print) |

When you quit with unsaved changes, a dialog offers: **[s]** save and quit · **[n]** quit without saving · **[Esc]** cancel.

## Redact on export

Mask sensitive values (API keys, passwords, tokens) when handing a document to someone else,
without ever touching the document you're actively editing. Only the exported copy is affected —
the live document in memory is never mutated, so nothing needs undoing afterward.

**Two ways a name in your list gets masked:**
- **Exact key match** — an object key named e.g. `apiKey` has its entire value replaced with
  `***REDACTED***`. The key itself and the document's shape are preserved.
- **Inline match** — any string value, regardless of which key holds it, is scanned for
  `name=value` occurrences (URL-query-string style) and only the value portion is masked — useful
  for API responses whose pagination/webhook URLs embed a key as a query parameter:
  ```
  "next": "https://api.example.com/items?page=2&api_key=sk-real-secret-value"
  ```
  becomes
  ```
  "next": "https://api.example.com/items?page=2&api_key=***REDACTED***"
  ```
  (the rest of the URL, and the parameter name's original casing, are left untouched).

Matching is case-insensitive on the exact name (`apiKey` matches `APIKEY`), but not fuzzy across
naming conventions — `api_key` and `apiKey` are different strings; list both if both appear.

**In the TUI**: `W` (Save as) now has a step between the format picker and the filename prompt —
type a comma-separated list of names to redact (e.g. `password,apiKey`), or leave it blank and
press `Enter` to skip redaction entirely.

**Headless**:
```sh
jsoned data.json --redact password,apiKey,token --to json -o safe.json
```

## Format conversion

### Headless

```sh
# stdout
jsoned input.yaml --to json
jsoned input.json --to yaml
jsoned input.json --to toml
jsoned data.csv   --to json
jsoned data.jsonl --to json

# write to file
jsoned input.yaml --to json --output output.json
```

Supported formats: `json`, `yaml`, `toml`, `csv`, `jsonl`

> **TOML** — `null` values are not supported; the conversion will error if the document contains nulls.  
> **CSV export** — accepts a root object or array of objects. Nested objects are flattened with dot-notation keys (`address.city`). The first array-of-objects field at each level is exploded into multiple rows (parent fields repeated). Arrays of scalars are joined with `;`. Deeper arrays of objects are serialized as JSON strings.  
> **CSV import** — produces an array of objects (all values as strings).  
> **JSONL import** — one JSON value per line (blank lines skipped) becomes a single JSON array, one element per line; editing then works exactly like any other array. **JSONL export** — an array root writes one element per line; any other root (object or bare scalar) writes as a single line.

---

## Diff mode

```sh
jsoned a.json --diff b.json
```

Opens a separate, **read-only** view showing a **structural, key-path diff** between two files —
not a line-based diff. Each row of the merged tree is tagged `Added` / `Removed` / `Changed` /
`Unchanged`, with the status shown as a leading glyph and a background tint:

| Glyph | Status | Meaning |
|-------|--------|---------|
| `+` | Added | present in the second file only |
| `-` | Removed | present in the first file only |
| `~` | Changed | present in both, value or type differs |
| (none) | Unchanged | identical in both |

A container row (Object/Array) is marked `Changed` if *any* descendant differs, even if the
container itself wasn't added or removed. `a.json` and `b.json` can be different formats — each
side is parsed independently by its own extension (`a.json --diff b.yaml` works).

> **Array comparison is index-aligned, not reorder-aware** (no LCS/diff-of-sequences). Inserting
> an element at the front of an array will show every later element as "changed" rather than as a
> single insertion. This is a known v1 limitation.

| Key | Action |
|-----|--------|
| `j` / `k` or `↓` / `↑` | Move cursor |
| `]` / `n` | Jump to next changed row |
| `[` / `N` | Jump to previous changed row |
| `o` | Toggle hiding `Unchanged` rows |
| `q` / `Esc` | Quit |

Diff mode has no editing, undo, search, or save — it's a dedicated read-only viewer, not a mode of
the main editor.

### Headless diff

```sh
jsoned a.json --diff b.json --to text            # +/-/~ report, one line per changed path
jsoned a.json --diff b.json --to json             # machine-readable, for scripts/CI
jsoned a.json --diff b.json --to json --output report.json
```

Unlike format-conversion `--to` (which accepts `json`/`yaml`/`toml`/`csv`/`jsonl`), diff's `--to`
only accepts `text` or `json` — anything else errors with a clear message. `Unchanged` rows are
never included in headless output (the `text`/`json` reports only list actual differences).

`--to text` format: `+ path: value`, `- path: value`, `~ path: old -> new` (plain ASCII arrow,
not the Unicode one used in the TUI — scripting output shouldn't depend on terminal Unicode
support).

`--to json` format: an array of `{"path", "status", "old", "new"}` objects, e.g.:

```json
[
  { "path": "b.c", "status": "changed", "old": "2", "new": "3" },
  { "path": "e", "status": "added", "old": null, "new": "true" }
]
```

---

## Keybinding reference

### Navigation

| Key | Action |
|-----|--------|
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `PgUp` / `PgDn` | Jump 20 rows |
| `Enter` / `Space` | Fold / unfold container |

### Editing

| Key | Action |
|-----|--------|
| `e` | Edit selected scalar (type dropdown + value editor) |
| `r` | Rename selected key (Object fields only) |
| `a` | Add child (container) or sibling after current node (scalar) |
| `d` | Delete selected node |
| `D` | Duplicate selected node |
| `K` | Move node up |
| `J` | Move node down |
| `y` | Copy node |
| `p` | Paste after |
| `P` | Paste before |
| `w` | Wrap node in Array or Object |
| `S` | Sort Object children by key |
| `E` | Expand all from selected |
| `C` | Collapse all from selected |
| `u` | Undo |
| `Ctrl+R` | Redo |

### File

| Key | Action |
|-----|--------|
| `s` | Save |
| `W` | Save as (format conversion + optional key redaction) |

### Search

| Key | Action |
|-----|--------|
| `/` | Open search |
| `n` | Next match |
| `N` | Previous match |
| `gg` | Jump to first row |
| `G` | Jump to last row |

### Lint

| Key | Action |
|-----|--------|
| `Tab` | Next lint warning |
| `Shift+Tab` | Previous lint warning |

### Plugins

| Key | Action |
|-----|--------|
| `\|` | Open Plugins menu |

### Diff mode

These only apply when jsoned was launched with `--diff` — a separate, read-only view with its own
keybindings (no overlap with the main editor's mode above):

| Key | Action |
|-----|--------|
| `j` / `k` or `↓` / `↑` | Move cursor |
| `]` / `n` | Jump to next changed row |
| `[` / `N` | Jump to previous changed row |
| `o` | Toggle hiding unchanged rows |
| `q` / `Esc` | Quit |
| `Ctrl+C` | Force quit immediately |

### View

| Key | Action |
|-----|--------|
| `[` | Toggle Source panel |
| `]` | Toggle Detail panel |
| `Ctrl+E` | Toggle Explorer fullscreen (hides Source + Detail; press again to restore) |
| `Esc` | Clear search / clear status / cancel confirm-quit |

### Quit

| Key | Action |
|-----|--------|
| `q` | Quit (press twice if no unsaved changes; save dialog if modified) |
| `Ctrl+C` | Force quit immediately — works even inside dialogs, edit prompts, or menus, and restores the terminal cleanly if jsoned ever panics |
