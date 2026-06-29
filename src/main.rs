use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

mod app;
mod tree;
mod ui;
mod event;
mod convert;

#[derive(Parser, Debug)]
#[command(name = "jsoned", version, about = "Keyboard-driven TUI JSON viewer and editor")]
struct Cli {
    /// File to open (JSON, YAML, TOML, CSV)
    file: Option<PathBuf>,

    /// Convert to format and exit (json, yaml, toml, csv) — no TUI
    #[arg(long, value_name = "FORMAT")]
    to: Option<String>,

    /// Output file for conversion (default: stdout)
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Headless conversion mode: jsoned input.yaml --to json
    if let Some(ref fmt) = cli.to {
        let input = cli.file.as_ref().ok_or_else(|| anyhow::anyhow!("input file required for conversion"))?;
        return convert::convert_file(input, fmt, cli.output.as_deref());
    }

    // TUI mode
    app::run(cli.file)
}
