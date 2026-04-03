use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use crate::pipeline::{
    check_epaper_format, choose_dither_mode, indices_to_rgb_image, prepare_image, resize_with_mode,
    save_bin_buffer,
};
use crate::quantize::{quantize_dithered, quantize_fast};

#[derive(Parser)]
#[command(name = "epaper-converter")]
#[command(
    about = "High-performance image converter for Waveshare 7.3inch e-Paper E (ACeP 6-color)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert an image to e-paper format
    Convert {
        /// Input image path
        input: String,
        /// Output image path
        output: String,
        /// Target width (default: 800)
        #[arg(short, long, default_value = "800")]
        width: u32,
        /// Target height (default: 480)
        #[arg(short = 'H', long, default_value = "480")]
        height: u32,
        /// Dithering mode
        #[arg(short, long, value_enum, default_value = "floyd")]
        dither: DitherMode,
        /// Resize strategy for fitting image into the target canvas
        #[arg(long, value_enum, default_value = "contain")]
        resize_mode: ResizeMode,
        /// Apply EXIF orientation before resizing
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        auto_rotate: bool,
        /// Output format
        #[arg(short, long, value_enum, default_value = "bmp")]
        format: OutputFormat,
        /// Show benchmark timing
        #[arg(short, long)]
        benchmark: bool,
    },
    /// Check if image is already in e-paper format
    Check {
        /// Input image path
        input: String,
        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,
        /// Silent mode (only exit code)
        #[arg(short, long)]
        quiet: bool,
    },
    /// Benchmark the converter with a test image
    Benchmark {
        /// Input image path
        input: String,
        /// Target width
        #[arg(short, long, default_value = "800")]
        width: u32,
        /// Target height
        #[arg(short = 'H', long, default_value = "480")]
        height: u32,
    },
}

#[derive(Default, Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum DitherMode {
    /// No dithering - fastest, good for simple images
    Fast,
    /// Floyd-Steinberg dithering - better quality, slower
    #[default]
    Floyd,
    /// Choose fast or floyd automatically based on image complexity
    Auto,
}

#[derive(Default, Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum ResizeMode {
    /// Stretch to target size exactly
    Stretch,
    /// Preserve aspect ratio and pad with white background
    #[default]
    Contain,
    /// Preserve aspect ratio and crop center area to fill target size
    Cover,
}

#[derive(Default, Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
enum OutputFormat {
    /// Windows Bitmap - good for preview
    Bmp,
    /// Raw binary buffer - one byte per pixel (0-5), directly usable by display
    #[default]
    Bin,
    /// PNG image
    Png,
    /// Both BMP and BIN
    Both,
}

#[inline(always)]
fn dither_mode_label(mode: DitherMode) -> &'static str {
    match mode {
        DitherMode::Fast => "fast (no dithering)",
        DitherMode::Floyd => "Floyd-Steinberg dithering",
        DitherMode::Auto => "auto",
    }
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Convert {
            input,
            output,
            width,
            height,
            dither,
            resize_mode,
            auto_rotate,
            format,
            benchmark,
        } => {
            let total_start = Instant::now();

            if !benchmark {
                println!("Loading: {}", input);
            }
            let load_start = Instant::now();

            let rgb_img =
                prepare_image(Path::new(&input), width, height, resize_mode, auto_rotate)?;

            let load_time = load_start.elapsed();

            let resolved_dither = match dither {
                DitherMode::Auto => choose_dither_mode(&rgb_img),
                mode => mode,
            };

            if !benchmark {
                let mode_str = dither_mode_label(resolved_dither);
                if dither == DitherMode::Auto {
                    println!("Dither strategy: auto -> {}", mode_str);
                }
                println!("Converting ({} mode)...", mode_str);
            }
            let convert_start = Instant::now();

            let indices = match resolved_dither {
                DitherMode::Fast => quantize_fast(&rgb_img, width, height),
                DitherMode::Floyd => quantize_dithered(&rgb_img, width, height),
                DitherMode::Auto => unreachable!(),
            };

            let convert_time = convert_start.elapsed();

            let output_path = Path::new(&output);
            let save_start = Instant::now();

            match format {
                OutputFormat::Bmp => {
                    let rgb_out = indices_to_rgb_image(&indices, width, height);
                    rgb_out
                        .save(output_path)
                        .with_context(|| format!("Failed to save BMP: {}", output))?;
                }
                OutputFormat::Bin => {
                    save_bin_buffer(&indices, output_path)?;
                }
                OutputFormat::Png => {
                    let rgb_out = indices_to_rgb_image(&indices, width, height);
                    rgb_out
                        .save(output_path)
                        .with_context(|| format!("Failed to save PNG: {}", output))?;
                }
                OutputFormat::Both => {
                    let rgb_out = indices_to_rgb_image(&indices, width, height);

                    let bmp_path = output_path.with_extension("bmp");
                    rgb_out
                        .save(&bmp_path)
                        .with_context(|| format!("Failed to save BMP: {}", bmp_path.display()))?;

                    let bin_path = output_path.with_extension("bin");
                    save_bin_buffer(&indices, &bin_path)?;

                    if !benchmark {
                        println!("Saved: {} + {}", bmp_path.display(), bin_path.display());
                    }
                }
            }

            let save_time = save_start.elapsed();
            let total_time = total_start.elapsed();

            if benchmark {
                println!("=== Performance ===");
                println!("Load:    {:>8.2}ms", load_time.as_secs_f64() * 1000.0);
                println!("Convert: {:>8.2}ms", convert_time.as_secs_f64() * 1000.0);
                println!("Save:    {:>8.2}ms", save_time.as_secs_f64() * 1000.0);
                println!("Total:   {:>8.2}ms", total_time.as_secs_f64() * 1000.0);
            } else {
                println!("Done: {} -> {}", input, output);
            }
        }
        Commands::Check {
            input,
            verbose,
            quiet,
        } => match check_epaper_format(Path::new(&input), verbose) {
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
        },
        Commands::Benchmark {
            input,
            width,
            height,
        } => {
            println!("=== Benchmarking {} ===", input);

            let img = image::open(&input).context("Failed to open image")?;
            let rgb_img = resize_with_mode(&img, width, height, ResizeMode::Stretch);

            let start = Instant::now();
            let indices_fast = black_box(quantize_fast(&rgb_img, width, height));
            let fast_time = start.elapsed();

            let start = Instant::now();
            black_box(quantize_dithered(&rgb_img, width, height));
            let dither_time = start.elapsed();

            let start = Instant::now();
            let _rgb_out = indices_to_rgb_image(&indices_fast, width, height);
            let convert_time = start.elapsed();

            println!("=== Results ({}x{}) ===", width, height);
            println!("Fast mode:     {:>8.2}ms", fast_time.as_secs_f64() * 1000.0);
            println!(
                "Dither mode:   {:>8.2}ms",
                dither_time.as_secs_f64() * 1000.0
            );
            println!(
                "RGB convert:   {:>8.2}ms",
                convert_time.as_secs_f64() * 1000.0
            );
            println!(
                "Total fast:    {:>8.2}ms",
                (fast_time + convert_time).as_secs_f64() * 1000.0
            );
            println!(
                "Total dither: {:>8.2}ms",
                (dither_time + convert_time).as_secs_f64() * 1000.0
            );
        }
    }

    Ok(())
}
