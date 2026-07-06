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

/// Restore the terminal (raw mode + alternate screen) before a panic's default report prints —
/// otherwise a panic while a raw-mode/alternate-screen TUI is active (main TUI or --diff TUI)
/// leaves the terminal in a broken state (garbled input, invisible cursor) until the user runs
/// `reset`/`stty sane`. Both stdout and stderr are cleared since the main TUI's backend depends
/// on stdout_mode (stderr when piped, stdout otherwise) and the panic hook is installed before
/// that's known.
fn install_panic_hook() {
    use crossterm::{execute, terminal::{disable_raw_mode, LeaveAlternateScreen}};

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        let _ = execute!(std::io::stderr(), LeaveAlternateScreen);
        default_hook(panic_info);
    }));
}

fn main() -> Result<()> {
    use std::io::{IsTerminal, Read};

    install_panic_hook();

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
