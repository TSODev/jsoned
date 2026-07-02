use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

mod app;
mod tree;
mod ui;
mod event;
mod convert;
mod pretty;
mod lint;
mod plugin;
mod diff;
mod diff_app;
mod redact;
#[cfg(test)]
mod bench;

#[derive(Parser, Debug)]
#[command(name = "jsoned", version, about = "Keyboard-driven TUI JSON viewer and editor")]
struct Cli {
    /// File to open (JSON, YAML, TOML, CSV, JSONL)
    file: Option<PathBuf>,

    /// Convert to format and exit (json, yaml, toml, csv, jsonl) — no TUI
    #[arg(long, value_name = "FORMAT")]
    to: Option<String>,

    /// Output file for conversion (default: stdout)
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Compare `file` against this second file (structural, key-path diff) — opens a read-only
    /// diff TUI, or with --to text|json prints a headless diff report and exits
    #[arg(long, value_name = "FILE")]
    diff: Option<PathBuf>,

    /// Comma-separated key names to mask (case-insensitive, applied recursively anywhere in the
    /// tree) before headless conversion — e.g. --redact password,apiKey,token. Values are
    /// replaced with "***REDACTED***"; key names and document shape are preserved.
    #[arg(long, value_name = "KEYS", value_delimiter = ',')]
    redact: Vec<String>,
}

fn main() -> Result<()> {
    use std::io::{IsTerminal, Read};

    let cli = Cli::parse();

    // Diff mode: jsoned a.json --diff b.json [--to text|json]
    if let Some(ref file_b) = cli.diff {
        let file_a = cli.file.as_ref()
            .ok_or_else(|| anyhow::anyhow!("--diff requires a file argument"))?;
        return if let Some(ref fmt) = cli.to {
            diff::diff_file_headless(file_a, file_b, fmt, cli.output.as_deref())
        } else {
            diff_app::run(file_a, file_b)
        };
    }

    // Headless conversion mode: jsoned input.yaml --to json
    if let Some(ref fmt) = cli.to {
        let input = cli.file.as_ref().ok_or_else(|| anyhow::anyhow!("input file required for conversion"))?;
        return convert::convert_file(input, fmt, cli.output.as_deref(), &cli.redact);
    }

    // Stdin pipe mode: cat file.json | jsoned  (only when no file arg given)
    let stdin_content = if cli.file.is_none() && !std::io::stdin().is_terminal() {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Some(buf)
    } else {
        None
    };

    // TUI mode
    app::run(cli.file, stdin_content)
}
