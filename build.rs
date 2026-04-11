use lab::Lab;
use rayon::prelude::*;
use std::env;
use std::fs;
use std::path::PathBuf;

const PALETTE: [[u8; 3]; 6] = [
    [0, 0, 0],
    [255, 255, 255],
    [255, 0, 0],
    [255, 255, 0],
    [0, 0, 255],
    [0, 255, 0],
];

const YLILUOMA_CACHE_BITS: u8 = 6;
const YLILUOMA_PLAN_SIZE: usize = 8;
const YLILUOMA_CACHE_SIZE: usize = 1 << ((YLILUOMA_CACHE_BITS as usize) * 3);
const YLILUOMA_SHORTLIST_SIZE: usize = 48;
const YLILUOMA_GAMMA: f32 = 2.2;
const YLILUOMA_LUMA_SPAN_FACTOR: f32 = 7.0;

#[derive(Clone, Copy)]
struct PrecomputedMix {
    mixed_lab: [f32; 3],
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let output_path = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR missing"))
        .join("yliluoma_best_mix_lut.bin");

    let mixes = yliluoma_mixes();
    let lut = (0..YLILUOMA_CACHE_SIZE)
        .into_par_iter()
        .map(|key| best_mix_index_for_key(key, &mixes))
        .collect::<Vec<_>>();

    let mut bytes = Vec::with_capacity(lut.len() * 2);
    for mix_idx in lut {
        bytes.extend_from_slice(&mix_idx.to_le_bytes());
    }

    fs::write(output_path, bytes).expect("failed to write yliluoma LUT");
}

