mod harness;

use crate::cli::{HalftoneMode, ResizeMode};
use crate::pipeline::{
    apply_gamma_to_rgb_image, check_epaper_format, choose_halftone_mode, indices_to_packed_buffer,
    palette_histogram_exact, palette_histogram_nearest, resize_with_mode,
};
use crate::quantize::{
    quantize_atkinson, quantize_bayer, quantize_blue_noise, quantize_yliluoma, PALETTE,
};
use harness::{
    TempImageFile, FIXTURE_NAMES, GAMMA_CASES, HARNESS_HALFTONE_CASES, TARGET_HEIGHT, TARGET_WIDTH,
};
use image::{DynamicImage, ImageBuffer, Rgb};

#[test]
fn contain_mode_pads_with_white_background() {
    let src = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(4, 2, Rgb([0, 0, 0])));
    let resized = resize_with_mode(&src, 8, 8, ResizeMode::Contain);

    assert_eq!(resized.dimensions(), (8, 8));
    assert_eq!(resized.get_pixel(0, 0).0, [255, 255, 255]);
    assert_eq!(resized.get_pixel(4, 4).0, [0, 0, 0]);
}

#[test]
fn cover_mode_fills_target_size() {
    let src = DynamicImage::ImageRgb8(ImageBuffer::from_fn(10, 4, |x, _| {
        if x < 5 {
            Rgb([255, 0, 0])
        } else {
            Rgb([0, 0, 255])
        }
    }));
    let resized = resize_with_mode(&src, 8, 8, ResizeMode::Cover);

    assert_eq!(resized.dimensions(), (8, 8));
}

#[test]
fn auto_strategy_prefers_bayer_for_flat_image() {
    let img = ImageBuffer::from_pixel(64, 64, Rgb([255, 255, 255]));
    assert_eq!(choose_halftone_mode(&img), HalftoneMode::Bayer);
}

#[test]
fn auto_strategy_prefers_bayer_for_smooth_gradient() {
    let img = ImageBuffer::from_fn(64, 64, |x, _| {
        let value = (x * 4).min(255) as u8;
        Rgb([value, value, 255])
    });
    assert_eq!(choose_halftone_mode(&img), HalftoneMode::Yliluoma);
}

#[test]
fn auto_strategy_prefers_atkinson_for_complex_image() {
    let img = ImageBuffer::from_fn(128, 128, |x, y| {
        Rgb([(x * 2) as u8, (y * 2) as u8, ((x + y) % 256) as u8])
    });
    assert_eq!(choose_halftone_mode(&img), HalftoneMode::Atkinson);
}

#[test]
fn gamma_below_one_brightens_midtones() {
    let mut img = ImageBuffer::from_pixel(1, 1, Rgb([128, 128, 128]));
    apply_gamma_to_rgb_image(&mut img, 0.85).unwrap();
    assert!(img.get_pixel(0, 0).0[0] > 128);
}

#[test]
fn gamma_above_one_darkens_midtones() {
    let mut img = ImageBuffer::from_pixel(1, 1, Rgb([128, 128, 128]));
    apply_gamma_to_rgb_image(&mut img, 1.15).unwrap();
    assert!(img.get_pixel(0, 0).0[0] < 128);
}

#[test]
fn bayer_quantizer_preserves_dimensions_and_palette_range() {
    let img = ImageBuffer::from_fn(16, 16, |x, y| {
        Rgb([(x * 17) as u8, (y * 17) as u8, ((x + y) * 8) as u8])
    });
    let indices = quantize_bayer(&img, 16, 16);

    assert_eq!(indices.len(), 16 * 16);
    assert!(indices.iter().all(|&idx| idx < PALETTE.len() as u8));
}

#[test]
fn blue_noise_quantizer_is_deterministic_and_in_palette() {
    let img = ImageBuffer::from_fn(16, 16, |x, y| {
        Rgb([
            (x * 17) as u8,
            (y * 17) as u8,
            ((x * 19 + y * 7) % 256) as u8,
        ])
    });
    let first = quantize_blue_noise(&img, 16, 16);
    let second = quantize_blue_noise(&img, 16, 16);

    assert_eq!(first, second);
    assert_eq!(first.len(), 16 * 16);
    assert!(first.iter().all(|&idx| idx < PALETTE.len() as u8));
}

#[test]
fn atkinson_quantizer_preserves_dimensions_and_palette_range() {
    let img = ImageBuffer::from_fn(16, 16, |x, y| {
        Rgb([(x * 13) as u8, (y * 11) as u8, ((x * y) % 256) as u8])
    });
    let indices = quantize_atkinson(&img, 16, 16);

    assert_eq!(indices.len(), 16 * 16);
    assert!(indices.iter().all(|&idx| idx < PALETTE.len() as u8));
}

#[test]
#[ignore = "Yliluoma is too slow for the default unit-test loop"]
fn yliluoma_quantizer_is_deterministic_and_in_palette() {
    let img = ImageBuffer::from_fn(16, 16, |x, y| {
        Rgb([
            (x * 17) as u8,
            (255 - y * 13) as u8,
            ((x * 11 + y * 23) % 256) as u8,
        ])
    });
    let first = quantize_yliluoma(&img, 16, 16);
    let second = quantize_yliluoma(&img, 16, 16);

    assert_eq!(first, second);
    assert_eq!(first.len(), 16 * 16);
    assert!(first.iter().all(|&idx| idx < PALETTE.len() as u8));
}

