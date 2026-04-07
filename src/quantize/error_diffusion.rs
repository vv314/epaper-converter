use image::RgbImage;

use super::color::{nearest_color, warm_up_color_lut, PALETTE};

#[derive(Clone, Copy)]
struct ErrorTap {
    dx: i32,
    dy: usize,
    weight: i32,
}

const ATKINSON_TAPS: [ErrorTap; 6] = [
    ErrorTap {
        dx: 1,
        dy: 0,
        weight: 1,
    },
    ErrorTap {
        dx: 2,
        dy: 0,
        weight: 1,
    },
    ErrorTap {
        dx: -1,
        dy: 1,
        weight: 1,
    },
    ErrorTap {
        dx: 0,
        dy: 1,
        weight: 1,
    },
    ErrorTap {
        dx: 1,
        dy: 1,
        weight: 1,
    },
    ErrorTap {
        dx: 0,
        dy: 2,
        weight: 1,
    },
];

const BURKES_TAPS: [ErrorTap; 7] = [
    ErrorTap {
        dx: 1,
        dy: 0,
        weight: 8,
    },
    ErrorTap {
        dx: 2,
        dy: 0,
        weight: 4,
    },
    ErrorTap {
        dx: -2,
        dy: 1,
        weight: 2,
    },
    ErrorTap {
        dx: -1,
        dy: 1,
        weight: 4,
    },
    ErrorTap {
        dx: 0,
        dy: 1,
        weight: 8,
    },
    ErrorTap {
        dx: 1,
        dy: 1,
        weight: 4,
    },
    ErrorTap {
        dx: 2,
        dy: 1,
        weight: 2,
    },
];

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
    quantize_error_diffusion(img, width, height, &ATKINSON_TAPS, 8)
}

pub(crate) fn quantize_burkes(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    quantize_error_diffusion(img, width, height, &BURKES_TAPS, 32)
}

fn process_error_diffusion_pixel(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    raw: &[u8],
    output: &mut [u8],
    error_rows: &mut [Vec<i32>],
    taps: &[ErrorTap],
    denominator: i32,
    mirror_horizontally: bool,
) {
    let idx = y * width + x;
    let src_base = idx * 3;
    let err_base = x * 3;
    let pixel = [
        raw[src_base] as i32 * denominator + error_rows[0][err_base],
        raw[src_base + 1] as i32 * denominator + error_rows[0][err_base + 1],
        raw[src_base + 2] as i32 * denominator + error_rows[0][err_base + 2],
    ];
    let r = clamp_scaled_to_u8(pixel[0], denominator);
    let g = clamp_scaled_to_u8(pixel[1], denominator);
    let b = clamp_scaled_to_u8(pixel[2], denominator);

    let color_idx = nearest_color(r, g, b);
    let new_color = PALETTE[color_idx as usize];

    output[idx] = color_idx;

    let error = [
        pixel[0] - new_color[0] as i32 * denominator,
        pixel[1] - new_color[1] as i32 * denominator,
        pixel[2] - new_color[2] as i32 * denominator,
    ];

    for tap in taps {
        if y + tap.dy >= height {
            continue;
        }

        let dx = if mirror_horizontally { -tap.dx } else { tap.dx };
        let target_x = x as i32 + dx;
        if !(0..width as i32).contains(&target_x) {
            continue;
        }

        let target_base = target_x as usize * 3;
        error_rows[tap.dy][target_base] += distribute_error(error[0], tap.weight, denominator);
        error_rows[tap.dy][target_base + 1] += distribute_error(error[1], tap.weight, denominator);
        error_rows[tap.dy][target_base + 2] += distribute_error(error[2], tap.weight, denominator);
    }
}

fn quantize_error_diffusion(
    img: &RgbImage,
    width: u32,
    height: u32,
    taps: &[ErrorTap],
    denominator: i32,
) -> Vec<u8> {
    warm_up_color_lut();

    let width = width as usize;
    let height = height as usize;
    let total = width * height;
    let raw = img.as_raw();
    let mut output = vec![0u8; total];
    let max_dy = taps.iter().map(|tap| tap.dy).max().unwrap_or(0);
    let mut error_rows = vec![vec![0i32; width * 3]; max_dy + 1];

    for y in 0..height {
        let serpentine = y % 2 == 1;

        for step in 0..width {
            let x = if serpentine { width - 1 - step } else { step };
            process_error_diffusion_pixel(
                x,
                y,
                width,
                height,
                raw,
                &mut output,
                &mut error_rows,
                taps,
                denominator,
                serpentine,
            );
        }

        error_rows[0].fill(0);
        error_rows.rotate_left(1);
        error_rows.last_mut().unwrap().fill(0);
    }

    output
}
