use image::RgbImage;

use super::ordered::{apply_bias, ordered_bias};
use super::palette::{nearest_color, warm_up_color_lut};

const CLUSTERED_DOT_8X8: [[u8; 8]; 8] = [
    [24, 10, 12, 26, 35, 47, 49, 37],
    [8, 0, 2, 14, 45, 59, 61, 51],
    [22, 6, 4, 16, 43, 57, 63, 53],
    [30, 20, 18, 28, 33, 41, 55, 39],
    [34, 46, 48, 36, 25, 11, 13, 27],
    [44, 58, 60, 50, 9, 1, 3, 15],
    [42, 56, 62, 54, 23, 7, 5, 17],
    [32, 40, 52, 38, 31, 21, 19, 29],
];
const CLUSTERED_DOT_STRENGTH: i32 = 52;

#[inline(always)]
fn clustered_dot_bias(threshold: u8) -> i32 {
    ordered_bias(threshold as u16, 64, CLUSTERED_DOT_STRENGTH)
}

pub(crate) fn quantize_clustered_dot(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    warm_up_color_lut();

    let width = width as usize;
    let height = height as usize;
    let raw = img.as_raw();
    let mut output = vec![0u8; width * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let src_base = idx * 3;
            let bias = clustered_dot_bias(CLUSTERED_DOT_8X8[y & 7][x & 7]);
            let r = apply_bias(raw[src_base], bias);
            let g = apply_bias(raw[src_base + 1], bias);
            let b = apply_bias(raw[src_base + 2], bias);
            output[idx] = nearest_color(r, g, b);
        }
    }

    output
}
