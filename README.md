# jsoned

[![crates.io](https://img.shields.io/crates/v/jsoned.svg)](https://crates.io/crates/jsoned)
[![license](https://img.shields.io/crates/l/jsoned.svg)](LICENSE)
[![rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

**`jless`, but you can actually edit things.**

Keyboard-driven TUI for viewing and editing JSON files — with full structural editing, undo/redo, search, and format conversion between JSON, YAML, TOML, and CSV.

![jsoned screenshot](https://raw.githubusercontent.com/TSODev/jsoned/main/assets/screenshot.png)

---

## Why not jless / fx / jq?

| Tool | View | Edit | Convert | Pipe mode |
|------|------|------|---------|-----------|
| **jsoned** | ✓ | ✓ full | ✓ | ✓ |
| jless | ✓ | ✗ read-only | ✗ | ✗ |
| fx | ✓ | ✗ filter only | ✗ | ✗ |
| jq | ✗ no TUI | ✗ | partial | ✓ |

---

## Features

- **3-panel layout** — Source (annotated JSON), Explorer (key/type/value table), Detail (node preview)
- **Edit** scalar values inline with type conversion (String ↔ Number ↔ Boolean ↔ Null)
- **Rename** keys, **add** / **delete** / **duplicate** / **move** nodes
- **Wrap** any node in an Array or Object with a single keypress
- **Undo / redo** — 50-level history, captured before every change
- **Copy / paste** nodes within the document
- **Sort** Object children alphabetically; **expand** or **collapse** entire subtrees
- **Search** — `/` to search keys and values, `n`/`N` to navigate matches
- **Save as** any supported format with `W` — format picker + filename editor in-TUI
- **Pipe mode** — `cat file.json | jsoned` reads stdin; `s` writes JSON to stdout and exits
- **Headless conversion** — `jsoned file.yaml --to json` with no TUI
- Dot-path indicator (`steps.0.extracted`) always shows where you are in the tree
- Fast on large files — flat render model, no DOM reflow

See [USAGE.md](USAGE.md) for the full reference. See [What's New](CHANGELOG.md) for the release history.

---

## Installation

```sh
cargo install jsoned
```

Requires Rust 1.75+.

---

## Usage

```sh
jsoned file.json          # open in TUI
jsoned file.yaml          # auto-detect format, open in TUI
jsoned                    # start with an empty JSON object

# Stdin / stdout pipe mode
cat file.json | jsoned              # read from stdin, TUI on stderr
cat file.json | jsoned > out.json   # edit then s to write to stdout and exit
TERAPI_JSON_EDITOR=jsoned terapi …  # drop-in external editor integration

# Headless conversion — no TUI
jsoned file.yaml --to json
jsoned file.json --to yaml --output converted.yaml
jsoned data.csv  --to json
```

Supported formats: `json`, `yaml`, `toml`, `csv`

---

## Keybindings

| Key | Action |
|-----|--------|
| `↑` / `↓` or `k` / `j` | Navigate |
| `PgUp` / `PgDn` | Jump 20 rows |
| `gg` / `G` | Jump to first / last row |
| `Enter` / `Space` | Fold / unfold |
| `e` | Edit selected scalar |
| `r` | Rename selected key |
| `a` | Add child (container) or sibling (scalar) |
| `w` | Wrap node in Array or Object |
| `d` | Delete node |
| `D` | Duplicate node |
| `K` / `J` | Move node up / down |
| `y` / `p` / `P` | Copy / paste after / paste before |
| `S` | Sort Object children by key |
| `E` / `C` | Expand all / collapse all from selected |
| `u` / `Ctrl+R` | Undo / redo |
| `/` | Search by key or value |
| `n` / `N` | Next / previous match |
| `s` | Save |
| `W` | Save as (format picker + filename editor) |
| `[` / `]` | Toggle Source / Detail panel |
| `Ctrl+E` | Toggle Explorer fullscreen |
| `Esc` | Cancel / clear search / clear status |
| `q` | Quit |

---

## Format notes

| Format | Read | Write | Notes |
|--------|------|-------|-------|
| JSON | ✓ | ✓ | Key order preserved |
| YAML | ✓ | ✓ | |
| TOML | ✓ | ✓ | `null` values not supported by TOML |
| CSV | ✓ | ✓ | Export: dot-notation flatten + 1-level explosion; root can be object or array |

---

## Author

Thierry Soulie — TSODev — <thierry.soulie@tsodev.fr>