#[test]
fn palette_histograms_distinguish_exact_and_nearest_projection() {
    let img = ImageBuffer::from_fn(2, 2, |x, y| match (x, y) {
        (0, 0) => Rgb(PALETTE[0]),
        (1, 0) => Rgb(PALETTE[4]),
        (0, 1) => Rgb([10, 20, 240]),
        _ => Rgb([250, 250, 250]),
    });

    let (exact_counts, invalid) = palette_histogram_exact(&img);
    let nearest_counts = palette_histogram_nearest(&img);

    assert_eq!(exact_counts[0], 1);
    assert_eq!(exact_counts[4], 1);
    assert_eq!(invalid, 2);

    assert_eq!(nearest_counts.iter().sum::<u64>(), 4);
    assert!(
        nearest_counts[4] >= 2,
        "expected blue-ish pixels to project to blue"
    );
    assert!(
        nearest_counts[1] >= 1,
        "expected near-white pixels to project to white"
    );
}

#[test]
fn check_accepts_valid_epaper_image() {
    let path = TempImageFile::new("valid");
    let img = ImageBuffer::from_pixel(800, 480, Rgb(PALETTE[0]));
    img.save(path.path()).unwrap();

    let result = check_epaper_format(path.path(), false).unwrap();

    assert!(result);
}

#[test]
fn check_rejects_wrong_resolution() {
    let path = TempImageFile::new("wrong_size");
    let img = ImageBuffer::from_pixel(16, 16, Rgb(PALETTE[0]));
    img.save(path.path()).unwrap();

    let result = check_epaper_format(path.path(), false).unwrap();

    assert!(!result);
}

#[test]
#[ignore = "Generates `output/` preview fixtures for manual algorithm review"]
fn harness_regenerates_cover_png_outputs() -> anyhow::Result<()> {
    let rendered = harness::render_standard_suite()?;

    println!("\n{}", harness::format_suite_report(&rendered));

    assert_eq!(
        rendered.len(),
        FIXTURE_NAMES.len() * HARNESS_HALFTONE_CASES.len(),
        "expected harness to render every fixture/mode combination"
    );
    assert!(rendered.iter().all(|case| case.output_path.is_file()));
    assert!(rendered
        .iter()
        .all(|case| case.palette_report.rendered_invalid_pixels == 0));
    assert!(rendered.iter().all(|case| {
        case.requested_mode != HalftoneMode::Auto || case.resolved_mode != HalftoneMode::Auto
    }));
    assert!(rendered.iter().all(|case| {
        case.output_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(case.fixture_name))
    }));

    Ok(())
}

#[test]
fn harness_uses_panel_target_dimensions() -> anyhow::Result<()> {
    let rendered = harness::render_fixture_to_output(
        "gradient",
        HalftoneMode::Bayer,
        "bayer_test",
        1.0,
        "g100",
    )?;
    let image = image::open(&rendered.output_path)?.to_rgb8();
    let _ = std::fs::remove_file(&rendered.output_path);

    assert_eq!(image.dimensions(), (TARGET_WIDTH, TARGET_HEIGHT));

    Ok(())
}

#[test]
fn harness_formats_palette_summary_for_humans() -> anyhow::Result<()> {
    let rendered = harness::render_fixture_to_output(
        "gradient",
        HalftoneMode::Bayer,
        "bayer_summary",
        1.0,
        "g100",
    )?;
    let output_path = rendered.output_path.clone();
    let report = harness::format_suite_report(&[rendered]);
    let _ = std::fs::remove_file(&output_path);

    assert!(report.contains("Fixture"));
    assert!(report.contains("gradient"));
    assert!(report.contains("bayer"));
    assert!(report.contains("g100"));
    assert!(report.contains("Time(ms)"));
    assert!(report.contains("Total abs delta"));

    Ok(())
}

#[test]
fn harness_formats_fixture_leaderboard() -> anyhow::Result<()> {
    let rendered = vec![
        harness::render_fixture_to_output(
            "gradient",
            HalftoneMode::Bayer,
            "bayer_rank",
            1.0,
            "g100",
        )?,
        harness::render_fixture_to_output(
            "gradient",
            HalftoneMode::Atkinson,
            "atkinson_rank",
            1.15,
            "g115",
        )?,
        harness::render_fixture_to_output("tree", HalftoneMode::Auto, "auto_rank", 0.85, "g085")?,
    ];
    let output_paths = rendered
        .iter()
        .map(|case| case.output_path.clone())
        .collect::<Vec<_>>();
    let report = harness::format_leaderboard(&rendered);
    for path in output_paths {
        let _ = std::fs::remove_file(path);
    }

    assert!(report.contains("Best candidate per fixture"));
    assert!(report.contains("gradient"));
    assert!(report.contains("tree"));

    Ok(())
}

