use image::RgbImage;

use crate::quantize::{exact_palette_index, nearest_palette_index};

pub(crate) fn palette_histogram_nearest(img: &RgbImage) -> [u64; 6] {
    let mut counts = [0u64; 6];

    for pixel in img.pixels() {
        let idx = nearest_palette_index(pixel.0) as usize;
        counts[idx] += 1;
    }

    counts
}

pub(crate) fn palette_histogram_exact(img: &RgbImage) -> ([u64; 6], u64) {
    let mut counts = [0u64; 6];
    let mut invalid = 0u64;

    for pixel in img.pixels() {
        match exact_palette_index(pixel.0) {
            Some(idx) => counts[idx as usize] += 1,
            None => invalid += 1,
        }
    }

    (counts, invalid)
}
