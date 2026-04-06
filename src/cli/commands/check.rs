use anyhow::Result;
use std::path::Path;

use crate::cli::args::CheckArgs;
use crate::pipeline::check_epaper_format;

pub(in crate::cli) fn run(args: CheckArgs) -> Result<()> {
    let CheckArgs {
        input,
        verbose,
        quiet,
    } = args;

    match check_epaper_format(Path::new(&input), verbose) {
        Ok(is_valid) => {
            if !quiet {
                if is_valid {
                    println!("[OK] Ready for e-paper");
                } else {
                    println!("[NEEDS CONVERSION]");
                }
            }
            std::process::exit(if is_valid { 0 } else { 1 });
        }
        Err(e) => {
            if !quiet {
                eprintln!("Error: {}", e);
            }
            std::process::exit(2);
        }
    }
}