#[inline(always)]
fn lab_components_from_rgb(color: [u8; 3]) -> [f32; 3] {
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
fn linear_luma(color: [u8; 3]) -> f32 {
    let linear = [
        srgb_to_linear(color[0]),
        srgb_to_linear(color[1]),
        srgb_to_linear(color[2]),
    ];
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

    let delta_big_h_prime =
        2.0 * (c1_prime * c2_prime).sqrt() * (0.5 * delta_h_prime).to_radians().sin();

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

    let t = 1.0 - 0.17 * (avg_h_prime - 30.0).to_radians().cos()
        + 0.24 * (2.0 * avg_h_prime).to_radians().cos()
        + 0.32 * (3.0 * avg_h_prime + 6.0).to_radians().cos()
        - 0.20 * (4.0 * avg_h_prime - 63.0).to_radians().cos();

    let delta_theta = 30.0 * (-(((avg_h_prime - 275.0) / 25.0).powi(2))).exp();
    let avg_c_prime7 = avg_c_prime.powi(7);
    let r_c = 2.0 * (avg_c_prime7 / (avg_c_prime7 + 6_103_515_625.0)).sqrt();
    let s_l =
        1.0 + (0.015 * (avg_l_prime - 50.0).powi(2)) / (20.0 + (avg_l_prime - 50.0).powi(2)).sqrt();
    let s_c = 1.0 + 0.045 * avg_c_prime;
    let s_h = 1.0 + 0.015 * avg_c_prime * t;
    let r_t = -r_c * (2.0 * delta_theta).to_radians().sin();

    let term_l = delta_l_prime / s_l;
    let term_c = delta_c_prime / s_c;
    let term_h = delta_big_h_prime / s_h;

    term_l * term_l + term_c * term_c + term_h * term_h + r_t * term_c * term_h
}

#[inline(always)]
fn lab_distance_sq(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    let dl = lhs[0] - rhs[0];
    let da = lhs[1] - rhs[1];
    let db = lhs[2] - rhs[2];
    dl * dl + da * da + db * db
}

#[inline(always)]
fn gamma_correct(channel: u8) -> f32 {
    (channel as f32 / 255.0).powf(YLILUOMA_GAMMA)
}

#[inline(always)]
fn gamma_uncorrect(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    (value.powf(1.0 / YLILUOMA_GAMMA) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn yliluoma_cache_rgb(key: usize) -> [u8; 3] {
    let mask = (1usize << (YLILUOMA_CACHE_BITS as usize)) - 1;
    let b6 = key & mask;
    let g6 = (key >> (YLILUOMA_CACHE_BITS as usize)) & mask;
    let r6 = (key >> ((YLILUOMA_CACHE_BITS as usize) * 2)) & mask;

    let expand = |v: usize| {
        let v = v as u8;
        (v << 2) | (v >> 4)
    };

    [expand(r6), expand(g6), expand(b6)]
}

fn luma_sorted_palette_indices() -> Vec<u8> {
    let mut indices = (0..PALETTE.len() as u8).collect::<Vec<_>>();
    let palette_luma = PALETTE.map(linear_luma);
    indices.sort_by(|lhs, rhs| palette_luma[*lhs as usize].total_cmp(&palette_luma[*rhs as usize]));
    indices
}

fn gamma_corrected_palette() -> [[f32; 3]; PALETTE.len()] {
    let mut corrected = [[0.0; 3]; PALETTE.len()];
    for (idx, color) in PALETTE.iter().enumerate() {
        corrected[idx] = [
            gamma_correct(color[0]),
            gamma_correct(color[1]),
            gamma_correct(color[2]),
        ];
    }
    corrected
}

fn luma_span_limit(luma_order: &[u8]) -> f32 {
    let palette_luma = PALETTE.map(linear_luma);
    let mut total_gap = 0.0f32;
    let mut gap_count = 0usize;

    for window in luma_order.windows(2) {
        let lhs = palette_luma[window[0] as usize];
        let rhs = palette_luma[window[1] as usize];
        total_gap += rhs - lhs;
        gap_count += 1;
    }

    if gap_count == 0 {
        f32::INFINITY
    } else {
        (total_gap / gap_count as f32) * YLILUOMA_LUMA_SPAN_FACTOR
    }
}

fn push_precomputed_mix(
    counts: &[u8; PALETTE.len()],
    total: usize,
    luma_order: &[u8],
    palette_gamma: &[[f32; 3]; PALETTE.len()],
    palette_luma: &[f32; PALETTE.len()],
    luma_limit: f32,
    mixes: &mut Vec<PrecomputedMix>,
) {
    let inv_total = 1.0 / total as f32;
    let mut mixed_gamma = [0.0f32; 3];
    let mut min_luma = f32::INFINITY;
    let mut max_luma = f32::NEG_INFINITY;
    let mut len = 0usize;

    for &palette_idx in luma_order {
        let count = counts[palette_idx as usize] as usize;
        if count == 0 {
            continue;
        }

        let luma = palette_luma[palette_idx as usize];
        min_luma = min_luma.min(luma);
        max_luma = max_luma.max(luma);
        len += count;

        for channel in 0..3 {
            mixed_gamma[channel] +=
                palette_gamma[palette_idx as usize][channel] * count as f32 * inv_total;
        }
    }

    if len > 1 && (max_luma - min_luma) > luma_limit {
        return;
    }

    let mixed_rgb = [
        gamma_uncorrect(mixed_gamma[0]),
        gamma_uncorrect(mixed_gamma[1]),
        gamma_uncorrect(mixed_gamma[2]),
    ];

    mixes.push(PrecomputedMix {
        mixed_lab: lab_components_from_rgb(mixed_rgb),
    });
}

fn build_mix_combinations(
    palette_idx: usize,
    remaining: usize,
    counts: &mut [u8; PALETTE.len()],
    luma_order: &[u8],
    palette_gamma: &[[f32; 3]; PALETTE.len()],
    palette_luma: &[f32; PALETTE.len()],
    luma_limit: f32,
    mixes: &mut Vec<PrecomputedMix>,
) {
    if palette_idx + 1 == PALETTE.len() {
        counts[palette_idx] = remaining as u8;
        push_precomputed_mix(
            counts,
            counts.iter().map(|&count| count as usize).sum(),
            luma_order,
            palette_gamma,
            palette_luma,
            luma_limit,
            mixes,
        );
        return;
    }

    for count in 0..=remaining {
        counts[palette_idx] = count as u8;
        build_mix_combinations(
            palette_idx + 1,
            remaining - count,
            counts,
            luma_order,
            palette_gamma,
            palette_luma,
            luma_limit,
            mixes,
        );
    }
}

fn yliluoma_mixes() -> Vec<PrecomputedMix> {
    let luma_order = luma_sorted_palette_indices();
    let palette_gamma = gamma_corrected_palette();
    let palette_luma = PALETTE.map(linear_luma);
    let luma_limit = luma_span_limit(&luma_order);
    let mut mixes = Vec::new();
    let mut counts = [0u8; PALETTE.len()];

    for total in 1..=YLILUOMA_PLAN_SIZE {
        counts.fill(0);
        build_mix_combinations(
            0,
            total,
            &mut counts,
            &luma_order,
            &palette_gamma,
            &palette_luma,
            luma_limit,
            &mut mixes,
        );
    }

    mixes
}

fn best_mix_index_for_key(key: usize, mixes: &[PrecomputedMix]) -> u16 {
    let target_lab = lab_components_from_rgb(yliluoma_cache_rgb(key));
    let mut shortlist_indices = [0u16; YLILUOMA_SHORTLIST_SIZE];
    let mut shortlist_scores = [f32::INFINITY; YLILUOMA_SHORTLIST_SIZE];
    let mut shortlist_len = 0usize;
    let mut worst_slot = 0usize;
    let mut best_index = 0u16;
    let mut best_score = f32::INFINITY;

    for (idx, mix) in mixes.iter().enumerate() {
        let approx_score = lab_distance_sq(target_lab, mix.mixed_lab);

        if shortlist_len < YLILUOMA_SHORTLIST_SIZE {
            shortlist_indices[shortlist_len] = idx as u16;
            shortlist_scores[shortlist_len] = approx_score;
            if approx_score > shortlist_scores[worst_slot] {
                worst_slot = shortlist_len;
            }
            shortlist_len += 1;
            continue;
        }

        if approx_score < shortlist_scores[worst_slot] {
            shortlist_indices[worst_slot] = idx as u16;
            shortlist_scores[worst_slot] = approx_score;

            worst_slot = 0;
            for slot in 1..YLILUOMA_SHORTLIST_SIZE {
                if shortlist_scores[slot] > shortlist_scores[worst_slot] {
                    worst_slot = slot;
                }
            }
        }
    }

    for &mix_idx in shortlist_indices.iter().take(shortlist_len) {
        let score = ciede2000_distance_sq(target_lab, mixes[mix_idx as usize].mixed_lab);
        if score < best_score {
            best_score = score;
            best_index = mix_idx;
        }
    }

    best_index
}
