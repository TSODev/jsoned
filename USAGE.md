# jsoned — Usage Guide

## Table of Contents

- [Installation](#installation)
- [Opening a file](#opening-a-file)
- [Navigation](#navigation)
- [Folding and unfolding](#folding-and-unfolding)
- [Editing values](#editing-values)
- [Editing structure](#editing-structure)
- [Undo and redo](#undo-and-redo)
- [Search](#search)
- [Saving](#saving)
- [Format conversion](#format-conversion)
  - [Interactive](#interactive)
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

## Navigation

`↑` / `↓` or `j` / `k` move the cursor one row at a time.
`PgUp` / `PgDn` jump 20 rows. The status bar always shows the dot-path of the selected node.

## Folding and unfolding

Press `Enter` or `Space` on an object or array to toggle it collapsed.
Collapsed nodes show an inline preview: `{…} (3 fields)` or `[…] (5 items)`.

## Editing values

Press `i` on a scalar node to open the inline editor (pre-filled with the current value).
`Enter` confirms, `Esc` cancels.

## Editing structure

| Key | Action |
|-----|--------|
| `r` | Rename the selected key (objects only) |
| `a` | Add a field / element after the cursor |
| `d` | Delete the selected node |

## Undo and redo

`u` undoes the last change. `Ctrl+R` redoes it. History is per-session and not persisted.

## Search

`/` opens a search bar. Type to filter nodes by key or value (case-insensitive).
`n` / `N` jump to the next / previous match. `Esc` closes search.

## Saving

| Key | Action |
|-----|--------|
| `w` | Save to the original file (same format) |
| `W` | Save as — opens a format picker |

## Format conversion

### Interactive

Press `W` to open the save-as dialog and choose a target format.

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

| Key | Action |
|-----|--------|
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `PgUp` / `PgDn` | Jump 20 rows |
| `Enter` / `Space` | Fold / unfold |
| `i` | Edit selected value |
| `r` | Rename selected key |
| `a` | Add field / element |
| `d` | Delete selected node |
| `u` | Undo |
| `Ctrl+R` | Redo |
| `/` | Search |
| `n` / `N` | Next / previous match |
| `w` | Save |
| `W` | Save as |
| `q` | Quit |
