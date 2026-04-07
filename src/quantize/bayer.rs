use image::RgbImage;

use super::ordered::{apply_bias, ordered_bias, BAYER_8X8};
use super::palette::{nearest_color, warm_up_color_lut};

const BAYER_STRENGTH: i32 = 48;

#[inline(always)]
fn bayer_bias(threshold: u8) -> i32 {
    ordered_bias(threshold as u16, 64, BAYER_STRENGTH)
}

pub(crate) fn quantize_bayer(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    warm_up_color_lut();

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
