use anyhow::{Context, Result};
use rayon::prelude::*;
use std::fs;
use std::time::Instant;

use crate::cli::{HalftoneMode, ResizeMode};
use crate::pipeline::{choose_halftone_mode, prepare_image};

use super::{
    build_palette_report, clear_output_dir, fixture_path, output_dir, quantize_image,
    RenderRequest, RenderedFixture, DEFAULT_GAMMA, FIXTURE_NAMES, GAMMA_CASES,
    HARNESS_HALFTONE_CASES, TARGET_HEIGHT, TARGET_WIDTH,
};

pub(crate) fn render_standard_suite() -> Result<Vec<RenderedFixture>> {
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

pub(crate) fn render_gamma_sweep() -> Result<Vec<RenderedFixture>> {
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

pub(crate) fn render_fixture_to_output(
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

    let resolved_mode = choose_mode(requested_mode, &rgb_img);
    let indices = quantize_image(&rgb_img, resolved_mode);
    let rendered_img = super::indices_to_rgb_image(&indices, TARGET_WIDTH, TARGET_HEIGHT);
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

fn choose_mode(requested_mode: HalftoneMode, rgb_img: &image::RgbImage) -> HalftoneMode {
    match requested_mode {
        HalftoneMode::Auto => choose_halftone_mode(rgb_img),
        mode => mode,
    }
}
