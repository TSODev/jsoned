# Changelog

All notable changes to jsoned will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [0.1.0] — 2026-06-29

### Added
- TUI viewer — keyboard navigation, fold/unfold objects and arrays, dot-path indicator
- Mutable tree model (`JNode`) separate from `serde_json::Value` — foundation for editing
- Flat render model (`FlatRow` + `flatten()`) for efficient display of large documents
- Format conversion — JSON ↔ YAML ↔ TOML ↔ CSV (interactive and headless)
- Headless mode — `jsoned file.yaml --to json [--output file]`
- CLI via `clap` — file argument, `--to`, `--output` flags
