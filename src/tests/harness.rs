use anyhow::{Context, Result};
use image::RgbImage;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::cli::{HalftoneMode, ResizeMode};
use crate::pipeline::{
    choose_halftone_mode, indices_to_rgb_image, palette_histogram_exact, palette_histogram_nearest,
    prepare_image,
};
use crate::quantize::{quantize_atkinson, quantize_bayer, quantize_blue_noise, quantize_yliluoma};

pub(super) const TARGET_WIDTH: u32 = 800;
pub(super) const TARGET_HEIGHT: u32 = 480;
pub(super) const DEFAULT_GAMMA: f32 = 1.0;
pub(super) const FIXTURE_NAMES: [&str; 3] = ["gradient", "starry_night", "tree"];
pub(super) const GAMMA_CASES: [(f32, &str); 3] = [(0.85, "g085"), (1.0, "g100"), (1.15, "g115")];
#[allow(dead_code)]
pub(super) const HALFTONE_CASES: [(HalftoneMode, &str); 5] = [
    (HalftoneMode::Bayer, "bayer"),
    (HalftoneMode::BlueNoise, "blue-noise"),
    (HalftoneMode::Atkinson, "atkinson"),
    (HalftoneMode::Yliluoma, "yliluoma"),
    (HalftoneMode::Auto, "auto"),
];
pub(super) const HARNESS_HALFTONE_CASES: [(HalftoneMode, &str); 4] = [
    (HalftoneMode::Bayer, "bayer"),
    (HalftoneMode::BlueNoise, "blue-noise"),
    (HalftoneMode::Atkinson, "atkinson"),
    (HalftoneMode::Auto, "auto"),
];

pub(super) struct TempImageFile {
    path: PathBuf,
}

impl TempImageFile {
    pub(super) fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        Self {
            path: std::env::temp_dir().join(format!("epaper_converter_{label}_{nanos}.png")),
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempImageFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) struct RenderedFixture {
    pub(super) fixture_name: &'static str,
    pub(super) gamma: f32,
    pub(super) gamma_slug: &'static str,
    pub(super) requested_mode: HalftoneMode,
    pub(super) resolved_mode: HalftoneMode,
    pub(super) elapsed_ms: u128,
    pub(super) output_path: PathBuf,
    pub(super) palette_report: PaletteReportSummary,
}

#[derive(Clone, Copy)]
pub(super) struct RankedCandidate<'a> {
    pub(super) fixture_name: &'static str,
    pub(super) gamma: f32,
    pub(super) gamma_slug: &'static str,
    pub(super) requested_mode: HalftoneMode,
    pub(super) resolved_mode: HalftoneMode,
    pub(super) elapsed_ms: u128,
    pub(super) palette_report: &'a PaletteReportSummary,
}

pub(super) struct ModeAggregateSummary {
    pub(super) requested_mode: HalftoneMode,
    pub(super) avg_total_abs_delta: f64,
    pub(super) avg_max_abs_delta: f64,
    pub(super) avg_elapsed_ms: f64,
    pub(super) sample_count: usize,
}

pub(super) struct PaletteReportSummary {
    pub(super) total_abs_delta: f64,
    pub(super) max_abs_delta: f64,
    pub(super) max_abs_delta_color: &'static str,
    pub(super) rendered_invalid_pixels: u64,
}

