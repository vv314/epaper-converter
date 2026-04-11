use image::RgbImage;

use super::diffusion::{quantize_error_diffusion, ErrorTap};

const FLOYD_STEINBERG_TAPS: [ErrorTap; 4] = [
    ErrorTap {
        dx: 1,
        dy: 0,
        weight: 7,
    },
    ErrorTap {
        dx: -1,
        dy: 1,
        weight: 3,
    },
    ErrorTap {
        dx: 0,
        dy: 1,
        weight: 5,
    },
    ErrorTap {
        dx: 1,
        dy: 1,
        weight: 1,
    },
];

pub(crate) fn quantize_floyd_steinberg(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    quantize_error_diffusion(img, width, height, &FLOYD_STEINBERG_TAPS, 16)
}
