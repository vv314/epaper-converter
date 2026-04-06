use image::RgbImage;

use super::color::{nearest_color, warm_up_color_lut, PALETTE};

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

pub(crate) fn quantize_atkinson(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    warm_up_color_lut();

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