pub(super) struct RegressionComparison {
    pub(super) fixture_name: String,
    pub(super) gamma_slug: String,
    pub(super) requested_mode: String,
    pub(super) status: RegressionStatus,
    pub(super) baseline_total_abs_delta: Option<f64>,
    pub(super) current_total_abs_delta: Option<f64>,
    pub(super) baseline_max_abs_delta: Option<f64>,
    pub(super) current_max_abs_delta: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RegressionStatus {
    Improved,
    Regressed,
    Unchanged,
    NewCandidate,
    MissingCandidate,
}

#[derive(Clone)]
struct BaselineEntry {
    fixture_name: String,
    gamma_slug: String,
    requested_mode: String,
    total_abs_delta: f64,
    max_abs_delta: f64,
    rendered_invalid_pixels: u64,
}

struct RenderRequest {
    fixture_name: &'static str,
    requested_mode: HalftoneMode,
    output_slug: String,
    gamma: f32,
    gamma_slug: &'static str,
}

pub(super) fn render_standard_suite() -> Result<Vec<RenderedFixture>> {
    clear_output_dir()?;

    let mut requests = Vec::with_capacity(FIXTURE_NAMES.len() * HARNESS_HALFTONE_CASES.len());
    for fixture_name in FIXTURE_NAMES {
        for (requested_mode, slug) in HARNESS_HALFTONE_CASES {
            requests.push(RenderRequest {
                fixture_name,
                requested_mode,
                output_slug: slug.to_string(),
                gamma: DEFAULT_GAMMA,
                gamma_slug: "g100",
            });
        }
    }

    render_requests_in_parallel(requests)
}

pub(super) fn render_gamma_sweep() -> Result<Vec<RenderedFixture>> {
    clear_output_dir()?;

    let mut requests =
        Vec::with_capacity(FIXTURE_NAMES.len() * HARNESS_HALFTONE_CASES.len() * GAMMA_CASES.len());
    for fixture_name in FIXTURE_NAMES {
        for (requested_mode, mode_slug) in HARNESS_HALFTONE_CASES {
            for (gamma, gamma_slug) in GAMMA_CASES {
                requests.push(RenderRequest {
                    fixture_name,
                    requested_mode,
                    output_slug: format!("{mode_slug}_{gamma_slug}"),
                    gamma,
                    gamma_slug,
                });
            }
        }
    }

    render_requests_in_parallel(requests)
}

pub(super) fn render_fixture_to_output(
    fixture_name: &'static str,
    requested_mode: HalftoneMode,
    output_slug: &str,
    gamma: f32,
    gamma_slug: &'static str,
) -> Result<RenderedFixture> {
    fs::create_dir_all(output_dir()).context("Failed to create output directory")?;
    let start = Instant::now();

    let input_path = fixture_path(fixture_name);
    let rgb_img = prepare_image(
        &input_path,
        TARGET_WIDTH,
        TARGET_HEIGHT,
        ResizeMode::Cover,
        true,
        gamma,
    )?;

    let resolved_mode = match requested_mode {
        HalftoneMode::Auto => choose_halftone_mode(&rgb_img),
        mode => mode,
    };
    let indices = quantize_image(&rgb_img, resolved_mode);
    let rendered_img = indices_to_rgb_image(&indices, TARGET_WIDTH, TARGET_HEIGHT);
    let palette_report = build_palette_report(&rgb_img, &rendered_img);

    let output_path = output_dir().join(format!("{fixture_name}.cover.{output_slug}.png"));
    rendered_img
        .save(&output_path)
        .with_context(|| format!("Failed to save rendered output: {}", output_path.display()))?;
    let elapsed_ms = start.elapsed().as_millis();

    Ok(RenderedFixture {
        fixture_name,
        gamma,
        gamma_slug,
        requested_mode,
        resolved_mode,
        elapsed_ms,
        output_path,
        palette_report,
    })
}

fn render_requests_in_parallel(requests: Vec<RenderRequest>) -> Result<Vec<RenderedFixture>> {
    requests
        .par_iter()
        .map(|request| {
            render_fixture_to_output(
                request.fixture_name,
                request.requested_mode,
                &request.output_slug,
                request.gamma,
                request.gamma_slug,
            )
        })
        .collect()
}

pub(super) fn format_suite_report(rendered: &[RenderedFixture]) -> String {
    let mut report = String::from(
        "Fixture           Gamma  Mode         Resolved     Total abs delta   Max color delta   Time(ms)  Invalid\n",
    );
    report.push_str(
        "------------------------------------------------------------------------------------------------\n",
    );

    for case in rendered {
        let requested = halftone_mode_slug(case.requested_mode);
        let resolved = halftone_mode_slug(case.resolved_mode);
        let _ = writeln!(
            report,
            "{:<16} {:<6} {:<12} {:<12} {:>8.2} pp     {:<6} {:>6.2} pp {:>8} {:>8}",
            case.fixture_name,
            case.gamma_slug,
            requested,
            resolved,
            case.palette_report.total_abs_delta,
            case.palette_report.max_abs_delta_color,
            case.palette_report.max_abs_delta,
            case.elapsed_ms,
            case.palette_report.rendered_invalid_pixels,
        );
    }

    report
}

pub(super) fn rank_best_candidates_per_fixture(
    rendered: &[RenderedFixture],
) -> Vec<RankedCandidate<'_>> {
    FIXTURE_NAMES
        .iter()
        .filter_map(|fixture_name| {
            rendered
                .iter()
                .filter(|case| case.fixture_name == *fixture_name)
                .min_by(compare_rendered_fixture)
                .map(RankedCandidate::from)
        })
        .collect()
}

