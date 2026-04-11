use std::fmt::Write;

use super::{
    compare_rendered_fixture, dither_mode_slug, fastest_candidate, overall_best_candidate,
    ModeAggregateSummary, RankedCandidate, RenderedFixture, HARNESS_DITHER_CASES,
};

pub(crate) fn format_suite_report(rendered: &[RenderedFixture]) -> String {
    let mut report = String::from(
        "Fixture           Gamma  Mode         Resolved     Total abs delta   Max color delta   Time(ms)  Invalid\n",
    );
    report.push_str(
        "------------------------------------------------------------------------------------------------\n",
    );

    for case in rendered {
        let requested = dither_mode_slug(case.requested_mode);
        let resolved = dither_mode_slug(case.resolved_mode);
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

pub(crate) fn rank_best_candidates_per_fixture(
    rendered: &[RenderedFixture],
) -> Vec<RankedCandidate<'_>> {
    ordered_fixture_names(rendered)
        .into_iter()
        .filter_map(|fixture_name| {
            rendered
                .iter()
                .filter(|case| case.fixture_name == fixture_name)
                .min_by(compare_rendered_fixture)
                .map(RankedCandidate::from)
        })
        .collect()
}

fn ordered_fixture_names(rendered: &[RenderedFixture]) -> Vec<&str> {
    let mut ordered = Vec::new();

    for case in rendered {
        let fixture_name = case.fixture_name.as_str();
        if !ordered.contains(&fixture_name) {
            ordered.push(fixture_name);
        }
    }

    ordered
}

pub(crate) fn format_leaderboard(rendered: &[RenderedFixture]) -> String {
    let leaders = rank_best_candidates_per_fixture(rendered);
    let mut report = String::from(
        "Best candidate per fixture\nFixture           Gamma  Mode         Resolved     Total abs delta   Max color delta\n",
    );
    report.push_str(
        "--------------------------------------------------------------------------------\n",
    );

    for candidate in leaders {
        let requested = dither_mode_slug(candidate.requested_mode);
        let resolved = dither_mode_slug(candidate.resolved_mode);
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

pub(crate) fn summarize_modes(rendered: &[RenderedFixture]) -> Vec<ModeAggregateSummary> {
    HARNESS_DITHER_CASES
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

pub(crate) fn format_mode_summary(rendered: &[RenderedFixture]) -> String {
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
            dither_mode_slug(summary.requested_mode),
            summary.avg_total_abs_delta,
            summary.avg_max_abs_delta,
            summary.avg_elapsed_ms,
            summary.sample_count,
        );
    }

    report
}

pub(crate) fn format_recommendations(rendered: &[RenderedFixture]) -> String {
    let mut report = String::from("Harness recommendation\n");

    if let Some(best) = overall_best_candidate(rendered) {
        let _ = writeln!(
            report,
            "Best overall: {} + {} ({:.2}) + {} -> {:.2} pp total / {:.2} pp max / {} ms",
            best.fixture_name,
            best.gamma_slug,
            best.gamma,
            dither_mode_slug(best.requested_mode),
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
            dither_mode_slug(fastest.requested_mode),
            fastest.elapsed_ms,
            fastest.palette_report.total_abs_delta,
        );
    }

    report
}
