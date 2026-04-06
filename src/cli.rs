use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use crate::pipeline::{
    check_epaper_format, choose_halftone_mode, indices_to_rgb_image, palette_histogram_exact,
    palette_histogram_nearest, prepare_image, resize_with_mode, save_bin_buffer,
    save_packed_buffer,
};
use crate::quantize::{quantize_atkinson, quantize_bayer, quantize_blue_noise, quantize_yliluoma};

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
        /// Halftone mode
        #[arg(short = 'm', long = "halftone", value_enum, default_value = "bayer")]
        halftone: HalftoneMode,
        /// Resize strategy for fitting image into the target canvas
        #[arg(long, value_enum, default_value = "contain")]
        resize_mode: ResizeMode,
        /// Apply EXIF orientation before resizing
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        auto_rotate: bool,
        /// Apply a power-law gamma curve after resizing (1.0 keeps original; <1 brightens, >1 darkens)
        #[arg(long, default_value = "1.0")]
        gamma: f32,
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
    /// Compare source and converted palette occupancy ratios
    PaletteReport {
        /// Source image path
        source: String,
        /// Converted image path
        rendered: String,
        /// Target width used before conversion
        #[arg(short, long, default_value = "800")]
        width: u32,
        /// Target height used before conversion
        #[arg(short = 'H', long, default_value = "480")]
        height: u32,
        /// Resize strategy used before conversion
        #[arg(long, value_enum, default_value = "cover")]
        resize_mode: ResizeMode,
        /// Apply EXIF orientation before resizing source image
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        auto_rotate: bool,
    },
}

#[derive(Default, Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum HalftoneMode {
    /// Bayer ordered dithering - cleaner and more stable on e-paper panels
    #[default]
    Bayer,
    /// Blue noise dithering - finer and less structured texture on gradients
    BlueNoise,
    /// Yliluoma ordered dithering - palette-aware mixing tuned for limited color panels
    Yliluoma,
    /// Atkinson dithering - sharper diffusion with less gray haze than Floyd
    Atkinson,
    /// Choose Bayer, Yliluoma, or Atkinson automatically based on image complexity
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
    /// Packed 4-bit display buffer - two pixels per byte, ready for Waveshare driver display()
    Packed,
    /// PNG image
    Png,
    /// Both BMP and BIN
    Both,
}

#[inline(always)]
fn halftone_mode_label(mode: HalftoneMode) -> &'static str {
    match mode {
        HalftoneMode::Bayer => "Bayer ordered dithering",
        HalftoneMode::BlueNoise => "Blue noise dithering",
        HalftoneMode::Yliluoma => "Yliluoma ordered dithering",
        HalftoneMode::Atkinson => "Atkinson dithering",
        HalftoneMode::Auto => "auto",
    }
}

#[inline(always)]
fn palette_label(idx: usize) -> &'static str {
    match idx {
        0 => "black",
        1 => "white",
        2 => "red",
        3 => "yellow",
        4 => "blue",
        5 => "green",
        _ => unreachable!(),
    }
}

