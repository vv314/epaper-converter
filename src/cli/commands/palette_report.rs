use anyhow::{Context, Result};
use image::RgbImage;
use std::path::Path;

use crate::cli::args::PaletteReportArgs;
use crate::pipeline::{palette_histogram_exact, palette_histogram_nearest, prepare_image};

use super::{palette_label, ratio};

#[derive(Debug)]
struct PaletteReportMetrics {
    source_counts: [u64; 6],
    rendered_counts: [u64; 6],
    rendered_invalid: u64,
    total_pixels: u64,
    total_abs_delta: f64,
    max_abs_delta: f64,
    max_abs_delta_idx: usize,
}

fn load_palette_report_images(args: &PaletteReportArgs) -> Result<(RgbImage, RgbImage)> {
    let source_img = prepare_image(
        Path::new(&args.source),
        args.width,
        args.height,
        args.resize_mode,
        args.auto_rotate,
        args.gamma,
    )?;
    let rendered_img = image::open(&args.rendered)
        .with_context(|| format!("Failed to open rendered image: {}", args.rendered))?
        .to_rgb8();

    anyhow::ensure!(
        rendered_img.width() == args.width && rendered_img.height() == args.height,
        "Rendered image size mismatch: expected {}x{}, got {}x{}",
        args.width,
        args.height,
        rendered_img.width(),
        rendered_img.height()
    );

    Ok((source_img, rendered_img))
}

fn build_palette_report(
    source_img: &RgbImage,
    rendered_img: &RgbImage,
    allow_non_palette: bool,
) -> Result<PaletteReportMetrics> {
    let source_counts = palette_histogram_nearest(source_img);
    let (rendered_exact_counts, rendered_invalid) = palette_histogram_exact(rendered_img);

    if !allow_non_palette {
        anyhow::ensure!(
            rendered_invalid == 0,
            "Rendered image contains {} non-palette pixels; rerun with `--allow-non-palette` to compare via nearest-palette projection",
            rendered_invalid
        );
    }

    let rendered_counts = if rendered_invalid == 0 {
        rendered_exact_counts
    } else {
        palette_histogram_nearest(rendered_img)
    };
    let total_pixels = (rendered_img.width() as u64) * (rendered_img.height() as u64);

    let mut total_abs_delta = 0.0f64;
    let mut max_abs_delta = 0.0f64;
    let mut max_abs_delta_idx = 0usize;

    for idx in 0..6 {
        let source_ratio = ratio(source_counts[idx], total_pixels);
        let rendered_ratio = ratio(rendered_counts[idx], total_pixels);
        let abs_delta_pp = ((rendered_ratio - source_ratio) * 100.0).abs();

        total_abs_delta += abs_delta_pp;
        if abs_delta_pp > max_abs_delta {
            max_abs_delta = abs_delta_pp;
            max_abs_delta_idx = idx;
        }
    }

    Ok(PaletteReportMetrics {
        source_counts,
        rendered_counts,
        rendered_invalid,
        total_pixels,
        total_abs_delta,
        max_abs_delta,
        max_abs_delta_idx,
    })
}

pub(in crate::cli) fn run(args: PaletteReportArgs) -> Result<()> {
    let (source_img, rendered_img) = load_palette_report_images(&args)?;
    let metrics = build_palette_report(&source_img, &rendered_img, args.allow_non_palette)?;

    println!("=== Palette Report ===");
    println!("Source:   {}", args.source);
    println!("Rendered: {}", args.rendered);
    println!(
        "Canvas:   {}x{} ({:?}, gamma {:.2})",
        args.width, args.height, args.resize_mode, args.gamma
    );
    println!(
        "Palette:  {}",
        if args.allow_non_palette {
            "allow non-palette pixels via nearest-palette fallback"
        } else {
            "strict exact-palette validation enabled"
        }
    );
    if metrics.rendered_invalid > 0 {
        println!(
            "Rendered non-palette pixels: {} ({:.2}%) -> using nearest-palette projection for comparison",
            metrics.rendered_invalid,
            ratio(metrics.rendered_invalid, metrics.total_pixels) * 100.0
        );
    }
    println!();
    println!(
        "{:<8} {:>12} {:>12} {:>12}",
        "Color", "source %", "output %", "delta pp"
    );

    for idx in 0..6 {
        let source_ratio = ratio(metrics.source_counts[idx], metrics.total_pixels);
        let rendered_ratio = ratio(metrics.rendered_counts[idx], metrics.total_pixels);
        let delta_pp = (rendered_ratio - source_ratio) * 100.0;

        println!(
            "{:<8} {:>11.2} {:>11.2} {:>+11.2}",
            palette_label(idx),
            source_ratio * 100.0,
            rendered_ratio * 100.0,
            delta_pp
        );
    }

    println!();
    println!("Total abs delta: {:.2} pp", metrics.total_abs_delta);
    println!(
        "Max color delta: {} ({:.2} pp)",
        palette_label(metrics.max_abs_delta_idx),
        metrics.max_abs_delta
    );
    println!("Interpretation: smaller delta means the output palette occupancy is closer to the source projection.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_palette_report, load_palette_report_images};
    use crate::cli::args::PaletteReportArgs;
    use crate::cli::ResizeMode;
    use crate::quantize::PALETTE;
    use image::{ImageBuffer, Rgb};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempImagePath {
        path: PathBuf,
    }

    impl TempImagePath {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before unix epoch")
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("epaper_palette_report_{name}_{nanos}.png"));
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempImagePath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[test]
    fn palette_report_uses_requested_gamma_for_source_projection() {
        let source = TempImagePath::new("source_gamma");
        let rendered = TempImagePath::new("rendered_gamma");

        ImageBuffer::from_pixel(1, 1, Rgb([128u8, 128, 128]))
            .save(source.path())
            .unwrap();
        ImageBuffer::from_pixel(1, 1, Rgb(PALETTE[0]))
            .save(rendered.path())
            .unwrap();

        let args = PaletteReportArgs {
            source: source.path().display().to_string(),
            rendered: rendered.path().display().to_string(),
            width: 1,
            height: 1,
            resize_mode: ResizeMode::Cover,
            auto_rotate: false,
            gamma: 2.0,
            allow_non_palette: false,
        };

        let (source_img, rendered_img) = load_palette_report_images(&args).unwrap();
        let metrics = build_palette_report(&source_img, &rendered_img, false).unwrap();

        assert_eq!(
            metrics.source_counts[0], 1,
            "gamma-adjusted source should project to black"
        );
        assert_eq!(metrics.rendered_counts[0], 1);
        assert_eq!(metrics.total_abs_delta, 0.0);
    }

    #[test]
    fn strict_mode_rejects_non_palette_pixels_by_default() {
        let source = ImageBuffer::from_pixel(1, 1, Rgb(PALETTE[0]));
        let rendered = ImageBuffer::from_pixel(1, 1, Rgb([1u8, 2, 3]));

        let err = build_palette_report(&source, &rendered, false).unwrap_err();
        assert!(err.to_string().contains("allow-non-palette"));
    }

    #[test]
    fn allow_non_palette_projection_reports_invalid_pixels() {
        let source = ImageBuffer::from_pixel(1, 1, Rgb(PALETTE[0]));
        let rendered = ImageBuffer::from_pixel(1, 1, Rgb([1u8, 2, 3]));

        let metrics = build_palette_report(&source, &rendered, true).unwrap();
        assert_eq!(metrics.rendered_invalid, 1);
        assert_eq!(metrics.rendered_counts[0], 1);
    }
}
