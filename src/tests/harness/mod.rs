mod config;
mod model;
mod regression;
mod render;
mod report;

use anyhow::{Context, Result};
use image::RgbImage;
use std::fs;
use std::path::PathBuf;

use crate::cli::DitherMode;
use crate::pipeline::{indices_to_rgb_image, palette_histogram_exact, palette_histogram_nearest};
use crate::quantize::{
    quantize_atkinson, quantize_bayer, quantize_blue_noise, quantize_clustered_dot,
    quantize_floyd_steinberg, quantize_yliluoma,
};

pub(super) use config::{
    TempImageFile, DEFAULT_GAMMA, FIXTURE_NAMES, GAMMA_CASES, HARNESS_DITHER_CASES, TARGET_HEIGHT,
    TARGET_WIDTH,
};
pub(super) use model::{
    BaselineEntry, ModeAggregateSummary, PaletteReportSummary, RankedCandidate,
    RegressionComparison, RegressionStatus, RenderRequest, RenderedFixture,
};
pub(super) use regression::{
    build_baseline_snapshot, compare_against_baseline, format_regression_report,
};
pub(super) use render::{render_fixture_to_output, render_gamma_sweep, render_standard_suite};
pub(super) use report::{
    format_leaderboard, format_mode_summary, format_recommendations, format_suite_report,
};

fn prune_output_dir_for_requests(requests: &[RenderRequest]) -> Result<()> {
    let dir = output_dir();
    fs::create_dir_all(&dir).context("Failed to create output directory")?;

    for request in requests {
        let path = output_path_for_request(request.fixture_name, &request.output_slug);
        if path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove output file: {}", path.display()))?;
        }
    }

    Ok(())
}

fn output_path_for_request(fixture_name: &str, output_slug: &str) -> PathBuf {
    output_dir().join(format!("{fixture_name}.cover.{output_slug}.png"))
}

fn quantize_image(img: &RgbImage, mode: DitherMode) -> Vec<u8> {
    match mode {
        DitherMode::Bayer => quantize_bayer(img, TARGET_WIDTH, TARGET_HEIGHT),
        DitherMode::BlueNoise => quantize_blue_noise(img, TARGET_WIDTH, TARGET_HEIGHT),
        DitherMode::Yliluoma => quantize_yliluoma(img, TARGET_WIDTH, TARGET_HEIGHT),
        DitherMode::Atkinson => quantize_atkinson(img, TARGET_WIDTH, TARGET_HEIGHT),
        DitherMode::FloydSteinberg => quantize_floyd_steinberg(img, TARGET_WIDTH, TARGET_HEIGHT),
        DitherMode::ClusteredDot => quantize_clustered_dot(img, TARGET_WIDTH, TARGET_HEIGHT),
    }
}

fn build_palette_report(source_img: &RgbImage, rendered_img: &RgbImage) -> PaletteReportSummary {
    let source_counts = palette_histogram_nearest(source_img);
    let (rendered_exact_counts, rendered_invalid_pixels) = palette_histogram_exact(rendered_img);
    let rendered_counts = if rendered_invalid_pixels == 0 {
        rendered_exact_counts
    } else {
        palette_histogram_nearest(rendered_img)
    };
    let total_pixels = (TARGET_WIDTH as u64) * (TARGET_HEIGHT as u64);

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

    PaletteReportSummary {
        total_abs_delta,
        max_abs_delta,
        max_abs_delta_color: palette_label(max_abs_delta_idx),
        rendered_invalid_pixels,
    }
}

fn compare_rendered_fixture(lhs: &&RenderedFixture, rhs: &&RenderedFixture) -> std::cmp::Ordering {
    score_key(lhs)
        .partial_cmp(&score_key(rhs))
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn overall_best_candidate(rendered: &[RenderedFixture]) -> Option<RankedCandidate<'_>> {
    rendered
        .iter()
        .min_by(compare_rendered_fixture)
        .map(RankedCandidate::from)
}

fn fastest_candidate(rendered: &[RenderedFixture]) -> Option<RankedCandidate<'_>> {
    rendered
        .iter()
        .min_by(|lhs, rhs| {
            lhs.elapsed_ms
                .cmp(&rhs.elapsed_ms)
                .then_with(|| compare_rendered_fixture(lhs, rhs))
        })
        .map(RankedCandidate::from)
}

fn score_key(case: &RenderedFixture) -> (u64, i64, i64, &'static str, &'static str) {
    (
        case.palette_report.rendered_invalid_pixels,
        (case.palette_report.total_abs_delta * 100.0).round() as i64,
        (case.palette_report.max_abs_delta * 100.0).round() as i64,
        case.gamma_slug,
        dither_mode_slug(case.requested_mode),
    )
}

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

fn dither_mode_slug(mode: DitherMode) -> &'static str {
    match mode {
        DitherMode::Bayer => "bayer",
        DitherMode::BlueNoise => "blue-noise",
        DitherMode::Yliluoma => "yliluoma",
        DitherMode::Atkinson => "atkinson",
        DitherMode::FloydSteinberg => "floyd-steinberg",
        DitherMode::ClusteredDot => "clustered-dot",
    }
}

fn ratio(count: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}

impl<'a> From<&'a RenderedFixture> for RankedCandidate<'a> {
    fn from(value: &'a RenderedFixture) -> Self {
        Self {
            fixture_name: value.fixture_name,
            gamma: value.gamma,
            gamma_slug: value.gamma_slug,
            requested_mode: value.requested_mode,
            resolved_mode: value.resolved_mode,
            elapsed_ms: value.elapsed_ms,
            palette_report: &value.palette_report,
        }
    }
}

fn fixture_path(fixture_name: &str) -> PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join(format!("{fixture_name}.jpg"))
}

fn output_dir() -> PathBuf {
    manifest_dir().join("output")
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
