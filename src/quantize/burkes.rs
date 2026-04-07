use image::RgbImage;

use super::diffusion::{quantize_error_diffusion, ErrorTap};

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

pub(crate) fn quantize_burkes(img: &RgbImage, width: u32, height: u32) -> Vec<u8> {
    quantize_error_diffusion(img, width, height, &BURKES_TAPS, 32)
}
