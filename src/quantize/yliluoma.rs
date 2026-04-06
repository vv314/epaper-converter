use image::RgbImage;
use std::sync::OnceLock;

use super::color::{ciede2000_distance_sq, lab_components_from_rgb, palette_luma, PALETTE};
use super::ordered::ordered_threshold_8x8;

const YLILUOMA_CACHE_BITS: u8 = 6;
const YLILUOMA_PLAN_SIZE: usize = 8;
const YLILUOMA_CACHE_SIZE: usize = 1 << ((YLILUOMA_CACHE_BITS as usize) * 3);
const YLILUOMA_CACHE_SENTINEL: u16 = u16::MAX;
const YLILUOMA_SHORTLIST_SIZE: usize = 48;
const YLILUOMA_GAMMA: f32 = 2.2;
const YLILUOMA_LUMA_SPAN_FACTOR: f32 = 7.0;

static YLILUOMA_MIXES: OnceLock<Box<[PrecomputedMix]>> = OnceLock::new();

#[derive(Clone, Copy)]
struct YliluomaMixPlan {
    len: u8,
    colors: [u8; YLILUOMA_PLAN_SIZE],
}

impl YliluomaMixPlan {
    #[inline(always)]
    fn color_at_threshold(&self, threshold: usize) -> u8 {
        let len = self.len as usize;
        let idx = (threshold * len) / 64;
        self.colors[idx.min(len.saturating_sub(1))]
    }
}

#[derive(Clone, Copy)]
struct PrecomputedMix {
    mixed_lab: [f32; 3],
    plan: YliluomaMixPlan,
}

#[inline(always)]
fn yliluoma_cache_key(r: u8, g: u8, b: u8) -> usize {
    let shift = 8 - YLILUOMA_CACHE_BITS;
    (((r >> shift) as usize) << ((YLILUOMA_CACHE_BITS as usize) * 2))
        | (((g >> shift) as usize) << (YLILUOMA_CACHE_BITS as usize))
        | (b >> shift) as usize
}

#[inline(always)]
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
    let luma = palette_luma();
    indices.sort_by(|lhs, rhs| luma[*lhs as usize].total_cmp(&luma[*rhs as usize]));
    indices
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
    let luma = palette_luma();
    let mut total_gap = 0.0f32;
    let mut gap_count = 0usize;

    for window in luma_order.windows(2) {
        let lhs = luma[window[0] as usize];
        let rhs = luma[window[1] as usize];
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
    luma_limit: f32,
    mixes: &mut Vec<PrecomputedMix>,
) {
    let inv_total = 1.0 / total as f32;
    let palette_luma = palette_luma();
    let mut mixed_gamma = [0.0f32; 3];
    let mut colors = [0u8; YLILUOMA_PLAN_SIZE];
    let mut len = 0usize;
    let mut min_luma = f32::INFINITY;
    let mut max_luma = f32::NEG_INFINITY;

    for &palette_idx in luma_order {
        let count = counts[palette_idx as usize] as usize;
        if count == 0 {
            continue;
        }

        let luma = palette_luma[palette_idx as usize];
        min_luma = min_luma.min(luma);
        max_luma = max_luma.max(luma);

        for channel in 0..3 {
            mixed_gamma[channel] += palette_gamma[palette_idx as usize][channel] * count as f32 * inv_total;
        }

        for _ in 0..count {
            colors[len] = palette_idx;
            len += 1;
        }
    }

    debug_assert_eq!(len, total);

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
        plan: YliluomaMixPlan {
            len: total as u8,
            colors,
        },
    });
}

fn build_mix_combinations(
    palette_idx: usize,
    remaining: usize,
    counts: &mut [u8; PALETTE.len()],
    luma_order: &[u8],
    palette_gamma: &[[f32; 3]; PALETTE.len()],
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
            luma_limit,
            mixes,
        );
    }
}

fn yliluoma_mixes() -> &'static [PrecomputedMix] {
    YLILUOMA_MIXES.get_or_init(|| {
        let luma_order = luma_sorted_palette_indices();
        let palette_gamma = gamma_corrected_palette();
        let luma_limit = luma_span_limit(&luma_order);
        let mut mixes = Vec::new();
        let mut counts = [0u8; PALETTE.len()];

        // 参考文档中的“Improved Yliluoma algorithm 1”：
        // 预生成 1..M 个颜色槽位的所有混色组合，并把槽位按亮度排序。
        // 混色在 gamma=2.2 的空间里完成，再映回 RGB/Lab；
        // 同时加入基于亮度跨度的 psychovisual pruning，过滤掉视觉上过于跳变的组合。
        // 当前调色板只有 6 色，M=8 时组合数量仍然很小，
        // 因此可以直接全量预计算，避免旧实现那种逐像素枚举/评分的重 CPU 热点。
        for total in 1..=YLILUOMA_PLAN_SIZE {
            counts.fill(0);
            build_mix_combinations(
                0,
                total,
                &mut counts,
                &luma_order,
                &palette_gamma,
                luma_limit,
                &mut mixes,
            );
        }

        mixes.into_boxed_slice()
    })
}

#[inline(always)]
fn lab_distance_sq(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    let dl = lhs[0] - rhs[0];
    let da = lhs[1] - rhs[1];
    let db = lhs[2] - rhs[2];
    dl * dl + da * da + db * db
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

pub(crate) fn quantize_yliluoma(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    let mixes = yliluoma_mixes();
    let width = width as usize;
    let height = height as usize;
    let raw = img.as_raw();
    let mut output = vec![0u8; width * height];
    let mut plan_cache = vec![YLILUOMA_CACHE_SENTINEL; YLILUOMA_CACHE_SIZE];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let src_base = idx * 3;
            let key = yliluoma_cache_key(raw[src_base], raw[src_base + 1], raw[src_base + 2]);

            if plan_cache[key] == YLILUOMA_CACHE_SENTINEL {
                plan_cache[key] = best_mix_index_for_key(key, mixes);
            }

            let plan = mixes[plan_cache[key] as usize].plan;
            output[idx] = plan.color_at_threshold(ordered_threshold_8x8(x, y));
        }
    }

    output
}
