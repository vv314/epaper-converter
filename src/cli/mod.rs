mod args;
mod commands;

use anyhow::Result;
use clap::Parser;

use args::{Cli, Commands};
pub use args::{DitherMode, ResizeMode};

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Convert(args) => commands::run_convert(args),
        Commands::Check(args) => commands::run_check(args),
        Commands::Benchmark(args) => commands::run_benchmark(args),
        Commands::PaletteReport(args) => commands::run_palette_report(args),
    }
}
