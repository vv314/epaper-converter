use image::RgbImage;
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
    ((((threshold as i32) << 1) - 63) * BAYER_STRENGTH) / 64
}

#[inline(always)]
fn apply_bias(channel: u8, bias: i32) -> u8 {
    (channel as i32 + bias).clamp(0, 255) as u8
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
