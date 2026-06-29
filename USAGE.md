# jsoned — Usage Guide

## Table of Contents

- [Installation](#installation)
- [Opening a file](#opening-a-file)
- [Layout](#layout)
- [Navigation](#navigation)
- [Folding and unfolding](#folding-and-unfolding)
- [Editing values](#editing-values)
- [Editing structure](#editing-structure)
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
│             │  Explorer (key/type/val) │
│   Source    ├──────────────────────────┤
│  (JSON +    │                          │
│ line nums)  │   Detail (node preview)  │
│             │                          │
└─────────────┴──────────────────────────┘
│ status bar · breadcrumb · hints        │
```

- **Source** — annotated JSON with line numbers; highlights the selected node. Toggle with `[`.
- **Explorer** — key / type / value table; main interaction surface.
- **Detail** — JSON preview of the selected node; becomes the value editor during `e`. Toggle with `]`.

## Navigation

`↑` / `↓` or `k` / `j` move the cursor one row at a time.  
`PgUp` / `PgDn` jump 20 rows.  
The status bar always shows the breadcrumb path of the selected node.

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
| `a` | Add a child to the selected Object or Array |
| `d` | Delete the selected node |
| `D` | Duplicate the selected node (inserted immediately after) |
| `K` | Move the selected node up within its parent |
| `J` | Move the selected node down within its parent |
| `y` | Copy the selected node to the clipboard |
| `p` | Paste clipboard after the selected node |
| `P` | Paste clipboard before the selected node |
| `S` | Sort Object children alphabetically by key |
| `E` | Expand all — recursively unfold the selected node and its descendants |
| `C` | Collapse all — recursively fold the selected node and its descendants |

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
> **CSV** — export requires the root to be an array of objects; import produces an array of objects.

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
| `a` | Add child to selected container |
| `d` | Delete selected node |
| `D` | Duplicate selected node |
| `K` | Move node up |
| `J` | Move node down |
| `y` | Copy node |
| `p` | Paste after |
| `P` | Paste before |
| `S` | Sort Object children by key |
| `E` | Expand all from selected |
| `C` | Collapse all from selected |
| `u` | Undo |
| `Ctrl+R` | Redo |

### File

| Key | Action |
|-----|--------|
| `s` | Save |

### View

| Key | Action |
|-----|--------|
| `[` | Toggle Source panel |
| `]` | Toggle Detail panel |

### Quit

| Key | Action |
|-----|--------|
| `q` | Quit (press twice if no unsaved changes; save dialog if modified) |
| `Ctrl+C` | Force quit immediately |
