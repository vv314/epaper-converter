use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use super::{
    halftone_mode_slug, BaselineEntry, RegressionComparison, RegressionStatus, RenderedFixture,
};

pub(crate) fn build_baseline_snapshot(rendered: &[RenderedFixture]) -> String {
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

pub(crate) fn compare_against_baseline(
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
        .collect::<BTreeSet<_>>();

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

pub(crate) fn format_regression_report(comparisons: &[RegressionComparison]) -> String {
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
