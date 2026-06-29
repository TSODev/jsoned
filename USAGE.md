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
- [Jump navigation](#jump-navigation)
- [Undo and redo](#undo-and-redo)
- [Saving](#saving)
- [Format conversion](#format-conversion)
  - [Headless](#headless)
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
jsoned                 # start with an empty object
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
| `W` | Save as — choose format (JSON / YAML / TOML / CSV), then enter filename |
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

## Format conversion

### Headless

```sh
# stdout
jsoned input.yaml --to json
jsoned input.json --to yaml
jsoned input.json --to toml
jsoned data.csv   --to json

# write to file
jsoned input.yaml --to json --output output.json
```

Supported formats: `json`, `yaml`, `toml`, `csv`

> **TOML** — `null` values are not supported; the conversion will error if the document contains nulls.  
> **CSV export** — accepts a root object or array of objects. Nested objects are flattened with dot-notation keys (`address.city`). The first array-of-objects field at each level is exploded into multiple rows (parent fields repeated). Arrays of scalars are joined with `;`. Deeper arrays of objects are serialized as JSON strings.  
> **CSV import** — produces an array of objects (all values as strings).

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
| `W` | Save as (format conversion) |

### Search

| Key | Action |
|-----|--------|
| `/` | Open search |
| `n` | Next match |
| `N` | Previous match |
| `gg` | Jump to first row |
| `G` | Jump to last row |

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
| `Ctrl+C` | Force quit immediately |