#[inline(always)]
fn ratio(count: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
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
            halftone,
            resize_mode,
            auto_rotate,
            gamma,
            format,
            benchmark,
        } => {
            let total_start = Instant::now();

            anyhow::ensure!(gamma.is_finite() && gamma > 0.0, "Gamma must be a finite value greater than 0");

            if !benchmark {
                println!("Loading: {}", input);
            }
            let load_start = Instant::now();

            let rgb_img =
                prepare_image(Path::new(&input), width, height, resize_mode, auto_rotate, gamma)?;

            let load_time = load_start.elapsed();

            let resolved_halftone = match halftone {
                HalftoneMode::Auto => choose_halftone_mode(&rgb_img),
                mode => mode,
            };

            if !benchmark {
                let mode_str = halftone_mode_label(resolved_halftone);
                if halftone == HalftoneMode::Auto {
                    println!("Halftone strategy: auto -> {}", mode_str);
                }
                println!("Converting ({} mode)...", mode_str);
            }
            let convert_start = Instant::now();

            let indices = match resolved_halftone {
                HalftoneMode::Bayer => quantize_bayer(&rgb_img, width, height),
                HalftoneMode::BlueNoise => quantize_blue_noise(&rgb_img, width, height),
                HalftoneMode::Yliluoma => quantize_yliluoma(&rgb_img, width, height),
                HalftoneMode::Atkinson => quantize_atkinson(&rgb_img, width, height),
                HalftoneMode::Auto => unreachable!(),
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
                OutputFormat::Packed => {
                    save_packed_buffer(&indices, output_path)?;
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
            let indices_bayer = black_box(quantize_bayer(&rgb_img, width, height));
            let bayer_time = start.elapsed();

            let start = Instant::now();
            black_box(quantize_blue_noise(&rgb_img, width, height));
            let blue_noise_time = start.elapsed();

            let start = Instant::now();
            black_box(quantize_yliluoma(&rgb_img, width, height));
            let yliluoma_time = start.elapsed();

            let start = Instant::now();
            black_box(quantize_atkinson(&rgb_img, width, height));
            let atkinson_time = start.elapsed();

            let start = Instant::now();
            let _rgb_out = indices_to_rgb_image(&indices_bayer, width, height);
            let convert_time = start.elapsed();

            println!("=== Results ({}x{}) ===", width, height);
            println!(
                "Bayer mode:    {:>8.2}ms",
                bayer_time.as_secs_f64() * 1000.0
            );
            println!(
                "Blue noise:    {:>8.2}ms",
                blue_noise_time.as_secs_f64() * 1000.0
            );
            println!(
                "Yliluoma:      {:>8.2}ms",
                yliluoma_time.as_secs_f64() * 1000.0
            );
            println!(
                "Atkinson mode:{:>8.2}ms",
                atkinson_time.as_secs_f64() * 1000.0
            );
            println!(
                "RGB convert:   {:>8.2}ms",
                convert_time.as_secs_f64() * 1000.0
            );
            println!(
                "Total Bayer:  {:>8.2}ms",
                (bayer_time + convert_time).as_secs_f64() * 1000.0
            );
            println!(
                "Total Blue:   {:>8.2}ms",
                (blue_noise_time + convert_time).as_secs_f64() * 1000.0
            );
            println!(
                "Total Yliluoma:{:>7.2}ms",
                (yliluoma_time + convert_time).as_secs_f64() * 1000.0
            );
            println!(
                "Total Atkinson:{:>8.2}ms",
                (atkinson_time + convert_time).as_secs_f64() * 1000.0
            );
        }
        Commands::PaletteReport {
            source,
            rendered,
            width,
            height,
            resize_mode,
            auto_rotate,
        } => {
            let source_img = prepare_image(Path::new(&source), width, height, resize_mode, auto_rotate, 1.0)?;
            let rendered_img = image::open(&rendered)
                .with_context(|| format!("Failed to open rendered image: {rendered}"))?
                .to_rgb8();

            anyhow::ensure!(
                rendered_img.width() == width && rendered_img.height() == height,
                "Rendered image size mismatch: expected {}x{}, got {}x{}",
                width,
                height,
                rendered_img.width(),
                rendered_img.height()
            );

            let source_counts = palette_histogram_nearest(&source_img);
            let (rendered_exact_counts, rendered_invalid) = palette_histogram_exact(&rendered_img);
            let rendered_counts = if rendered_invalid == 0 {
                rendered_exact_counts
            } else {
                palette_histogram_nearest(&rendered_img)
            };
            let total_pixels = (width as u64) * (height as u64);

            let mut total_abs_delta = 0.0f64;
            let mut max_abs_delta = 0.0f64;
            let mut max_abs_delta_idx = 0usize;

            println!("=== Palette Report ===");
            println!("Source:   {}", source);
            println!("Rendered: {}", rendered);
            println!("Canvas:   {}x{} ({:?})", width, height, resize_mode);
            if rendered_invalid > 0 {
                println!(
                    "Rendered non-palette pixels: {} ({:.2}%) -> using nearest-palette projection for comparison",
                    rendered_invalid,
                    ratio(rendered_invalid, total_pixels) * 100.0
                );
            }
            println!();
            println!("{:<8} {:>12} {:>12} {:>12}", "Color", "source %", "output %", "delta pp");

            for idx in 0..6 {
                let source_ratio = ratio(source_counts[idx], total_pixels);
                let rendered_ratio = ratio(rendered_counts[idx], total_pixels);
                let delta_pp = (rendered_ratio - source_ratio) * 100.0;
                let abs_delta_pp = delta_pp.abs();

                total_abs_delta += abs_delta_pp;
                if abs_delta_pp > max_abs_delta {
                    max_abs_delta = abs_delta_pp;
                    max_abs_delta_idx = idx;
                }

                println!(
                    "{:<8} {:>11.2} {:>11.2} {:>+11.2}",
                    palette_label(idx),
                    source_ratio * 100.0,
                    rendered_ratio * 100.0,
                    delta_pp
                );
            }

            println!();
            println!("Total abs delta: {:.2} pp", total_abs_delta);
            println!(
                "Max color delta: {} ({:.2} pp)",
                palette_label(max_abs_delta_idx),
                max_abs_delta
            );
            println!("Interpretation: smaller delta means the output palette occupancy is closer to the source projection.");
        }
    }

    Ok(())
}
