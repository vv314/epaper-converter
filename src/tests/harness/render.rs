use anyhow::{Context, Result};
use rayon::prelude::*;
use std::fs;
use std::time::Instant;

use crate::cli::{DitherMode, ResizeMode};
use crate::pipeline::prepare_image;

use super::{
    build_palette_report, fixture_path, output_dir, output_path_for_request,
    prune_output_dir_for_requests, quantize_image, RenderRequest, RenderedFixture, DEFAULT_GAMMA,
    FIXTURE_NAMES, GAMMA_CASES, HARNESS_DITHER_CASES, TARGET_HEIGHT, TARGET_WIDTH,
};

const HARNESS_ARTIFACT_TAG_ENV: &str = "EPAPER_HARNESS_TAG";

pub(crate) fn render_standard_suite() -> Result<Vec<RenderedFixture>> {
    let artifact_tag = current_artifact_tag();
    render_standard_suite_with_tag(artifact_tag.as_deref())
}

pub(crate) fn render_standard_suite_with_tag(
    artifact_tag: Option<&str>,
) -> Result<Vec<RenderedFixture>> {
    let normalized_tag = sanitize_artifact_tag(artifact_tag);

    let mut requests = Vec::with_capacity(FIXTURE_NAMES.len() * HARNESS_DITHER_CASES.len());
    for fixture_name in FIXTURE_NAMES {
        for (requested_mode, slug) in HARNESS_DITHER_CASES {
            requests.push(RenderRequest {
                fixture_name,
                requested_mode,
                output_slug: with_artifact_tag(slug, normalized_tag.as_deref()),
                gamma: DEFAULT_GAMMA,
                gamma_slug: "g100",
            });
        }
    }

    prune_output_dir_for_requests(&requests)?;
    render_requests_in_parallel(requests)
}

pub(crate) fn render_gamma_sweep() -> Result<Vec<RenderedFixture>> {
    let artifact_tag = current_artifact_tag();
    render_gamma_sweep_with_tag(artifact_tag.as_deref())
}

pub(crate) fn render_gamma_sweep_with_tag(
    artifact_tag: Option<&str>,
) -> Result<Vec<RenderedFixture>> {
    let normalized_tag = sanitize_artifact_tag(artifact_tag);

    let mut requests =
        Vec::with_capacity(FIXTURE_NAMES.len() * HARNESS_DITHER_CASES.len() * GAMMA_CASES.len());
    for fixture_name in FIXTURE_NAMES {
        for (requested_mode, mode_slug) in HARNESS_DITHER_CASES {
            for (gamma, gamma_slug) in GAMMA_CASES {
                requests.push(RenderRequest {
                    fixture_name,
                    requested_mode,
                    output_slug: with_artifact_tag(
                        &format!("{mode_slug}_{gamma_slug}"),
                        normalized_tag.as_deref(),
                    ),
                    gamma,
                    gamma_slug,
                });
            }
        }
    }

    prune_output_dir_for_requests(&requests)?;
    render_requests_in_parallel(requests)
}

pub(crate) fn render_fixture_to_output(
    fixture_name: &'static str,
    requested_mode: DitherMode,
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

    let resolved_mode = requested_mode;
    let indices = quantize_image(&rgb_img, requested_mode);
    let rendered_img = super::indices_to_rgb_image(&indices, TARGET_WIDTH, TARGET_HEIGHT);
    let palette_report = build_palette_report(&rgb_img, &rendered_img);

    let output_path = output_path_for_request(fixture_name, output_slug);
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

fn current_artifact_tag() -> Option<String> {
    std::env::var(HARNESS_ARTIFACT_TAG_ENV)
        .ok()
        .and_then(|value| sanitize_artifact_tag(Some(&value)))
}

fn with_artifact_tag(output_slug: &str, artifact_tag: Option<&str>) -> String {
    match sanitize_artifact_tag(artifact_tag) {
        Some(tag) => format!("{output_slug}_{tag}"),
        None => output_slug.to_string(),
    }
}

fn sanitize_artifact_tag(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    let mut normalized = String::with_capacity(raw.len());
    let mut prev_was_separator = false;

    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            prev_was_separator = false;
            continue;
        }

        let mapped_separator = match ch {
            '-' => Some('-'),
            '_' | ' ' | '.' | '/' | '\\' => Some('_'),
            _ => None,
        };

        if let Some(separator) = mapped_separator {
            if !normalized.is_empty() && !prev_was_separator {
                normalized.push(separator);
                prev_was_separator = true;
            }
        }
    }

    let trimmed = normalized.trim_matches(['_', '-']);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::DitherMode;

    #[test]
    fn sanitize_artifact_tag_keeps_readable_iteration_names() {
        assert_eq!(sanitize_artifact_tag(Some(" v2 ")).as_deref(), Some("v2"));
        assert_eq!(
            sanitize_artifact_tag(Some("Lab FastPath")).as_deref(),
            Some("lab_fastpath")
        );
        assert_eq!(
            sanitize_artifact_tag(Some("rgb/branch.01")).as_deref(),
            Some("rgb_branch_01")
        );
        assert_eq!(sanitize_artifact_tag(Some("***")).as_deref(), None);
    }

    #[test]
    fn with_artifact_tag_appends_iteration_suffix() {
        assert_eq!(with_artifact_tag("bayer", Some("v3")), "bayer_v3");
        assert_eq!(
            with_artifact_tag("yliluoma_g100", Some("Lab Tune")),
            "yliluoma_g100_lab_tune"
        );
        assert_eq!(
            with_artifact_tag("floyd-steinberg", None),
            "floyd-steinberg"
        );
    }

    #[test]
    fn prune_output_dir_only_removes_files_for_current_requests() -> Result<()> {
        let request = RenderRequest {
            fixture_name: "gradient",
            requested_mode: DitherMode::Bayer,
            output_slug: "bayer_vnext".to_string(),
            gamma: 1.0,
            gamma_slug: "g100",
        };
        let matching_path = output_path_for_request(request.fixture_name, &request.output_slug);
        let unrelated_path = output_dir().join("keep-me.txt");

        fs::create_dir_all(output_dir())?;
        fs::write(&matching_path, b"replace me")?;
        fs::write(&unrelated_path, b"keep me")?;

        prune_output_dir_for_requests(&[request])?;

        assert!(!matching_path.exists());
        assert!(unrelated_path.exists());

        let _ = fs::remove_file(unrelated_path);
        Ok(())
    }
}
