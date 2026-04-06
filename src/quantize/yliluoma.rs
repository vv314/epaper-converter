use image::RgbImage;
use std::collections::HashMap;

use super::color::{
    ciede2000_distance_sq, lab_components_from_rgb, linear_array_to_lab, palette_lab,
    palette_linear, palette_luma, warm_up_color_lut, PALETTE,
};
use super::ordered::ordered_threshold_8x8;

const YLILUOMA_MIX_LEVELS: usize = 64;
const YLILUOMA_CACHE_BITS: u8 = 6;
const YLILUOMA_NEAREST_CANDIDATES: usize = 5;
const YLILUOMA_COMPONENT_PENALTY_WEIGHT: f32 = 0.18;
const YLILUOMA_COMPLEXITY_PENALTY_WEIGHT: f32 = 0.22;
const YLILUOMA_CHROMA_MATCH_WEIGHT: f32 = 0.24;
const YLILUOMA_COMPONENT_CHROMA_DEFICIT_WEIGHT: f32 = 0.18;
const YLILUOMA_COMPONENT_HUE_WEIGHT: f32 = 0.24;
const YLILUOMA_PLAN_TRANSITION_WEIGHT: f32 = 0.16;
const YLILUOMA_LUMA_SPAN_PENALTY_WEIGHT: f32 = 0.22;
const YLILUOMA_NEAREST_PRESENCE_WEIGHT: f32 = 0.30;

#[derive(Clone, Copy)]
struct YliluomaMixPlan {
    colors: [u8; 64],
}

impl Default for YliluomaMixPlan {
    fn default() -> Self {
        Self { colors: [0; 64] }
    }
}

#[inline(always)]
fn yliluoma_cache_key(r: u8, g: u8, b: u8) -> u32 {
    let shift = 8 - YLILUOMA_CACHE_BITS;
    (((r >> shift) as u32) << (YLILUOMA_CACHE_BITS as u32 * 2))
        | (((g >> shift) as u32) << YLILUOMA_CACHE_BITS as u32)
        | (b >> shift) as u32
}

