# jsoned

Keyboard-driven TUI for viewing and editing JSON files — with format conversion between JSON, YAML, TOML, and CSV.

```
┌ jsoned ──────────────────────────────────────────────────────────────────┐
│  "name": "Alice"                                           [string]      │
│▶ "age": 32                                                 [number]      │
│  ▼ "address":                                              [Object]      │
│      "street": "12 rue de la Paix"                         [string]      │
│      "city": "Paris"                                       [string]      │
│  ▶ "tags": […] (3 items)                                   [Array]       │
└──────────────────────────────────────────────────────────────────────────┘
 user.json  ·  address.city                ↑/↓: navigate  Enter: fold  q: quit
```

## Features

- **Navigate** any JSON structure with keyboard — fold/unfold objects and arrays
- **Edit** scalar values inline, rename keys, add and delete nodes
- **Undo / redo** structural and value changes
- **Convert** between JSON, YAML, TOML, and CSV — interactively or headless
- Dot-path indicator always shows where you are in the tree
- Fast on large files — flat render model, no DOM reflow

## Installation

```sh
cargo install jsoned
```

Requires Rust 1.75+.

## Usage

```sh
jsoned file.json          # open in TUI
jsoned file.yaml          # auto-detect format, open in TUI
jsoned                    # start with an empty JSON object

# Headless conversion — no TUI
jsoned file.yaml --to json
jsoned file.json --to yaml --output converted.yaml
jsoned data.csv  --to json
```

Supported formats: `json`, `yaml`, `toml`, `csv`

## Keybindings

| Key | Action |
|-----|--------|
| `↑` / `↓` or `j` / `k` | Navigate |
| `PgUp` / `PgDn` | Jump 20 rows |
| `Enter` or `Space` | Fold / unfold |
| `i` | Edit selected value |
| `r` | Rename selected key |
| `a` | Add field / element after cursor |
| `d` | Delete selected node |
| `u` | Undo |
| `Ctrl+R` | Redo |
| `w` | Save |
| `W` | Save as (format picker) |
| `/` | Search |
| `q` | Quit |

## Format notes

| Format | Read | Write | Notes |
|--------|------|-------|-------|
| JSON | ✓ | ✓ | Key order preserved |
| YAML | ✓ | ✓ | |
| TOML | ✓ | ✓ | `null` values not supported by TOML |
| CSV | ✓ | ✓ | Root must be an array of objects |

## Author

Thierry Soulie — TSODev — <thierry.soulie@tsodev.fr>