pub(super) fn format_leaderboard(rendered: &[RenderedFixture]) -> String {
    let leaders = rank_best_candidates_per_fixture(rendered);
    let mut report = String::from(
        "Best candidate per fixture\nFixture           Gamma  Mode         Resolved     Total abs delta   Max color delta\n",
    );
    report.push_str(
        "--------------------------------------------------------------------------------\n",
    );

    for candidate in leaders {
        let requested = halftone_mode_slug(candidate.requested_mode);
        let resolved = halftone_mode_slug(candidate.resolved_mode);
        let _ = writeln!(
            report,
            "{:<16} {:<6} {:<12} {:<12} {:>8.2} pp     {:<6} {:>6.2} pp",
            candidate.fixture_name,
            candidate.gamma_slug,
            requested,
            resolved,
            candidate.palette_report.total_abs_delta,
            candidate.palette_report.max_abs_delta_color,
            candidate.palette_report.max_abs_delta,
        );
    }

    report
}

pub(super) fn summarize_modes(rendered: &[RenderedFixture]) -> Vec<ModeAggregateSummary> {
    HARNESS_HALFTONE_CASES
        .iter()
        .filter_map(|(mode, _)| {
            let cases = rendered
                .iter()
                .filter(|case| case.requested_mode == *mode)
                .collect::<Vec<_>>();

            if cases.is_empty() {
                return None;
            }

            let sample_count = cases.len();
            let total_abs_delta = cases
                .iter()
                .map(|case| case.palette_report.total_abs_delta)
                .sum::<f64>();
            let max_abs_delta = cases
                .iter()
                .map(|case| case.palette_report.max_abs_delta)
                .sum::<f64>();
            let elapsed_ms = cases.iter().map(|case| case.elapsed_ms as f64).sum::<f64>();

            Some(ModeAggregateSummary {
                requested_mode: *mode,
                avg_total_abs_delta: total_abs_delta / sample_count as f64,
                avg_max_abs_delta: max_abs_delta / sample_count as f64,
                avg_elapsed_ms: elapsed_ms / sample_count as f64,
                sample_count,
            })
        })
        .collect()
}

pub(super) fn format_mode_summary(rendered: &[RenderedFixture]) -> String {
    let mut report = String::from(
        "Average quality/speed by requested mode\nMode         Avg total abs delta   Avg max color delta   Avg time(ms)   Samples\n",
    );
    report.push_str(
        "--------------------------------------------------------------------------------\n",
    );

    for summary in summarize_modes(rendered) {
        let _ = writeln!(
            report,
            "{:<12} {:>8.2} pp           {:>8.2} pp       {:>8.2} {:>9}",
            halftone_mode_slug(summary.requested_mode),
            summary.avg_total_abs_delta,
            summary.avg_max_abs_delta,
            summary.avg_elapsed_ms,
            summary.sample_count,
        );
    }

    report
}

pub(super) fn format_recommendations(rendered: &[RenderedFixture]) -> String {
    let mut report = String::from("Harness recommendation\n");

    if let Some(best) = overall_best_candidate(rendered) {
        let _ = writeln!(
            report,
            "Best overall: {} + {} ({:.2}) + {} -> {:.2} pp total / {:.2} pp max / {} ms",
            best.fixture_name,
            best.gamma_slug,
            best.gamma,
            halftone_mode_slug(best.requested_mode),
            best.palette_report.total_abs_delta,
            best.palette_report.max_abs_delta,
            best.elapsed_ms,
        );
    }

    if let Some(fastest) = fastest_candidate(rendered) {
        let _ = writeln!(
            report,
            "Fastest candidate: {} + {} ({:.2}) + {} -> {} ms / {:.2} pp total",
            fastest.fixture_name,
            fastest.gamma_slug,
            fastest.gamma,
            halftone_mode_slug(fastest.requested_mode),
            fastest.elapsed_ms,
            fastest.palette_report.total_abs_delta,
        );
    }

    let leaders = rank_best_candidates_per_fixture(rendered);
    let auto_wins = leaders
        .iter()
        .filter(|candidate| candidate.requested_mode == HalftoneMode::Auto)
        .count();
    let _ = writeln!(
        report,
        "Auto wins: {}/{} fixtures",
        auto_wins,
        leaders.len(),
    );

    report
}

