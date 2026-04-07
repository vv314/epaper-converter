use image::RgbImage;

use super::diffusion::{quantize_error_diffusion, ErrorTap};

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

pub(crate) fn quantize_atkinson(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    quantize_error_diffusion(img, width, height, &ATKINSON_TAPS, 8)
}
