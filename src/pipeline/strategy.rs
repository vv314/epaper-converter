use image::RgbImage;

use crate::cli::HalftoneMode;

pub(crate) fn choose_halftone_mode(img: &RgbImage) -> HalftoneMode {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let total = width * height;
    let step = ((total / 4096).max(1) as f64).sqrt().floor() as usize;
    let step = step.max(1);

    let mut buckets = [false; 4096];
    let mut unique_count = 0usize;
    let mut diff_sum = 0u64;
    let mut diff_samples = 0u64;

    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let pixel = img.get_pixel(x as u32, y as u32);
            let bucket = ((pixel[0] >> 4) as usize) << 8
                | ((pixel[1] >> 4) as usize) << 4
                | (pixel[2] >> 4) as usize;
            if !buckets[bucket] {
                buckets[bucket] = true;
                unique_count += 1;
            }

            if x + step < width {
                let other = img.get_pixel((x + step) as u32, y as u32);
                diff_sum += (pixel[0].abs_diff(other[0]) as u64)
                    + (pixel[1].abs_diff(other[1]) as u64)
                    + (pixel[2].abs_diff(other[2]) as u64);
                diff_samples += 1;
            }
            if y + step < height {
                let other = img.get_pixel(x as u32, (y + step) as u32);
                diff_sum += (pixel[0].abs_diff(other[0]) as u64)
                    + (pixel[1].abs_diff(other[1]) as u64)
                    + (pixel[2].abs_diff(other[2]) as u64);
                diff_samples += 1;
            }
        }
    }

    let avg_diff = if diff_samples == 0 {
        0.0
    } else {
        diff_sum as f64 / diff_samples as f64
    };

    if unique_count <= 8 && avg_diff < 12.0 {
        HalftoneMode::Bayer
    } else if unique_count > 96 || avg_diff > 48.0 {
        HalftoneMode::Atkinson
    } else {
        HalftoneMode::Yliluoma
    }
}
