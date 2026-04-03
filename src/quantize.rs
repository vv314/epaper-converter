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
fn clamp_fixed_to_u8(value: i32) -> u8 {
    let clamped = value.clamp(0, 255 * 16);
    ((clamped + 8) >> 4) as u8
}

#[inline(always)]
fn diffuse_error(error: i32, weight: i32) -> i32 {
    let scaled = error * weight;
    if scaled >= 0 {
        (scaled + 8) >> 4
    } else {
        (scaled - 8) >> 4
    }
}

#[inline(always)]
fn quantize_scanline_into(pixels: &[u8], output: &mut [u8]) {
    let width = output.len();
    let chunks = width / 4;
    let remainder = width % 4;

    for chunk in 0..chunks {
        let base = chunk * 4 * 3;
        let p0 = unsafe {
            let r = *pixels.get_unchecked(base);
            let g = *pixels.get_unchecked(base + 1);
            let b = *pixels.get_unchecked(base + 2);
            nearest_color(r, g, b)
        };
        let p1 = unsafe {
            let r = *pixels.get_unchecked(base + 3);
            let g = *pixels.get_unchecked(base + 4);
            let b = *pixels.get_unchecked(base + 5);
            nearest_color(r, g, b)
        };
        let p2 = unsafe {
            let r = *pixels.get_unchecked(base + 6);
            let g = *pixels.get_unchecked(base + 7);
            let b = *pixels.get_unchecked(base + 8);
            nearest_color(r, g, b)
        };
        let p3 = unsafe {
            let r = *pixels.get_unchecked(base + 9);
            let g = *pixels.get_unchecked(base + 10);
            let b = *pixels.get_unchecked(base + 11);
            nearest_color(r, g, b)
        };

        let out_base = chunk * 4;
        output[out_base] = p0;
        output[out_base + 1] = p1;
        output[out_base + 2] = p2;
        output[out_base + 3] = p3;
    }

    for i in 0..remainder {
        let base = chunks * 4 * 3 + i * 3;
        output[chunks * 4 + i] = nearest_color(pixels[base], pixels[base + 1], pixels[base + 2]);
    }
}

pub(crate) fn quantize_fast(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    color_lut();

    let total = (width * height) as usize;
    let mut output = vec![0u8; total];
    let row_bytes = width as usize * 3;

    for (src_row, dst_row) in img
        .as_raw()
        .chunks_exact(row_bytes)
        .zip(output.chunks_exact_mut(width as usize))
    {
        quantize_scanline_into(src_row, dst_row);
    }

    output
}

pub(crate) fn quantize_dithered(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    color_lut();

    let width = width as usize;
    let height = height as usize;
    let total = width * height;
    let raw = img.as_raw();
    let mut output = vec![0u8; total];
    let mut curr_err = vec![0i32; width * 3];
    let mut next_err = vec![0i32; width * 3];

    for y in 0..height {
        let row_start = y * width;
        let forward = (y & 1) == 0;
        next_err.fill(0);

        if forward {
            for x in 0..width {
                let idx = row_start + x;
                let src_base = idx * 3;
                let err_base = x * 3;
                let pixel = [
                    raw[src_base] as i32 * 16 + curr_err[err_base],
                    raw[src_base + 1] as i32 * 16 + curr_err[err_base + 1],
                    raw[src_base + 2] as i32 * 16 + curr_err[err_base + 2],
                ];
                let r = clamp_fixed_to_u8(pixel[0]);
                let g = clamp_fixed_to_u8(pixel[1]);
                let b = clamp_fixed_to_u8(pixel[2]);

                let color_idx = nearest_color(r, g, b);
                let new_color = PALETTE[color_idx as usize];

                output[idx] = color_idx;

                let error = [
                    pixel[0] - new_color[0] as i32 * 16,
                    pixel[1] - new_color[1] as i32 * 16,
                    pixel[2] - new_color[2] as i32 * 16,
                ];

                if x + 1 < width {
                    let right = (x + 1) * 3;
                    curr_err[right] += diffuse_error(error[0], 7);
                    curr_err[right + 1] += diffuse_error(error[1], 7);
                    curr_err[right + 2] += diffuse_error(error[2], 7);
                }

                if y + 1 < height {
                    if x > 0 {
                        let dl = (x - 1) * 3;
                        next_err[dl] += diffuse_error(error[0], 3);
                        next_err[dl + 1] += diffuse_error(error[1], 3);
                        next_err[dl + 2] += diffuse_error(error[2], 3);
                    }

                    next_err[err_base] += diffuse_error(error[0], 5);
                    next_err[err_base + 1] += diffuse_error(error[1], 5);
                    next_err[err_base + 2] += diffuse_error(error[2], 5);

                    if x + 1 < width {
                        let dr = (x + 1) * 3;
                        next_err[dr] += diffuse_error(error[0], 1);
                        next_err[dr + 1] += diffuse_error(error[1], 1);
                        next_err[dr + 2] += diffuse_error(error[2], 1);
                    }
                }
            }
        } else {
            for x in (0..width).rev() {
                let idx = row_start + x;
                let src_base = idx * 3;
                let err_base = x * 3;
                let pixel = [
                    raw[src_base] as i32 * 16 + curr_err[err_base],
                    raw[src_base + 1] as i32 * 16 + curr_err[err_base + 1],
                    raw[src_base + 2] as i32 * 16 + curr_err[err_base + 2],
                ];
                let r = clamp_fixed_to_u8(pixel[0]);
                let g = clamp_fixed_to_u8(pixel[1]);
                let b = clamp_fixed_to_u8(pixel[2]);

                let color_idx = nearest_color(r, g, b);
                let new_color = PALETTE[color_idx as usize];

                output[idx] = color_idx;

                let error = [
                    pixel[0] - new_color[0] as i32 * 16,
                    pixel[1] - new_color[1] as i32 * 16,
                    pixel[2] - new_color[2] as i32 * 16,
                ];

                if x > 0 {
                    let left = (x - 1) * 3;
                    curr_err[left] += diffuse_error(error[0], 7);
                    curr_err[left + 1] += diffuse_error(error[1], 7);
                    curr_err[left + 2] += diffuse_error(error[2], 7);
                }

                if y + 1 < height {
                    if x + 1 < width {
                        let dr = (x + 1) * 3;
                        next_err[dr] += diffuse_error(error[0], 3);
                        next_err[dr + 1] += diffuse_error(error[1], 3);
                        next_err[dr + 2] += diffuse_error(error[2], 3);
                    }

                    next_err[err_base] += diffuse_error(error[0], 5);
                    next_err[err_base + 1] += diffuse_error(error[1], 5);
                    next_err[err_base + 2] += diffuse_error(error[2], 5);

                    if x > 0 {
                        let dl = (x - 1) * 3;
                        next_err[dl] += diffuse_error(error[0], 1);
                        next_err[dl + 1] += diffuse_error(error[1], 1);
                        next_err[dl + 2] += diffuse_error(error[2], 1);
                    }
                }
            }
        }

        std::mem::swap(&mut curr_err, &mut next_err);
    }

    output
}
