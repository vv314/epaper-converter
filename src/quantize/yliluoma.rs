use image::RgbImage;
use std::sync::OnceLock;

use super::ordered::ordered_threshold_8x8;
use super::palette::{palette_luma, PALETTE};

const YLILUOMA_CACHE_BITS: u8 = 6;
const YLILUOMA_PLAN_SIZE: usize = 8;
const YLILUOMA_CACHE_SIZE: usize = 1 << ((YLILUOMA_CACHE_BITS as usize) * 3);
const YLILUOMA_LUMA_SPAN_FACTOR: f32 = 7.0;

static YLILUOMA_MIXES: OnceLock<Box<[PrecomputedMix]>> = OnceLock::new();
static YLILUOMA_BEST_MIX_LUT: OnceLock<Box<[u16]>> = OnceLock::new();

#[derive(Clone, Copy)]
struct YliluomaMixPlan {
    threshold_colors: [u8; 64],
}

impl YliluomaMixPlan {
    #[inline(always)]
    fn from_sorted_colors(colors: [u8; YLILUOMA_PLAN_SIZE], len: usize) -> Self {
        let mut threshold_colors = [0u8; 64];
        let last_idx = len.saturating_sub(1);

        for (threshold, entry) in threshold_colors.iter_mut().enumerate() {
            let idx = (threshold * len) / 64;
            *entry = colors[idx.min(last_idx)];
        }

        Self { threshold_colors }
    }

    #[inline(always)]
    fn color_at_threshold(&self, threshold: usize) -> u8 {
        self.threshold_colors[threshold]
    }
}

#[derive(Clone, Copy)]
struct PrecomputedMix {
    plan: YliluomaMixPlan,
}

#[inline(always)]
fn yliluoma_cache_key(r: u8, g: u8, b: u8) -> usize {
    let shift = 8 - YLILUOMA_CACHE_BITS;
    (((r >> shift) as usize) << ((YLILUOMA_CACHE_BITS as usize) * 2))
        | (((g >> shift) as usize) << (YLILUOMA_CACHE_BITS as usize))
        | (b >> shift) as usize
}

fn yliluoma_best_mix_lut() -> &'static [u16] {
    const LUT_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/yliluoma_best_mix_lut.bin"));

    YLILUOMA_BEST_MIX_LUT.get_or_init(|| {
        debug_assert_eq!(
            LUT_BYTES.len(),
            YLILUOMA_CACHE_SIZE * std::mem::size_of::<u16>()
        );
        LUT_BYTES
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
}

fn luma_sorted_palette_indices() -> Vec<u8> {
    let mut indices = (0..PALETTE.len() as u8).collect::<Vec<_>>();
    let luma = palette_luma();
    indices.sort_by(|lhs, rhs| luma[*lhs as usize].total_cmp(&luma[*rhs as usize]));
    indices
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
    luma_limit: f32,
    mixes: &mut Vec<PrecomputedMix>,
) {
    let palette_luma = palette_luma();
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

        for _ in 0..count {
            colors[len] = palette_idx;
            len += 1;
        }
    }

    debug_assert_eq!(len, total);

    if len > 1 && (max_luma - min_luma) > luma_limit {
        return;
    }

    mixes.push(PrecomputedMix {
        plan: YliluomaMixPlan::from_sorted_colors(colors, total),
    });
}

fn build_mix_combinations(
    palette_idx: usize,
    remaining: usize,
    counts: &mut [u8; PALETTE.len()],
    luma_order: &[u8],
    luma_limit: f32,
    mixes: &mut Vec<PrecomputedMix>,
) {
    if palette_idx + 1 == PALETTE.len() {
        counts[palette_idx] = remaining as u8;
        push_precomputed_mix(
            counts,
            counts.iter().map(|&count| count as usize).sum(),
            luma_order,
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
            luma_limit,
            mixes,
        );
    }
}

fn yliluoma_mixes() -> &'static [PrecomputedMix] {
    YLILUOMA_MIXES.get_or_init(|| {
        let luma_order = luma_sorted_palette_indices();
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
            build_mix_combinations(0, total, &mut counts, &luma_order, luma_limit, &mut mixes);
        }

        mixes.into_boxed_slice()
    })
}

pub(crate) fn quantize_yliluoma(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    let mixes = yliluoma_mixes();
    let best_mix_lut = yliluoma_best_mix_lut();
    let width = width as usize;
    let height = height as usize;
    let raw = img.as_raw();
    let mut output = vec![0u8; width * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let src_base = idx * 3;
            let key = yliluoma_cache_key(raw[src_base], raw[src_base + 1], raw[src_base + 2]);
            let plan = mixes[best_mix_lut[key] as usize].plan;
            output[idx] = plan.color_at_threshold(ordered_threshold_8x8(x, y));
        }
    }

    output
}
