# jsoned

Keyboard-driven TUI for viewing and editing JSON files — with format conversion between JSON, YAML, TOML, and CSV.

```
┌─ Source [[] ──────┬─── Explorer ──────────────────────────────────────────────┐
│  1  {             │ Key              Type        Value                         │
│  2    "name":..   │ ▼ {}             Object                                   │
│  3    "age": 32,  │   "name"         string      Alice                        │
│  4    "address":{ │ ▶ "age"          number      32                           │
│  5      "street.. │ ▼ "address"      Object                                   │
│  6      "city":.. │     "street"     string      12 rue de la Paix            │
│                   │     "city"       string      Paris                        │
│                   ├─── Detail [] ─────────────────────────────────────────────┤
│                   │  {} address                                                │
│                   │  {                                                         │
│                   │    "street": "12 rue de la Paix",                         │
│                   │    "city": "Paris"                                         │
└───────────────────┴───────────────────────────────────────────────────────────┘
 user.json  ·  address.city
 e: edit  r: rename  a: add  d: del  D: dup  y: copy  p/P: paste  s: save  q: quit
```

## Features

- **3-panel layout** — Source (annotated JSON), Explorer (key/type/value table), Detail (node preview)
- **Edit** scalar values inline with type conversion (String ↔ Number ↔ Boolean ↔ Null)
- **Rename** keys, **add** / **delete** / **duplicate** / **move** nodes
- **Undo / redo** — 50-level history, captured before every change
- **Copy / paste** nodes within the document
- **Sort** Object children alphabetically; **expand** or **collapse** entire subtrees
- **Convert** between JSON, YAML, TOML, and CSV — interactively or headless
- Dot-path indicator (`address.city`) always shows where you are in the tree
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
| `↑` / `↓` or `k` / `j` | Navigate |
| `PgUp` / `PgDn` | Jump 20 rows |
| `Enter` / `Space` | Fold / unfold |
| `e` | Edit selected scalar |
| `r` | Rename selected key |
| `a` | Add child to container |
| `d` | Delete node |
| `D` | Duplicate node |
| `K` / `J` | Move node up / down |
| `y` / `p` / `P` | Copy / paste after / paste before |
| `S` | Sort Object children by key |
| `E` / `C` | Expand all / collapse all from selected |
| `w` | Wrap node in Array or Object |
| `u` / `Ctrl+R` | Undo / redo |
| `/` | Search by key or value |
| `n` / `N` | Next / previous match |
| `gg` / `G` | Jump to first / last row |
| `s` | Save |
| `[` / `]` | Toggle Source / Detail panel |
| `Ctrl+E` | Toggle Explorer fullscreen |
| `Esc` | Cancel / clear search / clear status |
| `q` | Quit |

See [USAGE.md](USAGE.md) for the full reference.

## Format notes

| Format | Read | Write | Notes |
|--------|------|-------|-------|
| JSON | ✓ | ✓ | Key order preserved |
| YAML | ✓ | ✓ | |
| TOML | ✓ | ✓ | `null` values not supported by TOML |
| CSV | ✓ | ✓ | Root must be an array of objects |

## Author

Thierry Soulie — TSODev — <thierry.soulie@tsodev.fr>