pub(super) fn build_baseline_snapshot(rendered: &[RenderedFixture]) -> String {
    let mut entries = rendered
        .iter()
        .map(|case| {
            format!(
                "{}\t{}\t{}\t{:.4}\t{:.4}\t{}",
                case.fixture_name,
                case.gamma_slug,
                halftone_mode_slug(case.requested_mode),
                case.palette_report.total_abs_delta,
                case.palette_report.max_abs_delta,
                case.palette_report.rendered_invalid_pixels,
            )
        })
        .collect::<Vec<_>>();
    entries.sort();

    let mut snapshot = String::from(
        "fixture\tgamma\tmode\ttotal_abs_delta\tmax_abs_delta\trendered_invalid_pixels\n",
    );
    for entry in entries {
        snapshot.push_str(&entry);
        snapshot.push('\n');
    }

    snapshot
}

pub(super) fn compare_against_baseline(
    rendered: &[RenderedFixture],
    snapshot: &str,
) -> Result<Vec<RegressionComparison>> {
    let baseline_entries = parse_baseline_snapshot(snapshot)?;
    let current_entries = rendered
        .iter()
        .map(|case| {
            (
                baseline_key(
                    case.fixture_name,
                    case.gamma_slug,
                    halftone_mode_slug(case.requested_mode),
                ),
                BaselineEntry {
                    fixture_name: case.fixture_name.to_string(),
                    gamma_slug: case.gamma_slug.to_string(),
                    requested_mode: halftone_mode_slug(case.requested_mode).to_string(),
                    total_abs_delta: case.palette_report.total_abs_delta,
                    max_abs_delta: case.palette_report.max_abs_delta,
                    rendered_invalid_pixels: case.palette_report.rendered_invalid_pixels,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    let all_keys = baseline_entries
        .keys()
        .chain(current_entries.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    Ok(all_keys
        .into_iter()
        .map(
            |key| match (baseline_entries.get(&key), current_entries.get(&key)) {
                (Some(baseline), Some(current)) => build_regression_comparison(baseline, current),
                (Some(baseline), None) => RegressionComparison {
                    fixture_name: baseline.fixture_name.clone(),
                    gamma_slug: baseline.gamma_slug.clone(),
                    requested_mode: baseline.requested_mode.clone(),
                    status: RegressionStatus::MissingCandidate,
                    baseline_total_abs_delta: Some(baseline.total_abs_delta),
                    current_total_abs_delta: None,
                    baseline_max_abs_delta: Some(baseline.max_abs_delta),
                    current_max_abs_delta: None,
                },
                (None, Some(current)) => RegressionComparison {
                    fixture_name: current.fixture_name.clone(),
                    gamma_slug: current.gamma_slug.clone(),
                    requested_mode: current.requested_mode.clone(),
                    status: RegressionStatus::NewCandidate,
                    baseline_total_abs_delta: None,
                    current_total_abs_delta: Some(current.total_abs_delta),
                    baseline_max_abs_delta: None,
                    current_max_abs_delta: Some(current.max_abs_delta),
                },
                (None, None) => unreachable!(),
            },
        )
        .collect())
}

pub(super) fn format_regression_report(comparisons: &[RegressionComparison]) -> String {
    let mut report = String::from(
        "Regression comparison\nFixture           Gamma  Mode         Status       Baseline total   Current total\n",
    );
    report.push_str(
        "--------------------------------------------------------------------------------\n",
    );

    for item in comparisons {
        let _ = writeln!(
            report,
            "{:<16} {:<6} {:<12} {:<12} {:>8} {:>14}",
            item.fixture_name,
            item.gamma_slug,
            item.requested_mode,
            regression_status_label(item.status),
            format_optional_delta(item.baseline_total_abs_delta),
            format_optional_delta(item.current_total_abs_delta),
        );
    }

    report
}

fn clear_output_dir() -> Result<()> {
    let dir = output_dir();
    fs::create_dir_all(&dir).context("Failed to create output directory")?;

    for entry in fs::read_dir(&dir).context("Failed to read output directory")? {
        let path = entry?.path();
        if path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove output file: {}", path.display()))?;
        }
    }

    Ok(())
}

fn quantize_image(img: &RgbImage, mode: HalftoneMode) -> Vec<u8> {
    match mode {
        HalftoneMode::Bayer => quantize_bayer(img, TARGET_WIDTH, TARGET_HEIGHT),
        HalftoneMode::BlueNoise => quantize_blue_noise(img, TARGET_WIDTH, TARGET_HEIGHT),
        HalftoneMode::Yliluoma => quantize_yliluoma(img, TARGET_WIDTH, TARGET_HEIGHT),
        HalftoneMode::Atkinson => quantize_atkinson(img, TARGET_WIDTH, TARGET_HEIGHT),
        HalftoneMode::Auto => unreachable!(),
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

fn build_regression_comparison(
    baseline: &BaselineEntry,
    current: &BaselineEntry,
) -> RegressionComparison {
    let status = if current.rendered_invalid_pixels != baseline.rendered_invalid_pixels {
        if current.rendered_invalid_pixels < baseline.rendered_invalid_pixels {
            RegressionStatus::Improved
        } else {
            RegressionStatus::Regressed
        }
    } else if metric_cmp(current.total_abs_delta, baseline.total_abs_delta) < 0
        || (metric_cmp(current.total_abs_delta, baseline.total_abs_delta) == 0
            && metric_cmp(current.max_abs_delta, baseline.max_abs_delta) < 0)
    {
        RegressionStatus::Improved
    } else if metric_cmp(current.total_abs_delta, baseline.total_abs_delta) > 0
        || (metric_cmp(current.total_abs_delta, baseline.total_abs_delta) == 0
            && metric_cmp(current.max_abs_delta, baseline.max_abs_delta) > 0)
    {
        RegressionStatus::Regressed
    } else {
        RegressionStatus::Unchanged
    };

    RegressionComparison {
        fixture_name: current.fixture_name.clone(),
        gamma_slug: current.gamma_slug.clone(),
        requested_mode: current.requested_mode.clone(),
        status,
        baseline_total_abs_delta: Some(baseline.total_abs_delta),
        current_total_abs_delta: Some(current.total_abs_delta),
        baseline_max_abs_delta: Some(baseline.max_abs_delta),
        current_max_abs_delta: Some(current.max_abs_delta),
    }
}

fn parse_baseline_snapshot(snapshot: &str) -> Result<BTreeMap<String, BaselineEntry>> {
    let mut lines = snapshot.lines();
    let header = lines.next().unwrap_or_default();
    anyhow::ensure!(
        header == "fixture\tgamma\tmode\ttotal_abs_delta\tmax_abs_delta\trendered_invalid_pixels",
        "Invalid baseline snapshot header"
    );

    let mut entries = BTreeMap::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let parts = line.split('\t').collect::<Vec<_>>();
        anyhow::ensure!(parts.len() == 6, "Invalid baseline snapshot row: {line}");

        let entry = BaselineEntry {
            fixture_name: parts[0].to_string(),
            gamma_slug: parts[1].to_string(),
            requested_mode: parts[2].to_string(),
            total_abs_delta: parts[3].parse().context("Invalid total_abs_delta")?,
            max_abs_delta: parts[4].parse().context("Invalid max_abs_delta")?,
            rendered_invalid_pixels: parts[5]
                .parse()
                .context("Invalid rendered_invalid_pixels")?,
        };
        let key = baseline_key(
            &entry.fixture_name,
            &entry.gamma_slug,
            &entry.requested_mode,
        );
        entries.insert(key, entry);
    }

    Ok(entries)
}

fn baseline_key(fixture_name: &str, gamma_slug: &str, requested_mode: &str) -> String {
    format!("{fixture_name}::{gamma_slug}::{requested_mode}")
}

fn metric_cmp(lhs: f64, rhs: f64) -> i8 {
    const EPSILON: f64 = 0.0001;

    if (lhs - rhs).abs() <= EPSILON {
        0
    } else if lhs < rhs {
        -1
    } else {
        1
    }
}

fn regression_status_label(status: RegressionStatus) -> &'static str {
    match status {
        RegressionStatus::Improved => "improved",
        RegressionStatus::Regressed => "regressed",
        RegressionStatus::Unchanged => "unchanged",
        RegressionStatus::NewCandidate => "new",
        RegressionStatus::MissingCandidate => "missing",
    }
}

fn format_optional_delta(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2} pp"))
        .unwrap_or_else(|| "-".to_string())
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
        halftone_mode_slug(case.requested_mode),
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

fn halftone_mode_slug(mode: HalftoneMode) -> &'static str {
    match mode {
        HalftoneMode::Bayer => "bayer",
        HalftoneMode::BlueNoise => "blue-noise",
        HalftoneMode::Yliluoma => "yliluoma",
        HalftoneMode::Atkinson => "atkinson",
        HalftoneMode::Auto => "auto",
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
