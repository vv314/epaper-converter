use std::path::PathBuf;

use crate::cli::HalftoneMode;

pub(crate) struct RenderedFixture {
    pub(crate) fixture_name: &'static str,
    pub(crate) gamma: f32,
    pub(crate) gamma_slug: &'static str,
    pub(crate) requested_mode: HalftoneMode,
    pub(crate) resolved_mode: HalftoneMode,
    pub(crate) elapsed_ms: u128,
    pub(crate) output_path: PathBuf,
    pub(crate) palette_report: PaletteReportSummary,
}

#[derive(Clone, Copy)]
pub(crate) struct RankedCandidate<'a> {
    pub(crate) fixture_name: &'static str,
    pub(crate) gamma: f32,
    pub(crate) gamma_slug: &'static str,
    pub(crate) requested_mode: HalftoneMode,
    pub(crate) resolved_mode: HalftoneMode,
    pub(crate) elapsed_ms: u128,
    pub(crate) palette_report: &'a PaletteReportSummary,
}

pub(crate) struct ModeAggregateSummary {
    pub(crate) requested_mode: HalftoneMode,
    pub(crate) avg_total_abs_delta: f64,
    pub(crate) avg_max_abs_delta: f64,
    pub(crate) avg_elapsed_ms: f64,
    pub(crate) sample_count: usize,
}

pub(crate) struct PaletteReportSummary {
    pub(crate) total_abs_delta: f64,
    pub(crate) max_abs_delta: f64,
    pub(crate) max_abs_delta_color: &'static str,
    pub(crate) rendered_invalid_pixels: u64,
}

pub(crate) struct RegressionComparison {
    pub(crate) fixture_name: String,
    pub(crate) gamma_slug: String,
    pub(crate) requested_mode: String,
    pub(crate) status: RegressionStatus,
    pub(crate) baseline_total_abs_delta: Option<f64>,
    pub(crate) current_total_abs_delta: Option<f64>,
    pub(crate) baseline_max_abs_delta: Option<f64>,
    pub(crate) current_max_abs_delta: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegressionStatus {
    Improved,
    Regressed,
    Unchanged,
    NewCandidate,
    MissingCandidate,
}

#[derive(Clone)]
pub(crate) struct BaselineEntry {
    pub(crate) fixture_name: String,
    pub(crate) gamma_slug: String,
    pub(crate) requested_mode: String,
    pub(crate) total_abs_delta: f64,
    pub(crate) max_abs_delta: f64,
    pub(crate) rendered_invalid_pixels: u64,
}

pub(crate) struct RenderRequest {
    pub(crate) fixture_name: &'static str,
    pub(crate) requested_mode: HalftoneMode,
    pub(crate) output_slug: String,
    pub(crate) gamma: f32,
    pub(crate) gamma_slug: &'static str,
}