fn build_plan_from_counts(counts: &[(u8, usize)]) -> YliluomaMixPlan {
    let palette_luma = palette_luma();
    let mut sorted = counts.to_vec();
    sorted.sort_by(|(lhs, _), (rhs, _)| {
        palette_luma[*lhs as usize]
            .partial_cmp(&palette_luma[*rhs as usize])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut colors = [0u8; YLILUOMA_MIX_LEVELS];
    let mut offset = 0usize;
    for &(color, count) in &sorted {
        for slot in colors.iter_mut().skip(offset).take(count) {
            *slot = color;
        }
        offset += count;
    }

    YliluomaMixPlan { colors }
}

fn plan_transition_penalty(plan: &YliluomaMixPlan) -> f32 {
    let palette_lab = palette_lab();
    let mut penalty = 0.0f32;

    for idx in 1..YLILUOMA_MIX_LEVELS {
        let prev = plan.colors[idx - 1];
        let next = plan.colors[idx];
        if prev != next {
            penalty +=
                ciede2000_distance_sq(palette_lab[prev as usize], palette_lab[next as usize])
                    .sqrt();
        }
    }

    penalty
}

fn top_palette_candidates(target_lab: [f32; 3], limit: usize) -> Vec<u8> {
    let palette_lab = palette_lab();
    let mut ranked = (0..PALETTE.len())
        .map(|idx| {
            (
                idx as u8,
                ciede2000_distance_sq(target_lab, palette_lab[idx]),
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|lhs, rhs| {
        lhs.1
            .partial_cmp(&rhs.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked.into_iter().take(limit).map(|(idx, _)| idx).collect()
}

fn evaluate_mix(target_lab: [f32; 3], counts: &[(u8, usize)]) -> (f32, YliluomaMixPlan) {
    let palette_linear = palette_linear();
    let palette_lab = palette_lab();
    let mut mixed_linear = [0.0f32; 3];
    let mut component_penalty = 0.0f32;
    let mut component_chroma_deficit = 0.0f32;
    let mut component_hue_penalty = 0.0f32;
    let mut min_luma = f32::INFINITY;
    let mut max_luma = f32::NEG_INFINITY;
    let mut active_colors = 0usize;
    let target_chroma = (target_lab[1] * target_lab[1] + target_lab[2] * target_lab[2]).sqrt();
    let lightness = (target_lab[0] / 100.0).clamp(0.0, 1.0);
    let dark_factor = ((45.0 - target_lab[0]) / 45.0).clamp(0.0, 1.0);
    let bright_factor = ((target_lab[0] - 35.0) / 50.0).clamp(0.0, 1.0);
    let vivid_factor = (target_chroma / 45.0).clamp(0.0, 1.0);
    let chroma_weight = 0.5 + 1.5 * vivid_factor;
    let (target_hue_a, target_hue_b) = if target_chroma > 1e-3 {
        (target_lab[1] / target_chroma, target_lab[2] / target_chroma)
    } else {
        (0.0, 0.0)
    };
    let mut nearest_palette_idx = 0u8;
    let mut nearest_palette_score = f32::INFINITY;

    for (idx, candidate_lab) in palette_lab.iter().enumerate() {
        let score = ciede2000_distance_sq(target_lab, *candidate_lab);
        if score < nearest_palette_score {
            nearest_palette_score = score;
            nearest_palette_idx = idx as u8;
        }
    }

    for &(color, count) in counts {
        if count == 0 {
            continue;
        }
        active_colors += 1;
        let weight = count as f32 / YLILUOMA_MIX_LEVELS as f32;
        mixed_linear[0] += palette_linear[color as usize][0] * weight;
        mixed_linear[1] += palette_linear[color as usize][1] * weight;
        mixed_linear[2] += palette_linear[color as usize][2] * weight;
        component_penalty +=
            ciede2000_distance_sq(target_lab, palette_lab[color as usize]) * weight;
        let luma = palette_luma()[color as usize];
        min_luma = min_luma.min(luma);
        max_luma = max_luma.max(luma);

        let component_chroma = (palette_lab[color as usize][1] * palette_lab[color as usize][1]
            + palette_lab[color as usize][2] * palette_lab[color as usize][2])
            .sqrt();
        component_chroma_deficit += (target_chroma - component_chroma).max(0.0) * weight;

        if target_chroma > 1e-3 && component_chroma > 1e-3 {
            let hue_cosine = ((palette_lab[color as usize][1] / component_chroma) * target_hue_a
                + (palette_lab[color as usize][2] / component_chroma) * target_hue_b)
                .clamp(-1.0, 1.0);
            component_hue_penalty += (1.0 - hue_cosine) * target_chroma * weight;
        }
    }

    let mixed_lab = linear_array_to_lab(mixed_linear);
    let mixed_chroma = (mixed_lab[1] * mixed_lab[1] + mixed_lab[2] * mixed_lab[2]).sqrt();
    let plan = build_plan_from_counts(counts);
    let luma_span = if active_colors > 1 {
        max_luma - min_luma
    } else {
        0.0
    };
    let nearest_present = counts
        .iter()
        .any(|(color, count)| *count > 0 && *color == nearest_palette_idx);
    let score = ciede2000_distance_sq(target_lab, mixed_lab)
        + (target_chroma - mixed_chroma).abs() * YLILUOMA_CHROMA_MATCH_WEIGHT * chroma_weight
        + component_penalty * YLILUOMA_COMPONENT_PENALTY_WEIGHT
        + component_chroma_deficit * YLILUOMA_COMPONENT_CHROMA_DEFICIT_WEIGHT * chroma_weight
        + component_hue_penalty * YLILUOMA_COMPONENT_HUE_WEIGHT * chroma_weight
        + luma_span
            * (active_colors.saturating_sub(1) as f32)
            * YLILUOMA_LUMA_SPAN_PENALTY_WEIGHT
            * bright_factor
            * (0.4 + 0.6 * vivid_factor)
        + (active_colors.saturating_sub(1) as f32)
            * YLILUOMA_COMPLEXITY_PENALTY_WEIGHT
            * (1.0 + 1.2 * dark_factor)
        + plan_transition_penalty(&plan) * YLILUOMA_PLAN_TRANSITION_WEIGHT
        + if nearest_present {
            0.0
        } else {
            nearest_palette_score.sqrt()
                * YLILUOMA_NEAREST_PRESENCE_WEIGHT
                * (0.4 + 0.9 * dark_factor + 0.4 * (1.0 - lightness))
        };

    (score, plan)
}

fn make_yliluoma_plan(r: u8, g: u8, b: u8) -> YliluomaMixPlan {
    let target_lab = lab_components_from_rgb([r, g, b]);
    let candidates = top_palette_candidates(target_lab, YLILUOMA_NEAREST_CANDIDATES);
    let mut best_score = f32::INFINITY;
    let mut best_plan = YliluomaMixPlan::default();

    for &color in &candidates {
        let (score, plan) = evaluate_mix(target_lab, &[(color, YLILUOMA_MIX_LEVELS)]);
        if score < best_score {
            best_score = score;
            best_plan = plan;
        }
    }

    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            for count_b in 0..=YLILUOMA_MIX_LEVELS {
                let count_a = YLILUOMA_MIX_LEVELS - count_b;
                let (score, plan) = evaluate_mix(
                    target_lab,
                    &[(candidates[i], count_a), (candidates[j], count_b)],
                );
                if score < best_score {
                    best_score = score;
                    best_plan = plan;
                }
            }
        }
    }

    if candidates.len() == YLILUOMA_NEAREST_CANDIDATES {
        for count_c in 0..=YLILUOMA_MIX_LEVELS {
            for count_b in 0..=(YLILUOMA_MIX_LEVELS - count_c) {
                let count_a = YLILUOMA_MIX_LEVELS - count_b - count_c;
                let (score, plan) = evaluate_mix(
                    target_lab,
                    &[
                        (candidates[0], count_a),
                        (candidates[1], count_b),
                        (candidates[2], count_c),
                    ],
                );
                if score < best_score {
                    best_score = score;
                    best_plan = plan;
                }
            }
        }
    }

    best_plan
}

pub(crate) fn quantize_yliluoma(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    warm_up_color_lut();
    palette_linear();
    palette_luma();

    let width = width as usize;
    let height = height as usize;
    let raw = img.as_raw();
    let mut output = vec![0u8; width * height];
    let mut plan_cache = HashMap::new();

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let src_base = idx * 3;
            let key = yliluoma_cache_key(raw[src_base], raw[src_base + 1], raw[src_base + 2]);
            let plan = *plan_cache.entry(key).or_insert_with(|| {
                make_yliluoma_plan(raw[src_base], raw[src_base + 1], raw[src_base + 2])
            });
            let threshold = ordered_threshold_8x8(x, y);

            output[idx] = plan.colors[threshold];
        }
    }

    output
}
