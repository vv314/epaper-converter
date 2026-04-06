use image::RgbImage;
use lab::Lab;
use std::collections::HashMap;
use std::sync::OnceLock;

pub(crate) const PALETTE: [[u8; 3]; 6] = [
    [0, 0, 0],       // Black (0)
    [255, 255, 255], // White (1)
    [255, 0, 0],     // Red (2)
    [255, 255, 0],   // Yellow (3)
    [0, 0, 255],     // Blue (4)
    [0, 255, 0],     // Green (5)
];

const LUT_BITS: usize = 6;
const LUT_SIZE: usize = 1 << (LUT_BITS * 3);
const LUT_MASK_USIZE: usize = (1 << LUT_BITS) - 1;

static COLOR_LUT: OnceLock<Box<[u8]>> = OnceLock::new();
static BLUE_NOISE_BIAS_MASK: OnceLock<Box<[i16]>> = OnceLock::new();
static PALETTE_LAB: OnceLock<Box<[[f32; 3]]>> = OnceLock::new();
static PALETTE_LINEAR: OnceLock<Box<[[f32; 3]]>> = OnceLock::new();
static PALETTE_LUMA: OnceLock<Box<[f32]>> = OnceLock::new();

const BAYER_8X8: [[u8; 8]; 8] = [
    [0, 48, 12, 60, 3, 51, 15, 63],
    [32, 16, 44, 28, 35, 19, 47, 31],
    [8, 56, 4, 52, 11, 59, 7, 55],
    [40, 24, 36, 20, 43, 27, 39, 23],
    [2, 50, 14, 62, 1, 49, 13, 61],
    [34, 18, 46, 30, 33, 17, 45, 29],
    [10, 58, 6, 54, 9, 57, 5, 53],
    [42, 26, 38, 22, 41, 25, 37, 21],
];

const BAYER_STRENGTH: i32 = 48;
const BLUE_NOISE_SIZE: usize = 32;
const BLUE_NOISE_PIXELS: usize = BLUE_NOISE_SIZE * BLUE_NOISE_SIZE;
const BLUE_NOISE_CANDIDATES: usize = 8;
const BLUE_NOISE_STRENGTH: i32 = 44;
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
    colors: [u8; 64], // Stores the sequence of colors for each threshold level (0-63)
}

impl Default for YliluomaMixPlan {
    fn default() -> Self {
        Self { colors: [0; 64] }
    }
}

#[inline(always)]
fn weighted_distance(r: u8, g: u8, b: u8, color: [u8; 3]) -> u32 {
    let dr = r as i32 - color[0] as i32;
    let dg = g as i32 - color[1] as i32;
    let db = b as i32 - color[2] as i32;

    (dr * dr * 299 + dg * dg * 587 + db * db * 114) as u32
}

#[inline(always)]
fn color_lut() -> &'static [u8] {
    COLOR_LUT.get_or_init(|| {
        let mut lut = vec![0u8; LUT_SIZE];

        for packed in 0..LUT_SIZE {
            let r6 = (packed >> 12) & LUT_MASK_USIZE;
            let g6 = (packed >> 6) & LUT_MASK_USIZE;
            let b6 = packed & LUT_MASK_USIZE;

            let r = (r6 << 2) | (r6 >> 4);
            let g = (g6 << 2) | (g6 >> 4);
            let b = (b6 << 2) | (b6 >> 4);

            let mut best_idx = 0u8;
            let mut best_dist = u32::MAX;

            for (idx, color) in PALETTE.iter().enumerate() {
                let dist = weighted_distance(r as u8, g as u8, b as u8, *color);

                if dist < best_dist {
                    best_dist = dist;
                    best_idx = idx as u8;
                }
            }

            lut[packed] = best_idx;
        }

        lut.into_boxed_slice()
    })
}

#[inline(always)]
fn nearest_color_6bit(r6: u8, g6: u8, b6: u8) -> u8 {
    let idx = ((r6 as usize) << 12) | ((g6 as usize) << 6) | (b6 as usize);
    color_lut()[idx]
}

