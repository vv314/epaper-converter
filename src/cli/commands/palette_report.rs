use anyhow::{Context, Result};
use std::path::Path;

use crate::cli::args::PaletteReportArgs;
use crate::pipeline::{palette_histogram_exact, palette_histogram_nearest, prepare_image};

use super::{palette_label, ratio};

pub(in crate::cli) fn run(args: PaletteReportArgs) -> Result<()> {
    let PaletteReportArgs {
        source,
        rendered,
        width,
        height,
        resize_mode,
        auto_rotate,
    } = args;

    let source_img = prepare_image(
        Path::new(&source),
        width,
        height,
        resize_mode,
        auto_rotate,
        1.0,
    )?;
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
    println!(
        "{:<8} {:>12} {:>12} {:>12}",
        "Color", "source %", "output %", "delta pp"
    );

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

    Ok(())
}