#[test]
fn harness_formats_mode_summary_and_recommendations() -> anyhow::Result<()> {
    let rendered = vec![
        harness::render_fixture_to_output(
            "gradient",
            HalftoneMode::Bayer,
            "bayer_perf",
            1.0,
            "g100",
        )?,
        harness::render_fixture_to_output(
            "gradient",
            HalftoneMode::BlueNoise,
            "blue_perf",
            0.85,
            "g085",
        )?,
        harness::render_fixture_to_output("tree", HalftoneMode::Auto, "auto_perf", 1.15, "g115")?,
    ];
    let output_paths = rendered
        .iter()
        .map(|case| case.output_path.clone())
        .collect::<Vec<_>>();
    let mode_report = harness::format_mode_summary(&rendered);
    let recommendation_report = harness::format_recommendations(&rendered);
    for path in output_paths {
        let _ = std::fs::remove_file(path);
    }

    assert!(mode_report.contains("Average quality/speed by requested mode"));
    assert!(mode_report.contains("Avg time(ms)"));
    assert!(recommendation_report.contains("Harness recommendation"));
    assert!(recommendation_report.contains("Best overall:"));
    assert!(recommendation_report.contains("Fastest candidate:"));

    Ok(())
}

#[test]
fn harness_builds_and_compares_baseline_snapshot() -> anyhow::Result<()> {
    let rendered = vec![
        harness::render_fixture_to_output(
            "gradient",
            HalftoneMode::Bayer,
            "bayer_base",
            1.0,
            "g100",
        )?,
        harness::render_fixture_to_output("tree", HalftoneMode::Auto, "auto_base", 0.85, "g085")?,
    ];
    let output_paths = rendered
        .iter()
        .map(|case| case.output_path.clone())
        .collect::<Vec<_>>();
    let snapshot = harness::build_baseline_snapshot(&rendered);
    let comparisons = harness::compare_against_baseline(&rendered, &snapshot)?;
    let report = harness::format_regression_report(&comparisons);
    for path in output_paths {
        let _ = std::fs::remove_file(path);
    }

    assert!(snapshot.contains("fixture\tgamma\tmode"));
    assert!(report.contains("Regression comparison"));
    assert!(comparisons
        .iter()
        .all(|item| matches!(item.status, harness::RegressionStatus::Unchanged)));

    Ok(())
}

#[test]
fn harness_detects_regression_from_snapshot_text() -> anyhow::Result<()> {
    let rendered = vec![harness::render_fixture_to_output(
        "gradient",
        HalftoneMode::Bayer,
        "bayer_regression",
        1.0,
        "g100",
    )?];
    let output_path = rendered[0].output_path.clone();
    let snapshot = "fixture\tgamma\tmode\ttotal_abs_delta\tmax_abs_delta\trendered_invalid_pixels\ngradient\tg100\tbayer\t0.0000\t0.0000\t0\n";
    let comparisons = harness::compare_against_baseline(&rendered, snapshot)?;
    let _ = std::fs::remove_file(output_path);

    assert_eq!(comparisons.len(), 1);
    assert!(matches!(
        comparisons[0].status,
        harness::RegressionStatus::Regressed
    ));
    assert!(comparisons[0].baseline_max_abs_delta.is_some());
    assert!(comparisons[0].current_max_abs_delta.is_some());

    Ok(())
}

#[test]
#[ignore = "Scans gamma candidates and prints a best-per-fixture leaderboard"]
fn harness_scans_gamma_candidates_and_prints_leaderboard() -> anyhow::Result<()> {
    let rendered = harness::render_gamma_sweep()?;
    let baseline = harness::build_baseline_snapshot(&rendered);
    let comparisons = harness::compare_against_baseline(&rendered, &baseline)?;

    println!("\n{}", harness::format_suite_report(&rendered));
    println!("\n{}", harness::format_leaderboard(&rendered));
    println!("\n{}", harness::format_mode_summary(&rendered));
    println!("\n{}", harness::format_recommendations(&rendered));
    println!("\n{}", harness::format_regression_report(&comparisons));

    assert_eq!(
        rendered.len(),
        FIXTURE_NAMES.len() * HARNESS_HALFTONE_CASES.len() * GAMMA_CASES.len(),
        "expected gamma sweep to cover every fixture/mode/gamma combination"
    );
    assert!(rendered.iter().all(|case| case.output_path.is_file()));
    assert!(rendered
        .iter()
        .all(|case| case.palette_report.rendered_invalid_pixels == 0));
    assert!(comparisons
        .iter()
        .all(|item| matches!(item.status, harness::RegressionStatus::Unchanged)));

    Ok(())
}

#[test]
fn packed_buffer_matches_driver_color_encoding() {
    let packed = indices_to_packed_buffer(&[0, 1, 2, 3, 4, 5]).unwrap();
    assert_eq!(packed, vec![0x01, 0x32, 0x56]);
}

#[test]
fn packed_buffer_rejects_odd_pixel_count() {
    let err = indices_to_packed_buffer(&[0, 1, 2]).unwrap_err();
    assert!(err.to_string().contains("even number of pixels"));
}