#[inline(always)]
fn nearest_color(r: u8, g: u8, b: u8) -> u8 {
    let r6 = r >> 2;
    let g6 = g >> 2;
    let b6 = b >> 2;
    nearest_color_6bit(r6, g6, b6)
}

pub(crate) fn nearest_palette_index(color: [u8; 3]) -> u8 {
    nearest_color(color[0], color[1], color[2])
}

pub(crate) fn exact_palette_index(color: [u8; 3]) -> Option<u8> {
    PALETTE
        .iter()
        .position(|&palette_color| palette_color == color)
        .map(|idx| idx as u8)
}

#[inline(always)]
fn lab_components_from_rgb(color: [u8; 3]) -> [f32; 3] {
    // Apply gamma decoding and Lab conversion to ensure correct distances and blending
    let lab = Lab::from_rgb(&color);
    [lab.l, lab.a, lab.b]
}

#[inline(always)]
fn srgb_to_linear(channel: u8) -> f32 {
    let value = channel as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[inline(always)]
fn linear_to_srgb(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let srgb = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round().clamp(0.0, 255.0) as u8
}

#[inline(always)]
fn rgb_to_linear_array(color: [u8; 3]) -> [f32; 3] {
    [
        srgb_to_linear(color[0]),
        srgb_to_linear(color[1]),
        srgb_to_linear(color[2]),
    ]
}

#[inline(always)]
fn linear_array_to_lab(linear: [f32; 3]) -> [f32; 3] {
    let srgb = [
        linear_to_srgb(linear[0]),
        linear_to_srgb(linear[1]),
        linear_to_srgb(linear[2]),
    ];
    let lab = Lab::from_rgb(&srgb);
    [lab.l, lab.a, lab.b]
}

#[inline(always)]
fn linear_luma(linear: [f32; 3]) -> f32 {
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

#[inline(always)]
fn ciede2000_distance_sq(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    let (l1, a1, b1) = (lhs[0], lhs[1], lhs[2]);
    let (l2, a2, b2) = (rhs[0], rhs[1], rhs[2]);

    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();
    let avg_c = 0.5 * (c1 + c2);
    let avg_c7 = avg_c.powi(7);
    let g = 0.5 * (1.0 - (avg_c7 / (avg_c7 + 6_103_515_625.0)).sqrt());

    let a1_prime = (1.0 + g) * a1;
    let a2_prime = (1.0 + g) * a2;
    let c1_prime = (a1_prime * a1_prime + b1 * b1).sqrt();
    let c2_prime = (a2_prime * a2_prime + b2 * b2).sqrt();

    fn hue_angle_degrees(b: f32, a: f32) -> f32 {
        let mut angle = b.atan2(a).to_degrees();
        if angle < 0.0 {
            angle += 360.0;
        }
        angle
    }

    let h1_prime = if c1_prime < 1e-9 {
        0.0
    } else {
        hue_angle_degrees(b1, a1_prime)
    };
    let h2_prime = if c2_prime < 1e-9 {
        0.0
    } else {
        hue_angle_degrees(b2, a2_prime)
    };

    let delta_l_prime = l2 - l1;
    let delta_c_prime = c2_prime - c1_prime;

    let delta_h_prime = if c1_prime < 1e-9 || c2_prime < 1e-9 {
        0.0
    } else {
        let mut delta = h2_prime - h1_prime;
        if delta > 180.0 {
            delta -= 360.0;
        } else if delta < -180.0 {
            delta += 360.0;
        }
        delta
    };

    let delta_big_h_prime = 2.0
        * (c1_prime * c2_prime).sqrt()
        * (0.5 * delta_h_prime).to_radians().sin();

    let avg_l_prime = 0.5 * (l1 + l2);
    let avg_c_prime = 0.5 * (c1_prime + c2_prime);

    let avg_h_prime = if c1_prime < 1e-9 || c2_prime < 1e-9 {
        h1_prime + h2_prime
    } else if (h1_prime - h2_prime).abs() > 180.0 {
        if h1_prime + h2_prime < 360.0 {
            0.5 * (h1_prime + h2_prime + 360.0)
        } else {
            0.5 * (h1_prime + h2_prime - 360.0)
        }
    } else {
        0.5 * (h1_prime + h2_prime)
    };

    let t = 1.0
        - 0.17 * (avg_h_prime - 30.0).to_radians().cos()
        + 0.24 * (2.0 * avg_h_prime).to_radians().cos()
        + 0.32 * (3.0 * avg_h_prime + 6.0).to_radians().cos()
        - 0.20 * (4.0 * avg_h_prime - 63.0).to_radians().cos();

    let delta_theta = 30.0 * (-(((avg_h_prime - 275.0) / 25.0).powi(2))).exp();
    let avg_c_prime7 = avg_c_prime.powi(7);
    let r_c = 2.0 * (avg_c_prime7 / (avg_c_prime7 + 6_103_515_625.0)).sqrt();
    let s_l = 1.0 + (0.015 * (avg_l_prime - 50.0).powi(2)) / (20.0 + (avg_l_prime - 50.0).powi(2)).sqrt();
    let s_c = 1.0 + 0.045 * avg_c_prime;
    let s_h = 1.0 + 0.015 * avg_c_prime * t;
    let r_t = -r_c * (2.0 * delta_theta).to_radians().sin();

    let term_l = delta_l_prime / s_l;
    let term_c = delta_c_prime / s_c;
    let term_h = delta_big_h_prime / s_h;

    term_l * term_l + term_c * term_c + term_h * term_h + r_t * term_c * term_h
}

#[inline(always)]
fn palette_lab() -> &'static [[f32; 3]] {
    PALETTE_LAB.get_or_init(|| {
        PALETTE
            .iter()
            .map(|&color| lab_components_from_rgb(color))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
}

#[inline(always)]
fn palette_linear() -> &'static [[f32; 3]] {
    PALETTE_LINEAR.get_or_init(|| {
        PALETTE
            .iter()
            .map(|&color| rgb_to_linear_array(color))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
}

#[inline(always)]
fn palette_luma() -> &'static [f32] {
    PALETTE_LUMA.get_or_init(|| {
        palette_linear()
            .iter()
            .map(|&color| linear_luma(color))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
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
            penalty += ciede2000_distance_sq(palette_lab[prev as usize], palette_lab[next as usize]).sqrt();
        }
    }

    penalty
}

fn top_palette_candidates(target_lab: [f32; 3], limit: usize) -> Vec<u8> {
    let palette_lab = palette_lab();
    let mut ranked = (0..PALETTE.len())
        .map(|idx| (idx as u8, ciede2000_distance_sq(target_lab, palette_lab[idx])))
        .collect::<Vec<_>>();
    ranked.sort_by(|lhs, rhs| lhs.1.partial_cmp(&rhs.1).unwrap_or(std::cmp::Ordering::Equal));
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
        component_penalty += ciede2000_distance_sq(target_lab, palette_lab[color as usize]) * weight;
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
    let luma_span = if active_colors > 1 { max_luma - min_luma } else { 0.0 };
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
                    &[(candidates[0], count_a), (candidates[1], count_b), (candidates[2], count_c)],
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

#[inline(always)]
fn clamp_scaled_to_u8(value: i32, scale: i32) -> u8 {
    let clamped = value.clamp(0, 255 * scale);
    ((clamped + scale / 2) / scale) as u8
}

#[inline(always)]
fn distribute_error(error: i32, numerator: i32, denominator: i32) -> i32 {
    let scaled = error * numerator;
    if scaled >= 0 {
        (scaled + denominator / 2) / denominator
    } else {
        (scaled - denominator / 2) / denominator
    }
}

#[inline(always)]
fn bayer_bias(threshold: u8) -> i32 {
    ordered_bias(threshold as u16, 64, BAYER_STRENGTH)
}

#[inline(always)]
fn ordered_bias(rank: u16, levels: i32, strength: i32) -> i32 {
    ((((rank as i32) << 1) - (levels - 1)) * strength) / levels
}

#[inline(always)]
fn apply_bias(channel: u8, bias: i32) -> u8 {
    (channel as i32 + bias).clamp(0, 255) as u8
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[inline(always)]
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[inline(always)]
    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        (z ^ (z >> 31)) as u32
    }

    #[inline(always)]
    fn gen_index(&mut self, upper: usize) -> usize {
        (self.next_u32() as usize) % upper
    }
}

#[inline(always)]
fn nearest_distance_sq(x: usize, y: usize, points: &[(usize, usize)]) -> usize {
    let mut best = usize::MAX;

    for &(px, py) in points {
        let dx = x.abs_diff(px);
        let dy = y.abs_diff(py);
        let dist = dx * dx + dy * dy;
        best = best.min(dist);
    }

    best
}

fn blue_noise_bias_mask() -> &'static [i16] {
    BLUE_NOISE_BIAS_MASK.get_or_init(|| {
        let mut rng = SplitMix64::new(0xC0FEBABE_73EACE06);
        let mut thresholds = vec![0u16; BLUE_NOISE_PIXELS];
        let mut points = Vec::with_capacity(BLUE_NOISE_PIXELS);
        let mut available = Vec::with_capacity(BLUE_NOISE_PIXELS);
        let mut available_index = vec![0usize; BLUE_NOISE_PIXELS];

        for pos in 0..BLUE_NOISE_PIXELS {
            available_index[pos] = pos;
            available.push(pos);
        }

        for rank in 0..BLUE_NOISE_PIXELS {
            let samples = BLUE_NOISE_CANDIDATES.min(available.len()).max(1);
            let mut best_pos = available[0];
            let mut best_dist = 0usize;

            if points.is_empty() {
                best_pos = available[rng.gen_index(available.len())];
            } else {
                for _ in 0..samples {
                    let candidate = available[rng.gen_index(available.len())];
                    let x = candidate & (BLUE_NOISE_SIZE - 1);
                    let y = candidate / BLUE_NOISE_SIZE;
                    let dist = nearest_distance_sq(x, y, &points);

                    if dist > best_dist {
                        best_dist = dist;
                        best_pos = candidate;
                    }
                }
            }

            thresholds[best_pos] = rank as u16;
            points.push((best_pos & (BLUE_NOISE_SIZE - 1), best_pos / BLUE_NOISE_SIZE));

            let remove_idx = available_index[best_pos];
            let tail_idx = available.len() - 1;
            let tail_pos = available[tail_idx];
            available.swap(remove_idx, tail_idx);
            available_index[tail_pos] = remove_idx;
            available.pop();
        }

        thresholds
            .into_iter()
            .map(|rank| ordered_bias(rank, BLUE_NOISE_PIXELS as i32, BLUE_NOISE_STRENGTH) as i16)
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
}

pub(crate) fn quantize_bayer(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    color_lut();

    let width = width as usize;
    let height = height as usize;
    let raw = img.as_raw();
    let mut output = vec![0u8; width * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let src_base = idx * 3;
            let bias = bayer_bias(BAYER_8X8[y & 7][x & 7]);
            let r = apply_bias(raw[src_base], bias);
            let g = apply_bias(raw[src_base + 1], bias);
            let b = apply_bias(raw[src_base + 2], bias);
            output[idx] = nearest_color(r, g, b);
        }
    }

    output
}

pub(crate) fn quantize_blue_noise(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    color_lut();

    let width = width as usize;
    let height = height as usize;
    let raw = img.as_raw();
    let mask = blue_noise_bias_mask();
    let mut output = vec![0u8; width * height];

    for y in 0..height {
        let mask_row = (y & (BLUE_NOISE_SIZE - 1)) * BLUE_NOISE_SIZE;

        for x in 0..width {
            let idx = y * width + x;
            let src_base = idx * 3;
            let bias = mask[mask_row + (x & (BLUE_NOISE_SIZE - 1))] as i32;
            let r = apply_bias(raw[src_base], bias);
            let g = apply_bias(raw[src_base + 1], bias);
            let b = apply_bias(raw[src_base + 2], bias);
            output[idx] = nearest_color(r, g, b);
        }
    }

    output
}

pub(crate) fn quantize_yliluoma(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    color_lut();
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
            let plan = *plan_cache
                .entry(key)
                .or_insert_with(|| make_yliluoma_plan(raw[src_base], raw[src_base + 1], raw[src_base + 2]));
            let threshold = BAYER_8X8[y & 7][x & 7] as usize;

            output[idx] = plan.colors[threshold];
        }
    }

    output
}

pub(crate) fn quantize_atkinson(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    color_lut();

    let width = width as usize;
    let height = height as usize;
    let total = width * height;
    let raw = img.as_raw();
    let mut output = vec![0u8; total];
    let mut curr_err = vec![0i32; width * 3];
    let mut next_err = vec![0i32; width * 3];
    let mut next_next_err = vec![0i32; width * 3];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let src_base = idx * 3;
            let err_base = x * 3;
            let pixel = [
                raw[src_base] as i32 * 8 + curr_err[err_base],
                raw[src_base + 1] as i32 * 8 + curr_err[err_base + 1],
                raw[src_base + 2] as i32 * 8 + curr_err[err_base + 2],
            ];
            let r = clamp_scaled_to_u8(pixel[0], 8);
            let g = clamp_scaled_to_u8(pixel[1], 8);
            let b = clamp_scaled_to_u8(pixel[2], 8);

            let color_idx = nearest_color(r, g, b);
            let new_color = PALETTE[color_idx as usize];

            output[idx] = color_idx;

            let error = [
                pixel[0] - new_color[0] as i32 * 8,
                pixel[1] - new_color[1] as i32 * 8,
                pixel[2] - new_color[2] as i32 * 8,
            ];

            if x + 1 < width {
                let right = (x + 1) * 3;
                curr_err[right] += distribute_error(error[0], 1, 8);
                curr_err[right + 1] += distribute_error(error[1], 1, 8);
                curr_err[right + 2] += distribute_error(error[2], 1, 8);
            }

            if x + 2 < width {
                let right2 = (x + 2) * 3;
                curr_err[right2] += distribute_error(error[0], 1, 8);
                curr_err[right2 + 1] += distribute_error(error[1], 1, 8);
                curr_err[right2 + 2] += distribute_error(error[2], 1, 8);
            }

            if y + 1 < height {
                if x > 0 {
                    let dl = (x - 1) * 3;
                    next_err[dl] += distribute_error(error[0], 1, 8);
                    next_err[dl + 1] += distribute_error(error[1], 1, 8);
                    next_err[dl + 2] += distribute_error(error[2], 1, 8);
                }

                next_err[err_base] += distribute_error(error[0], 1, 8);
                next_err[err_base + 1] += distribute_error(error[1], 1, 8);
                next_err[err_base + 2] += distribute_error(error[2], 1, 8);

                if x + 1 < width {
                    let dr = (x + 1) * 3;
                    next_err[dr] += distribute_error(error[0], 1, 8);
                    next_err[dr + 1] += distribute_error(error[1], 1, 8);
                    next_err[dr + 2] += distribute_error(error[2], 1, 8);
                }
            }

            if y + 2 < height {
                next_next_err[err_base] += distribute_error(error[0], 1, 8);
                next_next_err[err_base + 1] += distribute_error(error[1], 1, 8);
                next_next_err[err_base + 2] += distribute_error(error[2], 1, 8);
            }
        }

        curr_err.fill(0);
        std::mem::swap(&mut curr_err, &mut next_err);
        std::mem::swap(&mut next_err, &mut next_next_err);
        next_next_err.fill(0);
    }

    output
}
