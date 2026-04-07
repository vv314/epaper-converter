use anyhow::{Context, Result};
use std::path::Path;
use std::time::Instant;

use crate::cli::args::{ConvertArgs, HalftoneMode, OutputFormat};
use crate::pipeline::{indices_to_rgb_image, prepare_image, save_bin_buffer, save_packed_buffer};
use crate::quantize::{quantize_atkinson, quantize_bayer, quantize_blue_noise, quantize_yliluoma};

use super::halftone_mode_label;

pub(in crate::cli) fn run(args: ConvertArgs) -> Result<()> {
    let ConvertArgs {
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
    } = args;

    let total_start = Instant::now();

    anyhow::ensure!(
        gamma.is_finite() && gamma > 0.0,
        "Gamma must be a finite value greater than 0"
    );

    if !benchmark {
        println!("Loading: {}", input);
    }
    let load_start = Instant::now();

    let rgb_img = prepare_image(
        Path::new(&input),
        width,
        height,
        resize_mode,
        auto_rotate,
        gamma,
    )?;

    let load_time = load_start.elapsed();

    if !benchmark {
        let mode_str = halftone_mode_label(halftone);
        println!("Converting ({} mode)...", mode_str);
    }
    let convert_start = Instant::now();

    let indices = match halftone {
        HalftoneMode::Bayer => quantize_bayer(&rgb_img, width, height),
        HalftoneMode::BlueNoise => quantize_blue_noise(&rgb_img, width, height),
        HalftoneMode::Yliluoma => quantize_yliluoma(&rgb_img, width, height),
        HalftoneMode::Atkinson => quantize_atkinson(&rgb_img, width, height),
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

    Ok(())
}
