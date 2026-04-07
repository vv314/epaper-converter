use image::RgbImage;
use std::sync::OnceLock;

use super::ordered::{apply_bias, ordered_bias};
use super::palette::{nearest_color, warm_up_color_lut};

const BLUE_NOISE_SIZE: usize = 32;
const BLUE_NOISE_PIXELS: usize = BLUE_NOISE_SIZE * BLUE_NOISE_SIZE;
const BLUE_NOISE_CANDIDATES: usize = 8;
const BLUE_NOISE_STRENGTH: i32 = 44;

static BLUE_NOISE_BIAS_MASK: OnceLock<Box<[i16]>> = OnceLock::new();

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

pub(crate) fn quantize_blue_noise(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    warm_up_color_lut();

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
