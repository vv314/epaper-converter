use super::harness::{
    self, FIXTURE_NAMES, GAMMA_CASES, HARNESS_HALFTONE_CASES, TARGET_HEIGHT, TARGET_WIDTH,
};
use crate::cli::HalftoneMode;

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
